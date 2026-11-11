# Mainline's job-queue worker on the PC: target `linux-pc` (x86_64 Linux), the
# NixOS successor of the old WSL worker. `just queue run linux-pc <recipe>`
# from any machine runs the recipe here at the job's commit.
#
# It's Mainline's own bin/queue-service.sh (from the queue's `queue-bin`
# branch, cloned into ~/Programing/Mainline-sim/mainline-queue-bin), run in the
# #mainline dev shell (projects/mainline.nix) as a user service, so it starts at
# boot (linger, dev/t3.nix) and keeps itself updated between jobs. Its clone of
# Mainline and each job's attempt live in ~/Programing/Mainline-sim/mainline-queue
# (MAINLINE_QUEUE_WORKDIR). The service's own log, pid and stop file are still
# in ~/mainline-queue (queue-service.sh doesn't take a directory yet): logs in
# ~/mainline-queue/worker.log; stop it between jobs: touch ~/mainline-queue/stop.
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
      root = "${home}/Programing/Mainline-sim";
      bin = "${root}/mainline-queue-bin";
      work = "${root}/mainline-queue";
      ssh = "${pkgs.openssh}/bin/ssh -i ${key} -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=accept-new";
      start = pkgs.writeShellScript "mainline-queue" ''
        set -eu
        export GIT_SSH_COMMAND=${lib.escapeShellArg ssh}
        mkdir -p ${root}
        # From when both lived in ~: move them (same disk, so no copying).
        if [ -d ${home}/mainline-queue-bin/.git ] && [ ! -e ${bin} ]; then
          mv ${home}/mainline-queue-bin ${bin}
        fi
        if [ ! -e ${work} ]; then
          mkdir -p ${work}
          for d in clone attempts logs; do
            if [ -e ${home}/mainline-queue/$d ]; then mv ${home}/mainline-queue/$d ${work}/; fi
          done
        fi
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
            "MAINLINE_QUEUE_WORKDIR=${work}"
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
