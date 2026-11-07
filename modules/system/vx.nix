# vx: one command for this flake, on the PC and the Mac (tools/vx, Rust).
# `vx` lists everything; `vx switch`, `vx update`, `vx status`, `vx wallpaper`,
# `vx @arc switch` from the Mac, ... Tab completion works in fish.
{
  perSystem =
    { pkgs, ... }:
    {
      packages.vx = pkgs.callPackage ../../tools/vx/package.nix { };
    };

  flake.modules.homeManager.base =
    { pkgs, ... }:
    {
      home.packages = [
        (pkgs.callPackage ../../tools/vx/package.nix { })
        pkgs.nvd # `vx update` / `vx diff`: what changed between two systems
      ];
    };
}
