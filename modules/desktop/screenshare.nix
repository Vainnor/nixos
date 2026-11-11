# Screen sharing (Vesktop, browsers, OBS's PipeWire source) goes through
# xdg-desktop-portal-wlr, which Mango's portal config picks for ScreenCast.
# With more than one monitor the portal asks which one to share: click it
# (slurp, in the palette). NixOS starts the portal with its own --config file,
# so the settings go there (a ~/.config one is never read); the chooser is
# given by its full path, as the portal's PATH has none of your tools.
{
  flake.modules.nixos.desktop =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      c = config.lib.stylix.colors;
    in
    {
      xdg.portal.wlr.settings.screencast = {
        chooser_type = "simple";
        chooser_cmd = "${lib.getExe pkgs.slurp} -f %o -or -b '#${c.base00}88' -c '#${c.base0D}ff' -w 2";
        max_fps = 60;
      };
    };
}
