# Backups, two layers:
#
#   1. btrbk snapshots (local, instant): @home and @persistent every hour,
#      kept 48 hours / 14 days / 6 weeks in @snapshots. Undo a deleted or
#      broken file in seconds: `vx snapshots`, then copy it back out of
#      /.btrfs/@snapshots/<name>. Same disk, so NOT a backup against disk death.
#
#   2. restic to the homelab (off-site, encrypted, deduplicated), every night.
#      Target and password come from sops (restic/repository, restic/password,
#      restic/env); on once meta.resticReady is true. The target is the
#      homelab's rest-server (restic-1, over NetBird only, append-only):
#      restic/env holds its login (RESTIC_REST_USERNAME/PASSWORD) and
#      RESTIC_CACERT, pointing at its pinned certificate
#      (assets/restic/homelab-ca.pem). restic/password is the repository's own
#      key: keep a copy in 1Password, a reinstall can't restore without it.
#
#      Reinstalling: the installer can restore /home/<user> from the latest
#      snapshot before the first boot ("restore /home from the homelab
#      backup", fresh /home only). Its login comes from restic/* when the
#      carried host key decrypts them, else from the 1Password item
#      meta.restoreItem (default "op://Personal/arc restic"; fields repository,
#      password, rest_username, rest_password: the same values), else typed.
#      Later, on the system: `vx restore --restic <path>`.
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
        extraBackupArgs = [
          "--exclude-caches"
          "--retry-lock 30m" # the server's Sunday prune holds the lock
        ];
        timerConfig = {
          OnCalendar = "03:30";
          Persistent = true; # runs at next boot if the PC was off
          RandomizedDelaySec = "30m";
        };
        # No forget/prune from here: the homelab's rest-server is append-only
        # (a compromised PC can't delete its own history) and prunes itself,
        # Sundays, keeping 7 daily, 4 weekly, 6 monthly (homelab-infra, restic-1).
        pruneOpts = [ ];
      };

      # The server's self-signed certificate, pinned (RESTIC_CACERT in restic/env).
      environment.etc."restic/homelab-ca.pem" = lib.mkIf (meta.secretsReady && meta.resticReady) {
        source = ../../assets/restic/homelab-ca.pem;
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
