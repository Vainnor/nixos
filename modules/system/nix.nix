# Nix itself: flakes, store optimisation, garbage collection via nh, and an
# explicit allow-list for unfree packages (each feature adds its own names).
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base =
    {
      lib,
      config,
      pkgs,
      ...
    }:
    {
      options.unfree.allowed = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
        description = "Unfree package names allowed on this system.";
      };

      config = {
        nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) config.unfree.allowed;

        nix = {
          channel.enable = false;
          settings = {
            experimental-features = [
              "nix-command"
              "flakes"
            ];
            auto-optimise-store = true;
            trusted-users = [ "root" ];
            allowed-users = [ "@wheel" ];
          };
        };

        # `nh os switch` to rebuild, `nh clean` handled automatically.
        programs.nh = {
          enable = true;
          flake = meta.flakePath;
          clean = {
            enable = true;
            extraArgs = "--keep 10 --keep-since 14d";
          };
        };

        environment.systemPackages = with pkgs; [
          nix-output-monitor
          mkpasswd # `vx password`
        ];
      };
    };
}
