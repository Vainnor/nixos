# Declarative layout of the dedicated 1TB Crucial P310. Applied ONLY by the
# installer (modules/flake/install.nix), which finds the disk by model and
# asks before erasing it. The Windows drives are never referenced here.
#
# Impermanence: `/` is a RAM disk wiped on every boot. Only these survive:
#   /nix         the store (the system itself)
#   /persistent  state listed in modules/system/impermanence.nix
#   /home        your files, Steam library, browser profiles, projects
#   /var/log     logs
# @snapshots holds the hourly btrbk snapshots of @home and @persistent
# (system/backups.nix), reached through the whole-filesystem view at /.btrfs.
let
  btrfsOpts = [
    "compress=zstd"
    "noatime"
  ];
in
{
  disko.devices.nodev."/" = {
    fsType = "tmpfs";
    mountOptions = [
      "size=25%"
      "mode=755"
    ];
  };

  disko.devices.disk.main = {
    type = "disk";
    # Filled in by the installer at install time; the installed system mounts by
    # partition label, so this value is never used after install.
    device = "/dev/disk/by-id/nvme-CT1000P310SSD8_detected-by-installer";
    content = {
      type = "gpt";
      partitions = {
        ESP = {
          size = "1G";
          type = "EF00";
          content = {
            type = "filesystem";
            format = "vfat";
            mountpoint = "/boot";
            mountOptions = [ "umask=0077" ];
          };
        };
        luks = {
          size = "100%";
          content = {
            type = "luks";
            name = "cryptroot";
            settings.allowDiscards = true;
            content = {
              type = "btrfs";
              extraArgs = [
                "-f"
                "-L"
                "nixos"
              ];
              subvolumes = {
                "@nix" = {
                  mountpoint = "/nix";
                  mountOptions = btrfsOpts;
                };
                "@persistent" = {
                  mountpoint = "/persistent";
                  mountOptions = btrfsOpts;
                };
                "@home" = {
                  mountpoint = "/home";
                  mountOptions = btrfsOpts;
                };
                "@log" = {
                  mountpoint = "/var/log";
                  mountOptions = btrfsOpts;
                };
                "@snapshots" = { };
              };
            };
          };
        };
      };
    };
  };

  fileSystems = {
    "/nix".neededForBoot = true;
    "/persistent".neededForBoot = true;
    "/var/log".neededForBoot = true;
  };

  # The top of the btrfs filesystem (all subvolumes side by side), for btrbk.
  fileSystems."/.btrfs" = {
    device = "/dev/mapper/cryptroot";
    fsType = "btrfs";
    options = [
      "subvolid=5"
      "noatime"
      "compress=zstd"
    ];
  };
}
