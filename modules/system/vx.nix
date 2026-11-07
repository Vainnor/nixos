# vx: the justfile from anywhere. `vx wallpaper halo`, `vx switch`, `vx fans`,
# `vx` alone lists everything. Same commands on the PC and the Mac; tab
# completion in fish shows each recipe with its description.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.homeManager.base =
    { pkgs, lib, ... }:
    let
      repo =
        if pkgs.stdenv.hostPlatform.isDarwin then "/Users/${meta.username}/Programing/nixos" else meta.flakePath;
      vx = pkgs.writeShellApplication {
        name = "vx";
        runtimeInputs = [ pkgs.just ];
        text = ''
          repo="''${VX_REPO:-${repo}}"
          [ -f "$repo/justfile" ] || { echo "vx: no repo at $repo (set VX_REPO)" >&2; exit 1; }
          exec just --justfile "$repo/justfile" --working-directory "$repo" "$@"
        '';
      };
    in
    {
      home.packages = [ vx ];
      programs.fish.interactiveShellInit = lib.mkAfter ''
        complete -c vx -f -a '(vx --list --unsorted 2>/dev/null | string match -r "^\s+\S.*" | string trim | string replace -r "\s+#\s*" "\t" | string replace -r "^(\S+)[^\t]*" "\$1")'
      '';
    };
}
