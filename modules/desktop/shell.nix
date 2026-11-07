# vshell: the desktop shell, written in QML for Quickshell (../../shell).
# One program for the bar, notifications, control center (sound, network,
# Bluetooth, media, quick toggles), volume OSD, launcher (apps, clipboard,
# emoji, calculator, files), power menu and lock screen. Square and flat like
# the rest of the desktop; the colours come from the active theme.
#
#   Super+Space        launcher            Super+V   clipboard
#   Super+;            emoji               Super+C   calculator
#   Super+/            files               Super+A   control center
#   Super+Shift+N      notifications       Super+Shift+Escape  power menu
#   Super+Escape       lock
#
# Hacking on it: `just shell-dev` stops the service and runs the shell from
# ~/nixos/shell with live reload (save a .qml file, it updates). Ctrl+C, then
# `systemctl --user start vshell` to go back.
{ config, ... }:
let
  inherit (config) meta;
  palette = import ../../assets/themes meta;
in
{
  # The lock screen checks your password through PAM (/etc/pam.d/vshell).
  flake.modules.nixos.desktop.security.pam.services.vshell = { };

  flake.modules.homeManager.desktop =
    {
      config,
      pkgs,
      lib,
      ...
    }:
    let
      c = config.lib.stylix.colors;
      src = lib.fileset.toSource {
        root = ../../shell;
        fileset = lib.fileset.fileFilter (
          f: f.hasExt "qml" || f.hasExt "json" || f.hasExt "js"
        ) ../../shell;
      };
      vshell = pkgs.writeShellScriptBin "vshell" ''
        exec ${lib.getExe pkgs.quickshell} -p ${src} "$@"
      '';
      slots = map (n: "base0${n}") [
        "0"
        "1"
        "2"
        "3"
        "4"
        "5"
        "6"
        "7"
        "8"
        "9"
        "A"
        "B"
        "C"
        "D"
        "E"
        "F"
      ];
      theme = pkgs.writeText "vshell-theme.json" (
        builtins.toJSON (
          lib.genAttrs slots (s: c.${s})
          // {
            border = if palette.extra.border != null then palette.extra.border else c.base02;
            accent2 = if palette.extra.accent2 != null then palette.extra.accent2 else c.base0F;
            font = config.stylix.fonts.monospace.name;
            primaryModel = "VG34VQL3A"; # the ultrawide gets pop-ups and the OSD
          }
        )
      );
      wallpaper = if palette.wallpapers != null then palette.wallpapers.wide else config.stylix.image;
      ipc = "vshell ipc call";
    in
    {
      home.packages = [
        vshell
        pkgs.quickshell # `qs` for `just shell-dev`
      ]
      ++ (with pkgs; [
        libqalculate # calculator tab
        fd # files tab
        wl-clipboard
        xdg-utils
        networkmanagerapplet # nm-connection-editor, from the network tile
        blueman # blueman-manager, right click on the Bluetooth tile
        pwvucontrol # the mixer button, for per-app routing
      ]);

      systemd.user.services.vshell = {
        Unit = {
          Description = "vshell (Quickshell desktop shell)";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" ];
          X-Restart-Triggers = [
            "${src}"
            "${theme}"
          ];
        };
        Service = {
          ExecStart = lib.getExe vshell;
          Environment = [
            "VSHELL_THEME=${theme}"
            "VSHELL_WALLPAPER=${wallpaper}"
            "VSHELL_AUDIO_OUTPUT=${toString meta.audioOutput}"
            # Programs the shell starts (nmcli, cliphist, qalc, mmsg, sudo, ...).
            "PATH=/run/wrappers/bin:/etc/profiles/per-user/${meta.username}/bin:/run/current-system/sw/bin"
          ];
          Restart = "on-failure";
          RestartSec = 1;
        };
        Install.WantedBy = [ "graphical-session.target" ];
      };

      wayland.windowManager.mango.settings.bind = [
        "SUPER,space,spawn,${ipc} panels toggleTab launcher apps"
        "SUPER,v,spawn,${ipc} panels toggleTab launcher clipboard"
        "SUPER,semicolon,spawn,${ipc} panels toggleTab launcher emoji"
        "SUPER,c,spawn,${ipc} panels toggleTab launcher calc"
        "SUPER,slash,spawn,${ipc} panels toggleTab launcher files"
        "SUPER,a,spawn,${ipc} panels toggle control"
        "SUPER+SHIFT,n,spawn,${ipc} panels toggle notifications"
        "SUPER+SHIFT,Escape,spawn,${ipc} panels toggle power"
        "SUPER,Escape,spawn,${ipc} lock lock"
        "NONE,XF86AudioRaiseVolume,spawn,${ipc} audio raise"
        "NONE,XF86AudioLowerVolume,spawn,${ipc} audio lower"
        "NONE,XF86AudioMute,spawn,${ipc} audio mute"
        "NONE,XF86AudioMicMute,spawn,${ipc} audio micMute"
      ];
    };
}
