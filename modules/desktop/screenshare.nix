# Screen sharing (Vesktop, browsers, OBS's PipeWire source) goes through
# xdg-desktop-portal-wlr, which Mango's portal config picks for ScreenCast.
# With more than one monitor the portal asks which one to share: click it
# (slurp, in the palette). The portal runs as a user service without your
# profile on its PATH, so the chooser is given by its full path.
{
  flake.modules.homeManager.desktop =
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
      xdg.configFile."xdg-desktop-portal-wlr/config".text = ''
        [screencast]
        chooser_type=simple
        chooser_cmd=${lib.getExe pkgs.slurp} -f %o -or -b '#${c.base00}88' -c '#${c.base0D}ff' -w 2
        max_fps=60
      '';
    };
}
