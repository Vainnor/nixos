{
  description = "Python project (uv + a Python pinned by this flake)";

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
          # The Python for this project. Change to python312, python314, ... and
          # run `rm -rf .venv` once; other projects are not affected.
          python = pkgs.python313;
        in
        {
          default = pkgs.mkShell {
            packages = [
              python
              pkgs.uv # deps and .venv: `uv init`, `uv add requests`, `uv run main.py`
              pkgs.ruff # lint + format
              pkgs.basedpyright # type checker / language server
            ];
            # uv uses the Python above and never downloads its own here.
            UV_PYTHON = "${python}/bin/python";
            UV_PYTHON_DOWNLOADS = "never";
            # Prebuilt wheels (numpy, ...) find the C++ runtime and zlib on NixOS.
            LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (
              pkgs.lib.makeLibraryPath [
                pkgs.stdenv.cc.cc.lib
                pkgs.zlib
              ]
            );
          };
        }
      );
    };
}
