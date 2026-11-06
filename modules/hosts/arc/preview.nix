# `just preview`: boot arc's real config in a QEMU window, on any Linux machine
# with Nix (the PC's current distro, or WSL). Same desktop, apps, theme and
# services, so problems show up here and not on the real drive.
#
# Only the parts that need the real hardware are swapped out: no disk
# partitioning or encryption (the VM gets its own scratch disk), no Secure Boot,
# no wiped-at-boot root, no USBGuard, no snapshots. Log in as vainnor / preview.
# Monitor rules won't match, so Mango uses the VM window as one screen.
{ config, ... }:
let
  user = config.meta.username;
in
{
  flake.modules.nixos.arc =
    { lib, ... }:
    {
      virtualisation.vmVariant = {
        virtualisation = {
          memorySize = 8192;
          cores = 4;
          diskSize = 30 * 1024; # Steam and caches land here, not on the host
          resolution = {
            x = 1920;
            y = 1080;
          };
        };

        boot.lanzaboote.enable = lib.mkForce false;
        preservation.enable = lib.mkForce false;
        services.usbguard.enable = lib.mkForce false;
        services.btrbk.instances = lib.mkForce { };

        # The real password lives on /persistent, which the VM doesn't have.
        users.users.${user} = {
          hashedPasswordFile = lib.mkVMOverride null;
          password = "preview";
        };

        # Let Mango start on a plain virtual GPU too (software rendering).
        environment.sessionVariables = {
          WLR_RENDERER_ALLOW_SOFTWARE = "1";
          WLR_NO_HARDWARE_CURSORS = "1";
        };
      };
    };
}
