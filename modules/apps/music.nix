# Cider: Apple Music client (needs your Apple Music subscription). Media keys
# and the notification panel's player controls work through MPRIS.
#
# nixpkgs' launcher escapes its Wayland flags (they reach Cider as literal
# `$\{NIXOS_OZONE_WL:+...\}` text), so Cider ran under XWayland, where Mango
# doesn't minimize it properly. This copy of the launcher passes them for real.
# Super+Shift+M brings a minimized window back (desktop/mango.nix).
{
  flake.modules.nixos.desktop = {
    unfree.allowed = [ "cider-2" ];
  };
  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    let
      flags = "--ozone-platform-hint=auto --enable-features=WaylandWindowDecorations --enable-wayland-ime=true";
      cider = pkgs.symlinkJoin {
        name = "cider-2-wayland";
        paths = [ pkgs.cider-2 ];
        postBuild = ''
          cp --remove-destination "$(readlink -f $out/bin/cider-2)" $out/bin/cider-2
          substituteInPlace $out/bin/cider-2 --replace-fail \
            '$\{NIXOS_OZONE_WL:+$\{WAYLAND_DISPLAY:+${flags}\}\}' '${flags}'
        '';
      };
    in
    {
      home.packages = [ cider ];
    };
}
