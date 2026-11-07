# Secure Boot with lanzaboote, and TPM2 unlock of the encrypted disk.
#
# Flow (see README "Secure Boot + TPM"):
#   1. Install with Secure Boot OFF. First boot generates your own keys
#      (/var/lib/sbctl, persisted) and prepares enrollment.
#   2. In the BIOS, put Secure Boot into Setup Mode (clear/delete keys), reboot.
#      systemd-boot enrolls your keys + Microsoft's (Windows and the 7900 XTX
#      option ROM need them). Then turn Secure Boot ON in the BIOS.
#   3. `vx tpm enroll`: the disk unlocks itself as long as Secure Boot state
#      (PCR 7) is unchanged. Your passphrase keeps working as a fallback.
{ inputs, ... }:
{
  flake.modules.nixos.base =
    { lib, pkgs, ... }:
    {
      imports = [ inputs.lanzaboote.nixosModules.lanzaboote ];

      boot.loader.systemd-boot.enable = lib.mkForce false;
      boot.lanzaboote = {
        enable = true;
        pkiBundle = "/var/lib/sbctl";
        configurationLimit = 10;
        autoGenerateKeys.enable = true;
        autoEnrollKeys.enable = true; # includes Microsoft keys by default
      };

      security.tpm2.enable = true;
      boot.initrd.luks.devices.cryptroot.crypttabExtraOpts = [ "tpm2-device=auto" ];

      environment.systemPackages = [ pkgs.sbctl ];

      preservation.preserveAt."/persistent".directories = [
        {
          directory = "/var/lib/sbctl"; # your Secure Boot keys
          mode = "0700";
        }
        "/var/lib/auto-cryptenroll"
      ];
    };
}
