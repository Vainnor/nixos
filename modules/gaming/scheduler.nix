# sched_ext with scx_lavd: a latency-aware CPU scheduler built for gaming.
# It keeps frame times smooth while OBS, Discord and the browser run alongside
# the game. scx_loader starts it at boot; switch schedulers on the fly with
# `scxctl switch --sched <name>`, or back to the kernel's default with
# `scxctl stop` (or remove this file).
{
  flake.modules.nixos.gaming = {
    services.scx-loader = {
      enable = true;
      config.default_sched = "scx_lavd";
    };
  };
}
