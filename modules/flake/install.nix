# The installer: `vx install` (tools/vx/src/cmd/install), fed with this
# flake's install-data. From any NixOS installer ISO:
#
#   sudo nix --extra-experimental-features 'nix-command flakes' run github:vainnor/nixos
#
# The custom ISO (hosts/installer.nix) starts it by itself. It lets you pick
# the system drive and data drives, carries state over from a previous
# install, writes modules/hosts/arc/disks.json and the meta edits into its
# repo copy, then runs disko (hosts/arc/_disko.nix) and nixos-install.
#
#   packages.<system>.install-data  defaults and choices for the screens (JSON;
#                                   builds on the Mac too, for `vx install --demo`)
#   packages.<linux>.vx-installer   `vx install --data <that JSON>`, as root
#   apps.install / apps.default     the same, for `nix run`
{
  self,
  lib,
  config,
  ...
}:
let
  inherit (config) meta;
  host = "arc";

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
    inherit (meta) repoUrl;
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
