# The project checkouts and their dev shells. Each one that has been cloned
# gets a local .envrc (direnv loads the shell on `cd`, and agent sessions get
# it through dev/claude.nix's hook), listed in .git/info/exclude so it never
# reaches the repo. An existing .envrc is left alone.
#
# On arc they came over from WSL (hosts/arc/wsl-import.nix). Elsewhere, clone
# them yourself (they need your GitHub login):
#   gh repo clone Mainline-sim/mainline ~/Programing/Mainline-sim/Mainline -- -b next
#   gh repo clone VATUSA/OIS ~/Programing/ois -- -b next
#   gh repo clone aviodeck/aviodeck ~/Programing/aviodeck
#   gh repo clone aviodeck/aviodeck-link ~/Programing/aviodeck-link
# then `vx switch` (or `home-manager switch`) writes the .envrc files. direnv
# trusts exactly these four files (not the folders), so a repo's own .envrc
# anywhere else still needs `direnv allow`.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.homeManager.dev =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      home = config.home.homeDirectory;
      # Folder (under ~/Programing) = what its .envrc says.
      projects = {
        # Mainline carries its own flake.nix (on its chore/nix-dev-shell branch
        # until merged); until then the .envrc does nothing.
        "Mainline-sim/Mainline" = "if [ -f flake.nix ]; then use flake; fi";
        "ois" = "use flake ${meta.flakePath}#ois";
        "aviodeck" = "use flake ${meta.flakePath}#aviodeck";
        "aviodeck-link" = "use flake ${meta.flakePath}#aviodeck-link";
      };
      write = dir: envrc: ''
        d=${home}/Programing/${dir}
        if [ -d "$d/.git" ]; then
          if [ ! -e "$d/.envrc" ]; then
            run sh -c 'printf "%s\n" "$1" > "$2"' sh ${lib.escapeShellArg envrc} "$d/.envrc"
          fi
          x="$d/.git/info/exclude"
          run mkdir -p "$d/.git/info"
          for p in /.envrc /.direnv/; do
            grep -qxF "$p" "$x" 2>/dev/null || run sh -c 'printf "%s\n" "$1" >> "$2"' sh "$p" "$x"
          done
        fi
      '';
    in
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      programs.direnv.config.whitelist.exact = lib.mapAttrsToList (
        dir: _: "${home}/Programing/${dir}/.envrc"
      ) projects;

      home.activation.projectEnvrc = lib.hm.dag.entryAfter [ "writeBoundary" ] (
        lib.concatStrings (lib.mapAttrsToList write projects)
      );
    };
}
