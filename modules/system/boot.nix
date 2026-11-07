# Boot: systemd-boot on the NixOS drive's own ESP. Windows stays on its own
# drive and is picked from the firmware boot menu (F11 on MSI boards).
{
  flake.modules.nixos.base =
    { pkgs, ... }:
    {
      boot = {
        loader = {
          systemd-boot = {
            enable = true;
            configurationLimit = 10;
            editor = false; # no editing kernel params at the boot menu
          };
          efi.canTouchEfiVariables = true;
          timeout = 0; # straight to NixOS; hold Space while booting for the menu (older generations)
        };
        kernelPackages = pkgs.linuxPackages_latest;
      };

      zramSwap.enable = true;

      # Windows keeps the hardware clock in local time.
      time.hardwareClockInLocalTime = true;
    };
}
