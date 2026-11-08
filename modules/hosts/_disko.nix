# Every host's drives, from its own modules/hosts/<name>/disks.json
# (flake/hosts.nix passes the path). The installer (`vx install`) writes that
# file with the drives you picked, then runs disko on this layout; it is
# committed (and pushed) so rebuilds keep mounting the same drives. Without
# the file this is the default layout: one encrypted system disk, no swap
# file, no data drives (the device is a placeholder; the installed system
# mounts by partition label, so the disk id only matters while installing).
#
# disks.json (the shape is DisksJson in tools/vx/src/cmd/install/plan.rs):
#   { "system": { "device": "/dev/disk/by-id/...", "encrypt": true, "tpm": true, "swap_gb": 16 },
#     "data": [ { "name": "games", "mount": "/home/vainnor/games", "mode": "format",
#                 "device": "/dev/disk/by-id/...", "encrypt": true },
#               { "name": "d", "mount": "/mnt/d", "mode": "mount",
#                 "uuid": "ABCD1234", "fsType": "ntfs3", "readOnly": false } ] }
#
# System disk "main": ESP + LUKS "cryptroot" (or a plain "root" partition
# when not encrypted) + btrfs labelled "nixos". Impermanence: `/` is a RAM
# disk wiped on every boot. Only these survive:
#   /nix         the store (the system itself)
#   /persistent  state listed in modules/system/impermanence.nix
#   /home        your files, Steam library, browser profiles, projects
#   /var/log     logs
#   /.swap       the swap file, when swap_gb > 0 (zram is always on as well)
# @snapshots holds the hourly btrbk snapshots of @home and @persistent
# (system/backups.nix), reached through the whole-filesystem view at /.btrfs.
#
# Data drives:
#   format  erased, one btrfs (label <name>, subvolume @data) mounted at
#           `mount`; when encrypted, LUKS "data-<name>" unlocked after boot
#           (not in the initrd) with /persistent/etc/keys/<name>.key.
#   mount   kept as it is and mounted by UUID (e.g. a Windows drive via ntfs3).
# Every data drive is `nofail`: a missing one never stops the boot.
{
  inputs,
  user,
  disksFile,
}:
{
  config,
  lib,
  pkgs,
  extendModules,
  ...
}:
let
  disks =
    if builtins.pathExists disksFile then
      lib.importJSON disksFile
    else
      {
        system = {
          device = "/dev/disk/by-id/set-by-the-installer";
          encrypt = true;
          tpm = false;
          swap_gb = 0;
        };
        data = [ ];
      };
  sys = disks.system;
  swapGb = sys.swap_gb or 0;

  # Shared with tools/vx/src/cmd/install/plan.rs (DISK_KEY, DATA_KEYS, INSTALLED_KEYS).
  diskKey = "/tmp/vx-install/disk.key";
  formatKey = name: "/tmp/vx-install/keys/${name}.key";
  installedKey = name: "/persistent/etc/keys/${name}.key";

  btrfsOpts = [
    "compress=zstd"
    "noatime"
  ];
  # A data drive that isn't there must not hold up the boot.
  dataOpts = [
    "nofail"
    "x-systemd.device-timeout=10s"
  ];

  rootDevice =
    if sys.encrypt then "/dev/mapper/cryptroot" else "/dev/disk/by-partlabel/disk-main-root";

  systemBtrfs = {
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
    }
    // lib.optionalAttrs (swapGb > 0) {
      # Not snapshotted (btrbk only takes @home and @persistent).
      "@swap" = {
        mountpoint = "/.swap";
        mountOptions = [ "noatime" ];
        swap.swapfile.size = "${toString swapGb}G";
      };
    };
  };

  mainDisk = {
    type = "disk";
    inherit (sys) device;
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
      }
      // (
        if sys.encrypt then
          {
            luks = {
              size = "100%";
              content = {
                type = "luks";
                name = "cryptroot";
                # Read only by the format script; never part of the system
                # (passwordFile stays out of boot.initrd.luks.devices).
                passwordFile = diskKey;
                settings = {
                  allowDiscards = true;
                  # Unlocks by itself once a TPM2 token is enrolled (see
                  # system/secure-boot.nix); the passphrase always works too.
                  crypttabExtraOpts = [ "tpm2-device=auto" ];
                };
                content = systemBtrfs;
              };
            };
          }
        else
          {
            root = {
              size = "100%";
              content = systemBtrfs;
            };
          }
      );
    };
  };

  formatDrives = builtins.filter (d: d.mode == "format") disks.data;
  mountDrives = builtins.filter (d: d.mode == "mount") disks.data;
  encryptedDrives = builtins.filter (d: d.encrypt) formatDrives;

  dataBtrfs = d: {
    type = "btrfs";
    extraArgs = [
      "-f"
      "-L"
      d.name
    ];
    subvolumes."@data" = {
      mountpoint = d.mount;
      mountOptions = btrfsOpts ++ dataOpts;
    };
  };

  dataDisk = d: {
    type = "disk";
    inherit (d) device;
    content = {
      type = "gpt";
      partitions =
        if d.encrypt then
          {
            luks = {
              size = "100%";
              content = {
                type = "luks";
                name = "data-${d.name}";
                # The raw key file at format time; at boot the same bytes
                # come from /persistent through crypttab below, so no initrd entry.
                settings = {
                  keyFile = formatKey d.name;
                  allowDiscards = true;
                };
                initrdUnlock = false;
                content = dataBtrfs d;
              };
            };
          }
        else
          {
            data = {
              size = "100%";
              content = dataBtrfs d;
            };
          };
    };
  };

  dataDisks = lib.listToAttrs (map (d: lib.nameValuePair "data-${d.name}" (dataDisk d)) formatDrives);

  u = config.users.users.${user};
  uid = if u.uid != null then u.uid else 1000;
  gid = config.users.groups.${u.group}.gid or null;
  owner = [
    "uid=${toString uid}"
    "gid=${toString (if gid != null then gid else 100)}"
  ];
  mountOptions =
    d:
    dataOpts
    ++ lib.optional d.readOnly "ro"
    ++ (
      if d.fsType == "ntfs3" then
        owner ++ [ "windows_names" ]
      else if d.fsType == "exfat" || d.fsType == "vfat" then
        owner ++ [ "umask=0022" ]
      else
        [ "noatime" ]
    );

  # Destroy + format + mount only the data drives that get formatted, under
  # /mnt like disko. For "keep /home" reinstalls: the system disk stays as it
  # is (mount it first with diskoSystemMountScript, so /mnt/home is there), so
  # unlike disko's own scripts this never runs `umount -R /mnt` and never asks
  # for confirmation.
  dataOnly = extendModules {
    modules = [
      {
        disko.devices.disk = lib.mkForce dataDisks;
        disko.devices.nodev = lib.mkForce { };
      }
    ];
  };
  dataDevices = dataOnly.config.disko.devices;
  # Mount (never format) just the system disk and the tmpfs `/` under /mnt,
  # unlocking with DISK_KEY: the other half of a "keep /home" reinstall.
  systemOnly = extendModules {
    modules = [ { disko.devices.disk = lib.mkForce { main = mainDisk; }; } ];
  };
  diskoDataScript =
    if formatDrives == [ ] then
      pkgs.writeShellScript "disko-data" ''
        echo "no data drives to format"
      ''
    else
      pkgs.writeScript "disko-data" ''
        #!${pkgs.bash}/bin/bash
        set -efu
        export PATH=${
          lib.makeBinPath (
            (dataDevices._packages pkgs)
            ++ (with pkgs; [
              bash
              util-linux
              e2fsprogs
              mdadm
              lvm2
              jq
              gnused
              gawk
              coreutils-full
            ])
          )
        }:$PATH
        ${lib.concatMapStrings (d: ''
          $BASH ${inputs.disko}/disk-deactivate/disk-deactivate ${lib.escapeShellArg d.device}
        '') formatDrives}
        ${dataDevices._create}
        ${dataDevices._mount}
      '';
