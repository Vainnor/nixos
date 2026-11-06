# Lock screen: gtklock, styled like the rest of the desktop (flat, square,
# the wallpaper behind it, a big clock and a plain password box).
# Locks after 10 min idle, screens off after 15, and always before sleep.
# Super+Escape (or the power menu) locks now.
{ config, ... }:
let
  palette = import ../../assets/themes config.meta;
in
{
  flake.modules.nixos.desktop =
    { config, ... }:
    let
      c = config.lib.stylix.colors.withHashtag;
      border = if palette.extra.border != null then "#${palette.extra.border}" else c.base02;
      font = config.stylix.fonts.monospace.name;
    in
    {
      programs.gtklock = {
        enable = true;
        config.main = {
          time-format = "%H:%M";
          date-format = "%A %d %B";
          idle-hide = true; # just the wallpaper and clock until you type
          idle-timeout = 15;
        };
        style = ''
          * {
            border-radius: 0;
            box-shadow: none;
            text-shadow: none;
            font-family: "${font}";
          }
          window {
            background-image: url("${config.stylix.image}");
            background-size: cover;
            background-position: center;
            background-color: ${c.base00};
          }
          #window-box {
            background-color: alpha(${c.base00}, 0.85);
            border: 2px solid ${border};
            padding: 24px 32px;
          }
          #clock-label {
            font-size: 64px;
            font-weight: bold;
            color: ${c.base05};
          }
          #date-label {
            font-size: 16px;
            color: ${c.base0D};
          }
          #input-label {
            color: ${c.base04};
          }
          entry {
            min-width: 280px;
            padding: 6px 10px;
            background: ${c.base01};
            color: ${c.base05};
            border: 2px solid ${border};
            outline: none;
            caret-color: ${c.base0D};
          }
          entry:focus-within {
            border-color: ${c.base0D};
          }
          button {
            background: ${c.base01};
            color: ${c.base0D};
            border: 2px solid ${border};
            padding: 6px 14px;
          }
          button:hover {
            background: ${c.base02};
          }
          #error-label {
            color: ${c.base08};
          }
          #warning-label {
            color: ${c.base0A};
          }
        '';
      };
    };

  flake.modules.homeManager.desktop =
    { pkgs, lib, ... }:
    let
      lock = "/run/current-system/sw/bin/gtklock -d";
    in
    {
      services.swayidle = {
        enable = true;
        events = {
          before-sleep = lock;
          lock = lock;
        };
        timeouts = [
          {
            timeout = 600;
            command = lock;
          }
          {
            timeout = 900;
            command = "${lib.getExe pkgs.wlopm} --off '*'";
            resumeCommand = "${lib.getExe pkgs.wlopm} --on '*'";
          }
        ];
      };
    };
}
