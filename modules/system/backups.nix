# Backups, two layers:
#
#   1. btrbk snapshots (local, instant): @home and @persistent every hour,
#      kept 48 hours / 14 days / 6 weeks in @snapshots. Undo a deleted or
#      broken file in seconds: `just snapshots`, then copy it back out of
#      /.btrfs/@snapshots/<name>. Same disk, so NOT a backup against disk death.
#
#   2. restic to the homelab (off-site, encrypted, deduplicated), every night.
#      Target and password come from sops (restic/repository, restic/password,
#      restic/env); on once meta.resticReady is true. Repository examples:
#        rest:https://user:pass@restic.homelab.lan/arc   (restic REST server)
#        sftp:vainnor@nas:/backups/arc                    (any SSH box)
#        s3:https://minio.homelab.lan/backups/arc         (MinIO; keys in restic/env)
#
# Not backed up: caches, the Steam library (re-downloadable), VM disk images,
# Docker data, and ~/Videos (recordings and replays are big; copy keepers off).
{ config, ... }:
let
  inherit (config) meta;
  home = "/home/${meta.username}";
in
{
  flake.modules.nixos.base =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      services.btrbk.instances.local = {
        onCalendar = "hourly";
        settings = {
          timestamp_format = "long";
          snapshot_preserve_min = "6h";
          snapshot_preserve = "48h 14d 6w";
          volume."/.btrfs" = {
            snapshot_dir = "@snapshots";
            subvolume = {
              "@home" = { };
              "@persistent" = { };
            };
          };
        };
      };

      sops.secrets = lib.mkIf (meta.secretsReady && meta.resticReady) {
        "restic/repository" = { };
        "restic/password" = { };
        "restic/env" = { };
      };

      services.restic.backups.homelab = lib.mkIf (meta.secretsReady && meta.resticReady) {
        initialize = true;
        repositoryFile = config.sops.secrets."restic/repository".path;
        passwordFile = config.sops.secrets."restic/password".path;
        environmentFile = config.sops.secrets."restic/env".path;
        paths = [
          "/home"
          "/persistent"
        ];
        exclude = [
          "/home/*/.cache"
          "/home/*/.local/share/Steam"
          "/home/*/.local/share/Trash"
          "/home/*/.local/share/docker"
          "/home/*/.local/share/containers"
          "/home/*/Videos"
          "/persistent/var/lib/libvirt/images"
        ];
        extraBackupArgs = [ "--exclude-caches" ];
        timerConfig = {
          OnCalendar = "03:30";
          Persistent = true; # runs at next boot if the PC was off
          RandomizedDelaySec = "30m";
        };
        pruneOpts = [
          "--keep-daily 7"
          "--keep-weekly 5"
          "--keep-monthly 12"
        ];
      };

      # restic's local index cache (otherwise rebuilt after every reboot).
      preservation.preserveAt."/persistent".directories = [ "/var/cache/restic-backups-homelab" ];

      environment.systemPackages = [
        pkgs.restic
        pkgs.btrbk
      ];
    };

  # Caches and the Steam library become their own subvolumes, which keeps them
  # out of the snapshots (a deleted game frees its space right away).
  flake.modules.homeManager.base =
    { pkgs, lib, ... }:
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      home.activation.cacheSubvolumes = lib.hm.dag.entryBefore [ "writeBoundary" ] ''
        for d in "${home}/.cache" "${home}/.local/share/Steam"; do
          if [ ! -e "$d" ] && [ "$(${pkgs.coreutils}/bin/stat -f -c %T "${home}")" = btrfs ]; then
            run mkdir -p "$(dirname "$d")"
            run ${pkgs.btrfs-progs}/bin/btrfs -q subvolume create "$d"
          fi
        done
      '';
    };
}
