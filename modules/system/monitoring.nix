# Monitoring for the homelab's Prometheus/Grafana, reachable over NetBird only:
#   arc.netbird.cloud:9100  node exporter (CPU, RAM, disks, network, systemd units)
#   arc.netbird.cloud:9633  SMART health of every drive (NVMe wear, errors, temps)
# Add both as scrape targets on the homelab; Grafana's "Node Exporter Full"
# dashboard (ID 1860) works as-is. smartd also warns on the desktop (wall)
# when a drive starts failing.
{
  flake.modules.nixos.base = {
    services.prometheus.exporters = {
      node = {
        enable = true;
        enabledCollectors = [ "systemd" ];
      };
      smartctl.enable = true;
    };
    services.smartd = {
      enable = true;
      notifications.wall.enable = true;
    };
    networking.firewall.interfaces.wt0.allowedTCPPorts = [
      9100
      9633
    ];
  };
}
