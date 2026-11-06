# Cider: Apple Music client (needs your Apple Music subscription). Media keys
# and the notification panel's player controls work through MPRIS.
{
  flake.modules.nixos.desktop = {
    unfree.allowed = [ "cider-2" ];
  };
  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      home.packages = [ pkgs.cider-2 ];
    };
}
