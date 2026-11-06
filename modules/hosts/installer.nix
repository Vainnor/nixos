# Custom installer ISO: the normal minimal NixOS ISO plus this flake's
# installer on PATH, flakes enabled, and a login message that says what to do.
#
#   Build (any x86_64 Linux, WSL works, or the GitHub Action):
#     nix build .#nixosConfigurations.installer.config.system.build.isoImage
#
# Boot it, connect Wi-Fi with `nmtui` (or plug in Ethernet), run `sudo install-nixos`.
{
  inputs,
  self,
  ...
}:
{
  flake.nixosConfigurations.installer = inputs.nixpkgs.lib.nixosSystem {
    modules = [
      "${inputs.nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-minimal.nix"
      (
        { pkgs, lib, ... }:
        {
          nixpkgs.hostPlatform = "x86_64-linux";
          nix.settings.experimental-features = [
            "nix-command"
            "flakes"
          ];
          boot.kernelPackages = pkgs.linuxPackages_latest; # Zen 5 / RDNA3 / MT7922 support
          boot.supportedFilesystems.zfs = lib.mkForce false; # ZFS lags the latest kernel; not needed
          environment.systemPackages = [
            self.packages.x86_64-linux.install-nixos
            pkgs.git
          ];
          image.baseName = lib.mkForce "nixos-arc-installer";
          services.getty.helpLine = ''

            ===  arc installer  ===
            1. Network: plug in Ethernet, or run `nmtui` for Wi-Fi.
            2. Install: sudo install-nixos
          '';
        }
      )
    ];
  };
}
