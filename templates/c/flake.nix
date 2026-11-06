{
  description = "C / C++ project (clang, CMake, Ninja)";

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
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.clang-tools # clangd, clang-format, clang-tidy
              pkgs.cmake
              pkgs.ninja
              pkgs.pkg-config
              pkgs.lldb
            ];
            # clangd reads build/compile_commands.json:
            #   cmake -B build -G Ninja -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
          };
        }
      );
    };
}
