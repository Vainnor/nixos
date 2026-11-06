# Homelab binary cache (Attic), on once meta.binaryCache is set.
# Both machines download from it before cache.nixos.org's misses get built
# locally; the PC pushes everything it builds (attic watch-store), so the Mac
# and a reinstall reuse it. If the homelab is unreachable, Nix moves on after
# a few seconds and builds or uses cache.nixos.org instead.
{ config, lib, ... }:
let
  inherit (config) meta;
  bc = meta.binaryCache;
  substituter = lib.optionalAttrs (bc != null) {
    nix.settings = {
      extra-substituters = [ "${bc.endpoint}/${bc.cache}" ];
      extra-trusted-public-keys = [ bc.publicKey ];
      connect-timeout = 5;
      fallback = true;
    };
  };
in
{
  flake.modules.darwin.base = substituter;

  flake.modules.nixos.base =
    { config, pkgs, ... }:
    lib.mkMerge [
      substituter
      { environment.systemPackages = [ pkgs.attic-client ]; }
      (lib.mkIf (bc != null && meta.secretsReady) {
        sops.secrets."attic/token" = { };

        # Pushes every new store path to the cache as it appears.
        systemd.services.attic-push = {
          description = "Push new Nix store paths to the homelab cache";
          wantedBy = [ "multi-user.target" ];
          after = [ "network-online.target" ];
          wants = [ "network-online.target" ];
          environment.HOME = "/var/lib/attic-push";
          serviceConfig = {
            StateDirectory = "attic-push";
            Restart = "on-failure";
            RestartSec = 30;
          };
          script = ''
            ${pkgs.attic-client}/bin/attic login homelab ${bc.endpoint} "$(cat ${
              config.sops.secrets."attic/token".path
            })"
            exec ${pkgs.attic-client}/bin/attic watch-store homelab:${bc.cache}
          '';
        };
      })
    ];
}
