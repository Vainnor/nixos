# The PC ("arc" on NetBird). host.json picks its aspects and settings, and
# flake/hosts.nix builds it like any other host. This folder holds only what
# arc has and no other machine: its monitors (here), fans (fans.nix), GPU
# passthrough (vfio.nix), hardware (_hardware.nix) and `vx preview`
# (preview.nix). All of them go into flake.modules.nixos.arc, which only arc
# includes.
{ config, ... }:
let
  user = config.meta.username;
in
{
  flake.modules.nixos.arc = {
    home-manager.users.${user} = {
      # Matched by EDID model so connector names (DP-1, ...) don't matter.
      # Samsung on the left, ASUS ultrawide on the right as primary. No negative
      # coordinates (they break XWayland input). Verify names with `wlr-randr`.
      wayland.windowManager.mango.settings.monitor_rule = [
        "model:Odyssey G5,width:2560,height:1440,refresh:165,x:0,y:0,vrr:1"
        "model:VG34VQL3A,width:3440,height:1440,refresh:180,x:2560,y:0,vrr:1,primary:1"
      ];
    };
  };
}
