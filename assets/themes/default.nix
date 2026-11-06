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
{ theme, wallpaper, ... }:
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
in
{
  scheme = null;
  extra = {
    border = null;
    accent2 = null;
  };
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
// (if builtins.pathExists themeFile then import themeFile else { })
