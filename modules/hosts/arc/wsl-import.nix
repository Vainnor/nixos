# The move from WSL (2026-11): Windows' D: drive at /mnt/d, and a one-time
# import of the WSL home that was packed there (D:\arc-migrate\home.tar.zst):
# the work repos with every local branch, both Claude logins, Codex, gh, the
# SSH keys and T3 Code itself (threads, projects and its identity, so other
# devices see the same "Arc"). Same user, same paths, so it all carries on.
#
# It runs once, before home-manager, and leaves ~/.local/state/wsl-import.done;
# remove that to run it again (files in ~ that the archive also holds are
# overwritten). Without the drive or the archive it does nothing.
#
# Commits are signed with the key WSL used (~/.ssh/mainline-signing, already a
# signing key on GitHub), so agents commit without waiting on 1Password.
{ config, ... }:
let
  user = config.meta.username;
  home = "/home/${user}";
  dir = "/mnt/d/arc-migrate";
  repos = [
    "Programing/Mainline-sim/Mainline"
    "Programing/ois"
    "Programing/aviodeck"
    "Programing/aviodeck-link"
  ];
in
{
  flake.modules.nixos.arc =
    { lib, pkgs, ... }:
    {
      # D: ("2TB HDD"). Windows' Fast Startup is off, so it mounts read-write.
      fileSystems."/mnt/d" = {
        device = "/dev/disk/by-uuid/B01EB32F1EB2EE0A";
        fsType = "ntfs3";
        options = [
          "nofail"
          "x-systemd.device-timeout=10s"
          "uid=1000"
          "gid=100"
          "windows_names"
        ];
      };
      boot.supportedFilesystems.ntfs = true;

      systemd.services.wsl-import = {
        description = "Import the WSL home from D: (once)";
        wantedBy = [ "multi-user.target" ];
        before = [ "home-manager-${user}.service" ];
        unitConfig = {
          RequiresMountsFor = dir;
          ConditionPathExists = [
            "${dir}/home.tar.zst"
            "!${home}/.local/state/wsl-import.done"
          ];
        };
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = user;
          Group = "users";
          TimeoutStartSec = "30min";
        };
        path = with pkgs; [
          gnutar
          zstd
          coreutils
          findutils
          git
        ];
        script = ''
          set -eu
          want=$(cut -d' ' -f1 ${dir}/home.tar.zst.sha256)
          have=$(sha256sum ${dir}/home.tar.zst | cut -d' ' -f1)
          if [ "$want" != "$have" ]; then
            echo "home.tar.zst doesn't match its sha256; not importing" >&2
            exit 1
          fi
          # gh's config.yml is home-manager's; the login (hosts.yml) comes along.
          tar --zstd -xf ${dir}/home.tar.zst -C ${home} --no-same-owner \
            --exclude=.config/gh/config.yml
          # The worktrees stayed behind (their branches are in .git).
          for r in ${lib.concatStringsSep " " repos}; do
            [ -d "${home}/$r/.git" ] && git -C "${home}/$r" worktree prune || true
          done
          chmod 700 ${home}/.ssh
          find ${home}/.ssh -type f ! -name '*.pub' ! -name 'known_hosts*' \
            ! -name allowed_signers ! -name config -exec chmod 600 {} +
          mkdir -p ${home}/.local/state
          date -Is > ${home}/.local/state/wsl-import.done
        '';
      };

      home-manager.users.${user}.programs.git.signing = {
        key = lib.mkForce "${home}/.ssh/mainline-signing";
        signer = lib.mkForce "${pkgs.openssh}/bin/ssh-keygen";
      };
    };
}
