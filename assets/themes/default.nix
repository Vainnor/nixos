# Resolve the look from meta: which palette, and which wallpaper files.
#
#   theme = "match"          the wallpaper's own palette (assets/wallpapers/<name>/theme.nix)
#   theme = "vainnor-slate"  a palette from this folder
#   theme = "<base16 name>"  any base16 scheme (scheme = null; Stylix reads it from base16-schemes)
#
#   wallpaper = "<name>"     a folder in assets/wallpapers with wide.png (ultrawide) and 4k.png
#   wallpaper = "dots"       a flat dot grid in the theme's colours (own palettes only)
#   wallpaper = ./some.png   any image
#
# `extra` has the two colours base16 has no slot for: `border` (unfocused
# edges) and `accent2`.
#
# Overrides, applied in this order on top of `scheme` and `extra`:
#
#   1. `overrides = { ... };` inside the theme file itself. Keep the generated
#      `scheme`/`extra` blocks as scripts/wallpaper-palette.py wrote them and
#      put hand fixes here, so a regenerated palette keeps them:
#        overrides = { base0D = "8FB0E6"; base01 = "15151A"; border = "3A3A42"; };
#   2. `meta.themeOverrides` (modules/flake/meta.nix), last, for quick
#      experiments on whatever theme is active:
#        themeOverrides = { base0D = "E0A040"; };
#
# Keys: any base16 slot (base00..base0F, also scheme/author) or an `extra`
# key (border, accent2). Hex without '#'. Slot overrides need a local palette
# (a base16 name has no scheme here to change); extra keys always apply.
# `just theme-check` checks the result for contrast and colour.
{
  theme,
  wallpaper,
  themeOverrides ? { },
  ...
}:
let
  wallDir = ../wallpapers + "/${wallpaper}";
  isSet = builtins.isString wallpaper && builtins.pathExists (wallDir + "/wide.png");
  themeFile =
    if theme == "match" then
      (
        if isSet then
          wallDir + "/theme.nix"
        else
          throw "meta.theme = \"match\" needs meta.wallpaper to be a folder in assets/wallpapers"
      )
    else
      ./. + "/${theme}.nix";

  base = {
    scheme = null;
    extra = {
      border = null;
      accent2 = null;
    };
  }
  // (if builtins.pathExists themeFile then import themeFile else { });

  extraKeys = [
    "border"
    "accent2"
  ];
  split = over: {
    extra = removeAttrs over (
      builtins.filter (k: !builtins.elem k extraKeys) (builtins.attrNames over)
    );
    scheme = removeAttrs over extraKeys;
  };
  apply =
    p: over:
    let
      o = split over;
    in
    {
      extra = p.extra // o.extra;
      scheme =
        if o.scheme == { } then
          p.scheme
        else if p.scheme == null then
          throw "theme overrides for ${toString (builtins.attrNames o.scheme)} need a local palette; \"${theme}\" is a base16 name"
        else
          p.scheme // o.scheme;
    };
  final = apply (apply base (base.overrides or { })) themeOverrides;
in
{
  inherit (final) scheme extra;
  # null unless meta.wallpaper names a folder in assets/wallpapers.
  wallpapers =
    if isSet then
      {
        wide = wallDir + "/wide.png";
        uhd = wallDir + "/4k.png";
      }
    else
      null;
}
