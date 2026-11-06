# sched_ext with scx_lavd: a latency-aware CPU scheduler built for gaming.
# It keeps frame times smooth while OBS, Discord and the browser run alongside
# the game. Switch back to the kernel's default any time with
# `sudo systemctl stop scx` (or remove this file).
{
  flake.modules.nixos.gaming = {
    services.scx = {
      enable = true;
      scheduler = "scx_lavd";
    };
  };
}
