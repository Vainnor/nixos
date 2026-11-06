# NetworkManager + systemd-resolved, and NetBird so the PC stays on your mesh.
#
# From sops (once meta.secretsReady is on):
#   wifi/ssid, wifi/psk    your Wi-Fi, connected on first boot (Ethernet still wins)
#   netbird/setup_key      joins NetBird by itself; no `netbird up` login needed.
#                          Make a reusable or one-off key in the NetBird dashboard
#                          (Setup Keys). Leave it empty to log in by hand instead.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base =
    { config, lib, ... }:
    lib.mkMerge [
      {
        networking.networkmanager = {
          enable = true;
          dns = "systemd-resolved";
        };
        services.resolved.enable = true;

        # Interface wt0.
        services.netbird.enable = true;
        # WireGuard + strict reverse-path filtering don't mix.
        networking.firewall.checkReversePath = "loose";
      }
      (lib.mkIf meta.secretsReady {
        sops.secrets = {
          "wifi/ssid" = { };
          "wifi/psk" = { };
          "netbird/setup_key" = { };
        };
        sops.templates."wifi.env".content = ''
          WIFI_SSID=${config.sops.placeholder."wifi/ssid"}
          WIFI_PSK=${config.sops.placeholder."wifi/psk"}
        '';
        networking.networkmanager.ensureProfiles = {
          environmentFiles = [ config.sops.templates."wifi.env".path ];
          profiles.home-wifi = {
            connection = {
              id = "home-wifi";
              type = "wifi";
              autoconnect = true;
            };
            wifi = {
              mode = "infrastructure";
              ssid = "$WIFI_SSID";
            };
            wifi-security = {
              key-mgmt = "wpa-psk";
              psk = "$WIFI_PSK";
            };
            ipv4.method = "auto";
            ipv6.method = "auto";
          };
        };

        # Only logs in when NetBird says it needs to; a stored login is kept.
        services.netbird.clients.default.login = {
          enable = true;
          setupKeyFile = config.sops.secrets."netbird/setup_key".path;
        };
      })
    ];
}
