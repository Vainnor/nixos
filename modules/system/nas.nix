# The homelab NAS's SMB shares, mounted at /mnt/nas/<share>, over NetBird when
# away (vpn-1 routes the homelab's 10.0.0.0/8) or directly on the home LAN.
#
# Set meta.nas (flake/meta.nix) to the NAS's address and its shares, and
# put the SMB login in sops (`vx secrets edit`):
#   nas/username, nas/password
# Off while meta.nas is null or secrets aren't ready.
#
# Each share mounts the first time something opens it (automount) and lets go
# after 10 idle minutes, so a NAS that's off or a NetBird that's down never
# holds up boot or a switch; opening the folder then just fails after 15 s.
#
# Big one-off copies (100+ GB) are faster and resumable with rsync/rclone over
# SSH than through SMB at cross-country latency; the mount is for everyday use.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base =
    { config, lib, ... }:
    let
      user = config.users.users.${meta.username};
      creds = config.sops.templates."nas-credentials".path;
      mount = share: {
        name = "/mnt/nas/${share}";
        value = {
          device = "//${meta.nas.host}/${share}";
          fsType = "cifs";
          options = [
            "credentials=${creds}"
            # names, not numbers: the user has no fixed uid (mount.cifs looks them up)
            "uid=${meta.username}"
            "gid=${user.group}"
            "file_mode=0664"
            "dir_mode=0775"
            "vers=3.1.1"
            "noauto"
            "nofail"
            "_netdev"
            "x-systemd.automount"
            "x-systemd.idle-timeout=600"
            "x-systemd.mount-timeout=15"
            # after, not requires: at home the LAN reaches the NAS without NetBird
            "x-systemd.after=netbird.service"
          ];
        };
      };
    in
    lib.mkIf (meta.nas != null && meta.secretsReady) {
      sops.secrets = {
        "nas/username" = { };
        "nas/password" = { };
      };
      sops.templates."nas-credentials".content = ''
        username=${config.sops.placeholder."nas/username"}
        password=${config.sops.placeholder."nas/password"}
      '';

      fileSystems = lib.listToAttrs (map mount meta.nas.shares);
    };

  # The shares in Thunar's (and every GTK file picker's) sidebar.
  flake.modules.homeManager.base =
    { pkgs, lib, ... }:
    lib.mkIf (pkgs.stdenv.hostPlatform.isLinux && meta.nas != null) {
      gtk.gtk3.bookmarks = map (s: "file:///mnt/nas/${s} NAS ${s}") meta.nas.shares;
    };
}
