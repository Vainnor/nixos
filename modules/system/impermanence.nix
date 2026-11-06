# Impermanence: `/` lives in RAM and is wiped every boot (see hosts/arc/_disko.nix).
# Anything not declared in this repo or listed here disappears on reboot.
# When a new service needs to remember something, add its directory below.
# /home and /var/log are their own persistent subvolumes, so they're not listed.
{ inputs, config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base = {
    imports = [ inputs.preservation.nixosModules.default ];

    preservation = {
      enable = true;
      preserveAt."/persistent" = {
        directories = [
          {
            directory = "/var/lib/nixos"; # uid/gid allocations
            inInitrd = true;
          }
          "/var/lib/systemd/timers"
          "/var/lib/systemd/rfkill"
          "/var/lib/systemd/coredump"
          "/var/lib/bluetooth" # paired devices
          "/var/lib/fwupd"
          "/var/lib/power-profiles-daemon"
          "/var/lib/netbird" # NetBird peer identity (stay logged in)
          {
            directory = "/var/lib/private"; # DynamicUser services, e.g. i2pd router identity
            mode = "0700";
          }
          {
            directory = "/etc/NetworkManager/system-connections"; # saved Wi-Fi
            mode = "0700";
          }
          "/etc/lact" # GPU fan curve / power limit
          "/var/cache/tuigreet" # remembered last user/session
        ];
        files = [
          {
            file = "/etc/machine-id";
            inInitrd = true;
          }
        ];
      };
    };

    # Not needed with a machine-id that is already persistent (see preservation docs).
    systemd.suppressedSystemUnits = [ "systemd-machine-id-commit.service" ];

    # SSH host keys generated straight onto the persistent volume.
    services.openssh.hostKeys = [
      {
        type = "ed25519";
        path = "/persistent/etc/ssh/ssh_host_ed25519_key";
      }
    ];

    # /etc/shadow is wiped too, so passwords are declarative. The installer
    # writes the hash; change it later with:
    #   mkpasswd -m yescrypt | sudo tee /persistent/passwords/<user>
    users.mutableUsers = false;
    users.users.${meta.username}.hashedPasswordFile = "/persistent/passwords/${meta.username}";
    users.users.root.hashedPassword = "!"; # root login disabled; use sudo

    # Keep big Nix builds on disk instead of filling the RAM-backed /tmp.
    nix.settings.build-dir = "/nix/var/nix/builds";
    systemd.tmpfiles.rules = [ "d /nix/var/nix/builds 0755 root root 7d -" ];
  };
}
