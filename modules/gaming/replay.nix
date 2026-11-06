# Instant replay (like ShadowPlay): gpu-screen-recorder keeps the last two
# minutes of meta.replayMonitor in RAM, encoded on the 7900 XTX (HEVC, near
# zero FPS cost). Alt+F10 saves it to ~/Videos/Replays; Alt+F9 saves only the
# last 30 seconds. Game + voice chat + mic are in the clip.
# `systemctl --user stop gpu-screen-recorder-replay` turns it off for a session.
{ config, ... }:
let
  inherit (config) meta;
  # Everything you hear (the real output's monitor carries every channel) plus your mic.
  audio =
    if meta.audioOutput != null then
      "device:${meta.audioOutput}.monitor|default_input"
    else
      "default_output|default_input";
in
{
  flake.modules.nixos.gaming = {
    programs.gpu-screen-recorder.enable = true; # setcap'd KMS capture helper
  };

  flake.modules.homeManager.gaming =
    { pkgs, ... }:
    let
      onSave = pkgs.writeShellScript "replay-saved" ''
        ${pkgs.libnotify}/bin/notify-send -a "Instant replay" -i media-record "Clip saved" "$(basename "$1")"
      '';
    in
    {
      systemd.user.services.gpu-screen-recorder-replay = {
        Unit = {
          Description = "Instant replay buffer";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" ];
        };
        Service = {
          # /run/wrappers/bin has the capability-enabled gsr-kms-server.
          Environment = "PATH=/run/wrappers/bin:${pkgs.coreutils}/bin";
          ExecStartPre = "${pkgs.coreutils}/bin/mkdir -p %h/Videos/Replays";
          ExecStart = builtins.concatStringsSep " " [
            "${pkgs.gpu-screen-recorder}/bin/gpu-screen-recorder"
            "-w ${meta.replayMonitor}"
            "-f 60"
            "-k hevc"
            "-bm cbr -q 40000"
            "-a '${audio}'"
            "-r 120"
            "-c mp4"
            "-sc ${onSave}"
            "-o %h/Videos/Replays"
          ];
          Restart = "on-failure";
          RestartSec = 5;
        };
        Install.WantedBy = [ "graphical-session.target" ];
      };

      wayland.windowManager.mango.settings.bind = [
        "ALT,F10,spawn,systemctl --user kill --kill-whom=main -s SIGUSR1 gpu-screen-recorder-replay"
        "ALT,F9,spawn,systemctl --user kill --kill-whom=main -s SIGRTMIN+2 gpu-screen-recorder-replay"
      ];
    };
}
