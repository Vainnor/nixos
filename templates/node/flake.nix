{
  description = "TypeScript / JavaScript project (Node + pnpm pinned by this flake)";

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
          # Change to nodejs_20, nodejs_24, ... per project.
          node = pkgs.nodejs_22;
        in
        {
          default = pkgs.mkShell {
            packages = [
              node
              pkgs.pnpm # or npm (comes with node); deps live in ./node_modules
              pkgs.typescript
              pkgs.typescript-language-server
              pkgs.vscode-langservers-extracted # html, css, json, eslint
            ];
            # `npm i -g` would write into the Nix store; keep globals per project.
            NPM_CONFIG_PREFIX = "./.direnv/npm-global";
          };
        }
      );
    };
}
