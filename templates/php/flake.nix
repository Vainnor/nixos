{
  description = "PHP project (PHP + Composer pinned by this flake)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      forAll = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-darwin"
      ];
    in
    {
      devShells = forAll (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          # Change to php83, php85, ... and pick extensions per project.
          php = pkgs.php84.buildEnv {
            extensions = { enabled, all }: enabled ++ [ all.xdebug ];
            extraConfig = "memory_limit = 1G";
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              php
              php.packages.composer # deps live in ./vendor
              pkgs.phpactor # language server
            ];

          };
        }
      );
    };
}
