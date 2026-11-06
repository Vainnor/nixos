{
  description = "Go project (Go pinned by this flake)";

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
          # Change to go_1_24, ... per project.
          go = pkgs.go;
        in
        {
          default = pkgs.mkShell {
            packages = [
              go
              pkgs.gopls
              pkgs.gotools # goimports, ...
              pkgs.golangci-lint
              pkgs.delve # debugger
            ];
            # Use exactly this Go; never auto-download another toolchain.
            GOTOOLCHAIN = "local";
          };
        }
      );
    };
}