in
{
  options.vx.disks = lib.mkOption {
    type = lib.types.raw;
    readOnly = true;
    default = disks;
    description = "The parsed disks.json (or the defaults when it's missing).";
  };

  config = {
    assertions = [
      {
        assertion = sys.tpm -> sys.encrypt;
        message = "disks.json: system.tpm needs system.encrypt";
      }
    ]
    ++ map (d: {
      assertion = builtins.match "[a-z0-9-]+" d.name != null && lib.hasPrefix "/" d.mount;
      message = "disks.json: data drive \"${d.name}\" needs a name of a-z, 0-9, - and an absolute mount path";
    }) disks.data;

    disko.devices = {
      nodev."/" = {
        fsType = "tmpfs";
        mountOptions = [
          "size=25%"
          "mode=755"
        ];
      };
      disk = {
        main = mainDisk;
      }
      // dataDisks;
    };

    fileSystems = {
      "/nix".neededForBoot = true;
      "/persistent".neededForBoot = true;
      "/var/log".neededForBoot = true;

      # The top of the btrfs filesystem (all subvolumes side by side), for btrbk.
      "/.btrfs" = {
        device = rootDevice;
        fsType = "btrfs";
        options = [
          "subvolid=5"
          "noatime"
          "compress=zstd"
        ];
      };
    }
    // lib.listToAttrs (
      map (
        d:
        lib.nameValuePair d.mount {
          device = "/dev/disk/by-uuid/${d.uuid}";
          inherit (d) fsType;
          options = mountOptions d;
          noCheck = true; # Windows (or whoever made it) owns this filesystem
        }
      ) mountDrives
    );

    # Encrypted data drives open after /persistent is up, with their key file.
    environment.etc.crypttab = lib.mkIf (encryptedDrives != [ ]) {
      mode = "0600";
      text = lib.concatMapStrings (d: ''
        data-${d.name} /dev/disk/by-partlabel/disk-data-${d.name}-luks ${installedKey d.name} nofail,discard,x-systemd.device-timeout=10s
      '') encryptedDrives;
    };
    systemd.tmpfiles.rules = lib.optional (
      encryptedDrives != [ ]
    ) "d /persistent/etc/keys 0700 root root -";

    boot.supportedFilesystems = lib.mkIf (builtins.any (d: d.fsType == "ntfs3") mountDrives) {
      ntfs = true;
    };

    # First Secure Boot boot enrolls the TPM by itself (system/secure-boot.nix).
    vx.tpmFirstBoot = sys.encrypt && sys.tpm;

    system.build = {
      inherit diskoDataScript;
      diskoSystemMountScript = systemOnly.config.system.build.mountScript;
    };
  };
}
