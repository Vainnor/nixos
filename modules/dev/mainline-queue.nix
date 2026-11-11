# Mainline's job-queue worker on the PC: target `linux-pc` (x86_64 Linux), the
# NixOS successor of the old WSL worker. `just queue run linux-pc <recipe>`
# from any machine runs the recipe here at the job's commit.
#
# It's Mainline's own bin/queue-service.sh (from the queue's `queue-bin`
# branch, cloned into ~/mainline-queue-bin), run in the #mainline dev shell
# (projects/mainline.nix) as a user service, so it starts at boot (linger,
# dev/t3.nix) and keeps itself updated between jobs. Logs:
# ~/mainline-queue/worker.log. Stop it between jobs: touch ~/mainline-queue/stop.
#
# The queue host (VM 520) lets in ~/.ssh/mainline-queue, a key made for this
# PC (its public half is in the queue user's authorized_keys); the service
# doesn't start without it. Trust: anyone who can push to the queue runs code
# here as you, as on every worker host (see Mainline's AGENTS.md).
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.homeManager.dev =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      home = config.home.homeDirectory;
      # The queue on VM 520, by NetBird address (as Mainline's bin/queue-lib.sh).
      url = "ssh://mainline-ci@100.104.113.170/home/mainline-ci/queue.git";
      key = "${home}/.ssh/mainline-queue";
      bin = "${home}/mainline-queue-bin";
      ssh = "${pkgs.openssh}/bin/ssh -i ${key} -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=accept-new";
      start = pkgs.writeShellScript "mainline-queue" ''
        set -eu
        export GIT_SSH_COMMAND=${lib.escapeShellArg ssh}
        if [ ! -d ${bin}/.git ]; then
          ${pkgs.git}/bin/git clone -q -b queue-bin ${url} ${bin}
        fi
        exec /run/current-system/sw/bin/nix develop ${meta.flakePath}#mainline \
          --command bash ${bin}/bin/queue-service.sh linux-pc
      '';
    in
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      systemd.user.services.mainline-queue = {
        Unit = {
          Description = "Mainline queue worker (linux-pc)";
          ConditionPathExists = key;
          After = [ "network-online.target" ];
          # A switch doesn't restart it (that would kill a running job); a
          # changed unit takes effect at the next boot, or restart it between
          # jobs yourself.
          X-RestartIfChanged = false;
        };
        Service = {
          ExecStart = "${start}";
          Environment = [
            "MAINLINE_QUEUE_URL=${url}"
            "MAINLINE_QUEUE_KEY=${key}"
            "PATH=${
              lib.makeBinPath [
                pkgs.git
                pkgs.openssh
                pkgs.bash
                pkgs.coreutils
                pkgs.gnugrep
                pkgs.gnused
                pkgs.findutils
                pkgs.procps
              ]
            }:/run/current-system/sw/bin"
          ];
          Restart = "always";
          RestartSec = 60;
        };
        Install.WantedBy = [ "default.target" ];
      };
    };
}
