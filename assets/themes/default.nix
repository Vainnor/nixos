# Load a theme by name: ours from this folder, or `scheme = null` for a base16
# name (Stylix then reads it from base16-schemes). `extra` has the two colours
# base16 has no slot for: `border` (unfocused edges) and `accent2`.
theme:
let
  file = ./. + "/${theme}.nix";
  own = builtins.pathExists file;
in
{
  scheme = null;
  extra = {
    border = null;
    accent2 = null;
  };
}
// (if own then import file else { })
