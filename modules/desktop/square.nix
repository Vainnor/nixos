# The flat, square look (after dacctal's desktop): no rounded corners, shadows
# or blur; lowercase text labels; the active thing is a solid accent block.
# Stylix supplies the colours; this only adds shape on top.
{ config, ... }:
let
  inherit (import ../../assets/themes config.meta) extra;
in
{
  flake.modules.homeManager.desktop =
    { config, pkgs, ... }:
    let
      c = config.lib.stylix.colors.withHashtag;
      border = if extra.border != null then "#${extra.border}" else c.base02;
    in
    {
      programs.waybar.style = ''
        * { border-radius: 0; box-shadow: none; text-shadow: none; min-height: 0; }
        window#waybar { background: ${c.base00}; border-bottom: 2px solid ${border}; }
        #workspaces { margin: 4px; }
        #workspaces button {
          padding: 0 12px; margin-right: 4px; border: none;
          background: ${c.base01}; color: ${c.base0D};
        }
        #workspaces button:hover { background: ${c.base02}; }
        #workspaces button.active, #workspaces button.focused {
          background: ${c.base0D}; color: ${c.base00};
        }
        #workspaces button.urgent { background: ${c.base08}; color: ${c.base00}; }
        #clock, #tray, #pulseaudio, #network, #cpu, #memory,
        #custom-notification, #custom-power {
          padding: 0 12px; margin: 4px 0 4px 4px; background: ${c.base01}; color: ${c.base05};
        }
        #clock { color: ${c.base0D}; }
        #pulseaudio.muted, #network.disconnected { color: ${c.base08}; }
        #custom-power { margin-right: 4px; color: ${c.base08}; }
        #custom-power:hover, #pulseaudio:hover, #custom-notification:hover { background: ${c.base02}; }
        tooltip { background: ${c.base00}; border: 2px solid ${border}; }
      '';

      services.swaync.style = ''
        * { border-radius: 0 !important; box-shadow: none !important; }
        .notification, .control-center { border: 2px solid ${border}; }
      '';

      services.swayosd.stylePath = pkgs.writeText "swayosd.css" ''
        window#osd { border-radius: 0; background: ${c.base00}; border: 2px solid ${border}; }
        #container { margin: 12px; }
        image, label { color: ${c.base05}; }
        progressbar, trough, progress { border-radius: 0; min-height: 6px; }
        trough { background: ${c.base02}; }
        progress { background: ${c.base0D}; }
      '';
    };
}
