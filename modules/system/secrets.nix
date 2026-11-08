# sops-nix: secrets live encrypted in secrets/secrets.yaml (safe in a public
# repo) and are decrypted at boot into /run/secrets with the PC's SSH host key.
# Recipients are in .sops.yaml: your Mac's age key (to edit) and every host.
#
# Features declare what they need, guarded by meta.secretsReady, e.g.
#   sops.secrets."twitch/stream_key".owner = "vainnor";
#
# New machines get their secrets by themselves (only with meta.secretsReady,
# so never in a fork that turned it off):
#   - The installer adds the new host's key to .sops.yaml and pushes.
#   - Every machine that can already decrypt runs `vx secrets rekey --push`
#     hourly (vx-secrets-rekey): it pulls, re-encrypts secrets.yaml for any
#     recipient newly listed in .sops.yaml, commits and pushes. Nothing to do:
#     it exits right away.
#   - The new machine runs `vx secrets wait` every 10 minutes
#     (vx-secrets-wait): it pulls and, once it can decrypt, switches to the
#     flake. Then it touches /persistent/install/secrets-ok and never runs again.
#
# Both run as root (decrypting needs the host key, which only root can read:
# sops gets it as SOPS_AGE_SSH_PRIVATE_KEY_FILE), but git and gh run as you
# through small wrappers (setpriv, your HOME), so the repo stays yours and a
# push uses your own GitHub login (gh's credential helper). The key never
# reaches a process running as you. Commits made this way are unsigned
# (1Password can't sign without you).
{ config, inputs, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      user = meta.username;
      home = config.users.users.${user}.home;
      group = config.users.users.${user}.group;
      flake = meta.flakePath;
      vx = pkgs.callPackage ../../tools/vx/package.nix { };
      hostKey = "/persistent/etc/ssh/ssh_host_ed25519_key";
      # Written once the new machine has its secrets (vx may write it too).
      secretsOk = "/persistent/install/secrets-ok";

      # `git` / `gh` that run as the user when called by root.
      asUser =
        name: real:
        pkgs.writeShellScriptBin name ''
          if [ "$(${pkgs.coreutils}/bin/id -u)" -ne 0 ]; then exec ${real} "$@"; fi
          uid=$(${pkgs.coreutils}/bin/id -u ${user})
          export HOME=${home} USER=${user} LOGNAME=${user}
          # gh may keep its token in the desktop keyring: reach it when you're logged in.
          if [ -S "/run/user/$uid/bus" ]; then
            export XDG_RUNTIME_DIR="/run/user/$uid" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus"
          fi
          exec ${pkgs.util-linux}/bin/setpriv --reuid=${user} --regid=${group} --init-groups -- ${real} "$@"
        '';
      userTools = pkgs.symlinkJoin {
        name = "vx-secrets-as-user";
        paths = [
          (asUser "git" "${pkgs.git}/bin/git")
          (asUser "gh" "${pkgs.gh}/bin/gh")
        ];
      };

      common = {
        # userTools first: its git/gh win over the real ones.
        path = [
          userTools
          vx
          pkgs.sops
          pkgs.ssh-to-age
          pkgs.openssh
          pkgs.nh
          config.system.build.nixos-rebuild
          config.nix.package
          pkgs.util-linux
        ];
        environment = {
          VX_REPO = flake;
          SOPS_AGE_SSH_PRIVATE_KEY_FILE = hostKey;
          # The commit can't be signed unattended.
          GIT_CONFIG_COUNT = "1";
          GIT_CONFIG_KEY_0 = "commit.gpgsign";
          GIT_CONFIG_VALUE_0 = "false";
        };
        wants = [ "network-online.target" ];
        after = [ "network-online.target" ];
        # Don't restart in the middle of a switch it started itself.
        restartIfChanged = false;
        unitConfig.X-StopOnRemoval = false;
        serviceConfig = {
          Type = "oneshot";
          WorkingDirectory = flake;
          # Anything root may have left in the repo goes back to you.
          ExecStopPost = "${pkgs.coreutils}/bin/chown -R -h --from=root ${user}:${group} ${flake}";
        };
      };
    in
    {
      imports = [ inputs.sops-nix.nixosModules.sops ];

      sops = {
        defaultSopsFile = ../../secrets/secrets.yaml;
        age.sshKeyPaths = [ hostKey ];
        gnupg.sshKeyPaths = [ ];
      };

      systemd = lib.mkIf meta.secretsReady {
        # A new machine waiting for another machine to re-encrypt for it.
        services.vx-secrets-wait = lib.recursiveUpdate common {
          description = "Wait for secrets.yaml to be re-encrypted for this host, then switch";
          unitConfig.ConditionPathExists = [
            "!${secretsOk}"
            "${flake}/flake.nix"
          ];
          script = ''
            done_() { mkdir -p ${dirOf secretsOk}; touch ${secretsOk}; }
            # The clone is the commit this system was built from: decrypting it
            # means this system already has its secrets (e.g. a carried-over key).
            if vx secrets ready; then
              echo "this host can decrypt secrets.yaml; nothing to wait for"
              done_
              exit 0
            fi
            if vx secrets wait; then
              echo "secrets.yaml re-encrypted for this host and switched"
              done_
            else
              # Not re-encrypted yet (or offline): not an error, try again later.
              echo "not re-encrypted for this host yet; trying again in 10 minutes"
            fi
          '';
        };
        timers.vx-secrets-wait = {
          wantedBy = [ "timers.target" ];
          unitConfig.ConditionPathExists = "!${secretsOk}";
          timerConfig = {
            OnBootSec = "2min";
            OnUnitActiveSec = "10min";
          };
        };

        # Any machine that can decrypt re-encrypts for newly listed hosts.
        services.vx-secrets-rekey = lib.recursiveUpdate common {
          description = "Re-encrypt secrets.yaml for hosts newly listed in .sops.yaml, and push";
          unitConfig.ConditionPathExists = [
            hostKey
            "${flake}/flake.nix"
          ];
          serviceConfig = {
            # A host that can't decrypt skips (cleanly) without touching anything.
            ExecCondition = "${lib.getExe vx} secrets ready";
            ExecStart = "${lib.getExe vx} secrets rekey --push";
          };
        };
        timers.vx-secrets-rekey = {
          wantedBy = [ "timers.target" ];
          timerConfig = {
            OnBootSec = "15min";
            OnUnitActiveSec = "1h";
            RandomizedDelaySec = "5min";
          };
        };
      };
    };
}
