# The work repos, from GitHub, and their dev shells. Each switch clones any
# that are missing into ~/Programing once you're logged in to GitHub
# (`gh auth login`; most are private), and gives each a local .envrc (direnv
# loads the shell on `cd`, and agent sessions get it through dev/claude.nix's
# hook), listed in .git/info/exclude so it never reaches the repo. An existing
# checkout or .envrc is left alone. direnv trusts exactly these .envrc files
# (not the folders), so a repo's own .envrc elsewhere still needs
# `direnv allow`.
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
      # Folder (under ~/Programing) = its GitHub repo, branch and .envrc.
      projects = {
        # Its shell is projects/mainline.nix until Mainline's own flake.nix
        # (chore/nix-dev-shell) is merged.
        "Mainline-sim/Mainline" = {
          repo = "Mainline-sim/mainline";
          branch = "next";
          envrc = "use flake ${meta.flakePath}#mainline";
        };
        "OIS" = {
          repo = "VATUSA/OIS";
          branch = "next";
          envrc = "use flake ${meta.flakePath}#ois";
        };
        "aviodeck" = {
          repo = "aviodeck/aviodeck";
          branch = "main";
          envrc = "use flake ${meta.flakePath}#aviodeck";
        };
        "aviodeck-link" = {
          repo = "aviodeck/aviodeck-link";
          branch = "main";
          envrc = "use flake ${meta.flakePath}#aviodeck-link";
        };
      }
      # vZDC (projects/vzdc.nix): website twice, master and next.
      //
        lib.mapAttrs'
          (name: p: {
            name = "vZDC/${name}";
            value = {
              repo = "vzdc-artcc/${p.repo or name}";
              branch = p.branch or "master";
              envrc = "use flake ${meta.flakePath}#vzdc-${name}";
            };
          })
          {
            website = { };
            website-next = {
              repo = "website";
              branch = "next";
            };
            osmium = { };
            ids = { };
            asx = { };
            discord-bot = { };
            moodle = {
              repo = "vzdc-moodle";
              branch = "main";
            };
          };
      gh = lib.getExe pkgs.gh;
      write = dir: p: ''
        d=${home}/Programing/${dir}
        if [ ! -e "$d" ]; then
          if ${gh} auth status >/dev/null 2>&1; then
            run mkdir -p "$(dirname "$d")"
            run env PATH="${pkgs.git}/bin:$PATH" ${gh} repo clone ${p.repo} "$d" -- -b ${p.branch} || echo "could not clone ${p.repo}" >&2
          else
            echo "${dir}: not cloned yet; run gh auth login, then vx switch" >&2
          fi
        fi
        if [ -d "$d/.git" ]; then
          if [ ! -e "$d/.envrc" ]; then
            run sh -c 'printf "%s\n" "$1" > "$2"' sh ${lib.escapeShellArg p.envrc} "$d/.envrc"
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

      home.activation.projects = lib.hm.dag.entryAfter [ "writeBoundary" ] (
        lib.concatStrings (lib.mapAttrsToList write projects)
      );
    };
}
