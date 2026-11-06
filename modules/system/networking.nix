# NetworkManager + systemd-resolved, and NetBird so the PC stays on your mesh.
{
  flake.modules.nixos.base = {
    networking.networkmanager = {
      enable = true;
      dns = "systemd-resolved";
    };
    services.resolved.enable = true;

    # Interface wt0. Run `netbird up` once after install to log in.
    services.netbird.enable = true;
    # WireGuard + strict reverse-path filtering don't mix.
    networking.firewall.checkReversePath = "loose";
  };
}
