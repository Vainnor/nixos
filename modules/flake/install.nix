# The installer: `vx install` (tools/vx/src/cmd/install), fed with this
# flake's install-data. From any NixOS installer ISO:
#
#   sudo nix --extra-experimental-features 'nix-command flakes' run github:vainnor/nixos
#
# The custom ISO (hosts/installer.nix) starts it by itself. It lets you pick
# the machine (reinstall an existing host, or a new one that gets its own
# modules/hosts/<name>/ with host.json, _hardware.nix and disks.json; see
# flake/hosts.nix), the system drive and data drives, carries state over from
# a previous install, can restore /home from the homelab backup (restic; its
# login from the secrets, the 1Password item meta.restoreItem, or typed),
# writes the host folder and the meta edits into its repo copy, runs disko
# (hosts/_disko.nix) and nixos-install, then commits and pushes (to your fork
# unless you are the owner).
#
#   packages.<system>.install-data  defaults and choices for the screens (JSON;
#                                   builds on the Mac too, for `vx install --demo`)
#   packages.<linux>.vx-installer   `vx install --data <that JSON>`, as root
#   apps.install / apps.default     the same, for `nix run`
{
  self,
  inputs,
  lib,
  config,
  ...
}:
let
  inherit (config) meta;
  # The default host: arc, or the first host folder in a repo without one.
  host = if config.vx.hosts ? arc then "arc" else lib.head (builtins.attrNames config.vx.hosts);

  # https://github.com/<owner>/<repo>[.git], or the SSH form <user>@github.com:<owner>/<repo>.git
  repoMatch = builtins.match "(https://github[.]com/|[a-z]+@github[.]com:)([^/]+)/([^/]+)" meta.repoUrl;
  repoPart =
    i:
    if repoMatch == null then
      throw "meta.repoUrl must be a GitHub repo (https://github.com/<owner>/<repo>), not ${meta.repoUrl}"
    else
      lib.removeSuffix ".git" (lib.removeSuffix "/" (builtins.elemAt repoMatch i));

  # Theme files are attrsets with a `scheme`; default.nix and wallpapers.nix are functions.
  themes = lib.sort lib.lessThan (
    map (lib.removeSuffix ".nix") (
      builtins.filter (
        f:
        let
          v = import (../../assets/themes + "/${f}");
        in
        lib.hasSuffix ".nix" f && builtins.isAttrs v && v ? scheme
      ) (builtins.attrNames (builtins.readDir ../../assets/themes))
    )
  );

  installData = {
    inherit host themes;
    inherit (meta) repoUrl githubUser;
    # The 1Password item with the home backup's login (null: not offered).
    restoreItem = meta.restoreItem or null;
    # The upstream repo on GitHub, from repoUrl.
    owner = repoPart 1;
    repo = repoPart 2;
    # Every host folder (flake/hosts.nix), for "reinstall this machine".
    hosts = map (h: { inherit (h) name aspects gpu; }) (builtins.attrValues config.vx.hosts);
    # What a new machine can pick; base is always on and not listed.
    inherit (config.vx) aspects;
    # A new machine's stateVersion: the NixOS release it is installed with.
    stateVersion = inputs.nixpkgs.lib.trivial.release;
    flake = "${self}";
    defaults = {
      hostname = self.nixosConfigurations.${host}.config.networking.hostName;
      inherit (meta) username timeZone theme;
      wallpaper = toString meta.wallpaper;
    };
    # Names and palettes only (no image paths, so the JSON stays tiny).
    wallpapers = map (w: { inherit (w) name colors; }) (import ../../assets/themes/wallpapers.nix meta);
  };
in
{
  perSystem =
    { pkgs, self', ... }:
    let
      data = pkgs.writeText "install-data.json" (builtins.toJSON installData);
      installer = pkgs.writeShellApplication {
        name = "vx-installer";
        text = ''
          if [ "$(id -u)" != 0 ]; then
            exec sudo "$0" "$@"
          fi
          exec ${lib.getExe self'.packages.vx} install --data ${data} "$@"
        '';
      };
    in
    {
      packages = {
        install-data = data;
      }
      // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux { vx-installer = installer; };

      apps = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux rec {
        install = {
          type = "app";
          meta.description = "Install NixOS onto the drives you pick (full-screen installer; asks before erasing)";
          program = lib.getExe installer;
        };
        default = install;
      };
    };
}
