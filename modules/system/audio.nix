# PipeWire. The System/Game/Chat/Music/Browser virtual outputs replace the BEACN
# app's channels and VB-Audio Cable. They all play into your real output, and
# OBS captures each one separately (see streaming/obs.nix for the track map).
# "System" is the default output, so anything you don't route lands there;
# move apps to Game/Chat/Music/Browser in pwvucontrol (it remembers per app).
# Needs audioOutput in the host's host.json (`vx audio devices` lists the names).
# Mic cleanup goes through EasyEffects. Any output device works.
{
  flake.modules.nixos.desktop =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      output = config.vx.host.audioOutput;
      channel = name: priority: {
        name = "libpipewire-module-loopback";
        args = {
          "node.description" = name;
          "capture.props" = {
            "node.name" = "channel_${lib.toLower name}";
            "media.class" = "Audio/Sink";
            "priority.session" = priority; # highest wins the default output
            "audio.position" = [
              "FL"
              "FR"
            ];
          };
          "playback.props" = {
            "node.name" = "channel_${lib.toLower name}_out";
            "node.passive" = true;
            "audio.position" = [
              "FL"
              "FR"
            ];
          }
          // lib.optionalAttrs (output != null) {
            "target.object" = output; # always play into the real device, never another channel
          };
        };
      };
    in
    {
      services.pulseaudio.enable = false;
      security.rtkit.enable = true;
      services.pipewire = {
        enable = true;
        alsa.enable = true;
        alsa.support32Bit = true;
        pulse.enable = true;
        wireplumber.enable = true;
        extraConfig.pipewire."60-virtual-channels"."context.modules" = [
          # System only becomes the default once audioOutput is set;
          # until then it would loop back into itself.
          (channel "System" (if output != null then 2500 else 500))
          (channel "Game" 500)
          (channel "Chat" 500)
          (channel "Music" 500)
          (channel "Browser" 500)
        ];
      };

      # HyperX Cloud Alpha Wireless battery/sidetone (`headsetcontrol -b`).
      services.udev.packages = [ pkgs.headsetcontrol ];
      environment.systemPackages = [ pkgs.headsetcontrol ];
    };

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      # Volume keys and the on-screen level: the shell (desktop/shell.nix). It
      # turns the real device (audioOutput in host.json), so every channel follows.
      services.easyeffects.enable = true;
      home.packages = with pkgs; [
        pwvucontrol
        qpwgraph
        playerctl
      ];
    };
}
