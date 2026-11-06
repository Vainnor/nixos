# One-command install from any NixOS installer ISO:
#
#   sudo nix --extra-experimental-features 'nix-command flakes' run github:vainnor/nixos
#
# - Finds the target disk by model (meta.installDiskModel), refusing anything
#   that looks like Windows (NTFS / Microsoft partitions).
# - Shows it and makes you type YES before erasing it.
# - Partitions + encrypts it (asks for the disk password), stores your login
#   password hash on /persistent, installs the system, and leaves this repo in ~/nixos.
#
# The installed system mounts by partition label, so the disk id only matters here.
{
  inputs,
  self,
  lib,
  config,
  ...
}:
let
  inherit (config) meta;
in
{
  perSystem =
    { pkgs, system, ... }:
    let
      installer = pkgs.writeShellApplication {
        name = "install-nixos";
        runtimeInputs = [
          inputs.disko.packages.${system}.disko
          pkgs.nixos-install-tools
          pkgs.util-linux
          pkgs.coreutils
          pkgs.gnugrep
          pkgs.git
          pkgs.mkpasswd
          pkgs.openssh
          pkgs.ssh-to-age
        ];
        text = ''
          HOST="''${1:-arc}"
          FLAKE="${self}"
          MODEL="${meta.installDiskModel}"
          USERNAME="${meta.username}"
          NIX=(nix --extra-experimental-features "nix-command flakes")

          [ "$(id -u)" = 0 ] || { echo "Run with sudo."; exit 1; }
          [ -d /sys/firmware/efi ] || { echo "Not booted in UEFI mode. Reboot the USB stick in UEFI mode."; exit 1; }

          # Find the target disk by model, ignoring partition / eui / wwn aliases.
          mapfile -t CANDIDATES < <(find /dev/disk/by-id -maxdepth 1 -name "nvme-''${MODEL}_*" ! -name '*-part*' ! -name '*_1' | sort)
          if [ "''${#CANDIDATES[@]}" -ne 1 ]; then
            echo "Expected exactly one disk matching model '$MODEL', found ''${#CANDIDATES[@]}:"
            printf '  %s\n' "''${CANDIDATES[@]}"
            lsblk -d -o NAME,SIZE,MODEL,SERIAL
            exit 1
          fi
          DISK="''${CANDIDATES[0]}"
          REAL="$(readlink -f "$DISK")"

          # Never touch anything Windows-like.
          if lsblk -n -o FSTYPE,PARTTYPENAME "$REAL" | grep -qiE 'ntfs|microsoft|bitlocker'; then
            echo "$DISK has NTFS / Microsoft partitions. Refusing."; exit 1
          fi
          if lsblk -n -o MOUNTPOINTS "$REAL" | grep -q '/iso'; then
            echo "$DISK is the installer USB. Refusing."; exit 1
          fi

          echo
          lsblk -o NAME,SIZE,MODEL,SERIAL,FSTYPE,LABEL "$REAL"
          echo
          echo "!!! EVERYTHING on $DISK ($REAL) will be ERASED and replaced with NixOS ($HOST)."
          read -rp "Type YES to continue: " CONFIRM
          [ "$CONFIRM" = "YES" ] || { echo "Aborted. Nothing was changed."; exit 1; }

          echo ">> Partitioning + encrypting (choose your disk password)"
          DISKO_SCRIPT="$("''${NIX[@]}" build --no-link --print-out-paths --impure --expr "
            let
              f = builtins.getFlake \"path:$FLAKE\";
              sys = f.nixosConfigurations.$HOST.extendModules {
                modules = [ { disko.devices.disk.main.device = f.inputs.nixpkgs.lib.mkForce \"$DISK\"; } ];
              };
            in sys.config.system.build.diskoScript")"
          "$DISKO_SCRIPT"

          echo ">> Choose the login password for $USERNAME"
          # / is wiped every boot, so the hash lives on the persistent volume.
          install -d -m 0700 /mnt/persistent/passwords
          while true; do
            read -rsp "Password: " PW1; echo
            read -rsp "Repeat:   " PW2; echo
            [ -n "$PW1" ] && [ "$PW1" = "$PW2" ] && break
            echo "Empty or didn't match, try again."
          done
          HASH="$(printf '%s' "$PW1" | mkpasswd -m yescrypt --stdin)"
          unset PW1 PW2
          printf '%s\n' "$HASH" > "/mnt/persistent/passwords/$USERNAME"
          chmod 0600 "/mnt/persistent/passwords/$USERNAME"

          echo ">> Creating the SSH host key (also the key that decrypts secrets)"
          install -d -m 0755 /mnt/persistent/etc/ssh
          ssh-keygen -q -t ed25519 -N "" -C "root@$HOST" -f /mnt/persistent/etc/ssh/ssh_host_ed25519_key
          AGE_KEY="$(ssh-to-age < /mnt/persistent/etc/ssh/ssh_host_ed25519_key.pub)"

          echo ">> Installing (this downloads the whole system, give it a while)"
          nixos-install --flake "path:$FLAKE#$HOST" --no-root-passwd --no-channel-copy

          echo ">> Putting this repo in /home/$USERNAME/nixos"
          if ! git clone "${meta.repoUrl}" "/mnt/home/$USERNAME/nixos" 2>/dev/null; then
            cp -r "$FLAKE" "/mnt/home/$USERNAME/nixos"
            chmod -R u+w "/mnt/home/$USERNAME/nixos"
          fi
          nixos-enter --root /mnt -c "chown -R $USERNAME:users /home/$USERNAME/nixos"

          echo
          echo "Done. Remove the USB stick and reboot. Pick the NixOS drive with F11 if Windows starts."
          echo "After logging in: sudo netbird up, then gh auth login (so git push works)"
          echo
          echo "Secrets: on the Mac run   just secrets-add-host $AGE_KEY"
          echo "(also saved in ~/nixos/HOST_AGE_KEY.txt)"
          echo "$AGE_KEY" > "/mnt/home/$USERNAME/nixos/HOST_AGE_KEY.txt"
          nixos-enter --root /mnt -c "chown $USERNAME:users /home/$USERNAME/nixos/HOST_AGE_KEY.txt"
        '';
      };
    in
    {
      apps = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux rec {
        install = {
          type = "app";
          meta.description = "Erase the dedicated drive and install NixOS (asks before wiping)";
          program = lib.getExe installer;
        };
        default = install;
      };
      packages = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux { install-nixos = installer; };
    };
}
