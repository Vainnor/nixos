# The PC ("arc" on NetBird). This file only picks features and sets what is
# unique to this machine; everything else lives in the feature files.
{ inputs, config, ... }:
let
  inherit (config.flake.modules) nixos;
  user = config.meta.username;
in
{
  flake.nixosConfigurations.arc = inputs.nixpkgs.lib.nixosSystem {
    modules = with nixos; [
      base
      desktop
      gaming
      dev
      privacy
      arc
    ];
  };

  flake.modules.nixos.arc = {
    imports = [
      inputs.disko.nixosModules.disko
      ./_disko.nix
      ./_hardware.nix
    ];

    networking.hostName = "arc";

    # Set once at install time; never bump it casually (it is NOT the NixOS version you run).
    system.stateVersion = "26.11";

    home-manager.users.${user} = {
      home.stateVersion = "26.11";

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
