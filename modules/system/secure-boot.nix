# Secure Boot with lanzaboote, and TPM2 unlock of the encrypted disk.
#
# Flow (see README "Secure Boot + TPM"):
#   1. Install (`vx install`) with Secure Boot OFF. The boot files are
#      installed unsigned. First boot generates your own keys (/var/lib/sbctl,
#      persisted), signs the boot files and prepares enrollment.
#   2. In the BIOS, put Secure Boot into Setup Mode (clear/delete keys), reboot.
#      systemd-boot enrolls your keys + Microsoft's (Windows and graphics
#      card option ROMs need them). Then turn Secure Boot ON in the BIOS.
#   3. The first boot with Secure Boot on enrolls the TPM by itself
#      (tpm-first-boot-enroll below, when the installer chose TPM unlock): the
#      disk then unlocks itself as long as Secure Boot state (PCR 7) is
#      unchanged. Your passphrase keeps working as a fallback.
#      `vx tpm enroll` does the same by hand (asks for the passphrase).
#
# Secure Boot keys carried over by the installer and still enrolled in the
# firmware: the boot files are signed at install time, so just turn Secure
# Boot on in the BIOS (no Setup Mode); step 3 follows by itself.
#
# The tpm2-device=auto crypttab option on cryptroot lives in hosts/_disko.nix
# (only when the disk is encrypted).
{ inputs, ... }:
{
  flake.modules.nixos.base =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      luks = "/dev/disk/by-partlabel/disk-main-luks";
      # Written by the installer (TPM_ENROLL_KEY in tools/vx/src/cmd/install/plan.rs):
      # a one-time extra LUKS key, removed again once the TPM is enrolled.
      enrollKey = "/persistent/install/tpm-enroll.key";
    in
    {
      imports = [ inputs.lanzaboote.nixosModules.lanzaboote ];

      options.vx.tpmFirstBoot = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Enroll the TPM on the first boot with Secure Boot on (set from disks.json).";
      };

      config = {
        boot.loader.systemd-boot.enable = lib.mkForce false;
        boot.lanzaboote = {
          enable = true;
          pkiBundle = "/var/lib/sbctl";
          configurationLimit = 10;
          autoGenerateKeys.enable = true;
          autoEnrollKeys.enable = true; # includes Microsoft keys by default
        };

        security.tpm2.enable = true;

        systemd.services.tpm-first-boot-enroll =
          lib.mkIf (config.vx.tpmFirstBoot && config.boot.initrd.luks.devices ? cryptroot)
            {
              description = "Enroll the TPM to unlock the disk (first boot with Secure Boot on)";
              wantedBy = [ "multi-user.target" ];
              wants = [ "tpm2.target" ];
              after = [
                "tpm2.target"
                "local-fs.target"
              ];
              unitConfig = {
                ConditionPathExists = enrollKey;
                # Waits until Secure Boot is really on, so PCR 7 is the state the TPM checks later.
                ConditionSecurity = "uefi-secureboot";
              };
              path = [
                config.systemd.package
                pkgs.cryptsetup
                pkgs.gnugrep
                pkgs.coreutils
              ];
              serviceConfig = {
                Type = "oneshot";
                UMask = "0077";
              };
              script = ''
                set -eu
                if systemd-cryptenroll ${luks} | grep -qw tpm2; then
                  echo "a tpm2 token is already enrolled"
                else
                  systemd-cryptenroll --unlock-key-file=${enrollKey} \
                    --tpm2-device=auto --tpm2-pcrs=7 ${luks}
                fi
                # Remove the one-time key's slot (never the last one: without a
                # terminal cryptsetup refuses that instead of asking).
                if cryptsetup open --test-passphrase --key-file=${enrollKey} ${luks}; then
                  cryptsetup luksRemoveKey ${luks} ${enrollKey}
                fi
                rm -f ${enrollKey}
                rmdir --ignore-fail-on-non-empty /persistent/install
                echo "tpm enrolled: the disk unlocks by itself while secure boot is unchanged"
              '';
            };

        environment.systemPackages = [ pkgs.sbctl ];

        preservation.preserveAt."/persistent".directories = [
          {
            directory = "/var/lib/sbctl"; # your Secure Boot keys
            mode = "0700";
          }
          "/var/lib/auto-cryptenroll"
        ];
      };
    };
}
