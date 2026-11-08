# Every wallpaper in assets/wallpapers (folders with a wide.png), each with
# its images and its own palette, overrides applied. Used by the shell's
# wallpaper picker (modules/desktop/shell.nix) and the installer's theme
# screen (install-data, modules/flake/install.nix).
#
#   import ./wallpapers.nix meta  ->  [ { name, wide, uhd, colors } ]
#
# `colors` is base00..base0F (plus scheme/author) and border, accent2.
meta:
map
  (
    name:
    let
      p = import ./. (
        meta
        // {
          wallpaper = name;
          theme = "match";
        }
      );
    in
    {
      inherit name;
      inherit (p.wallpapers) wide uhd;
      colors = p.scheme // {
        border = if p.extra.border != null then p.extra.border else p.scheme.base02;
        accent2 = if p.extra.accent2 != null then p.extra.accent2 else p.scheme.base0F;
      };
    }
  )
  (
    builtins.filter (n: builtins.pathExists (../wallpapers + "/${n}/wide.png")) (
      builtins.attrNames (builtins.readDir ../wallpapers)
    )
  )
