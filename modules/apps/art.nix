# Aseprite, the pixel-art editor. nixpkgs builds it from source (its EULA
# allows that for your own use, and you own a licence anyway). It's unfree and
# not redistributable, so there's no binary cache: the first build compiles
# Aseprite and its Skia fork locally, which takes a while.
{
  flake.modules.nixos.desktop = {
    unfree.allowed = [ "aseprite" ];
  };
  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      home.packages = [ pkgs.aseprite ];
    };
}
