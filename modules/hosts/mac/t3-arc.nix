# The Mac's T3 links to the PC's T3 server (dev/t3.nix) at
# http://localhost:13773. T3 only links over plain http to this machine or a
# Tailscale address (it doesn't know NetBird's 100.x addresses are encrypted
# too), so this forwards that local port to the PC's 3773 over NetBird. It
# listens on 127.0.0.1 alone, and the traffic to the PC goes through NetBird's
# WireGuard tunnel.
#
# Until this Mac runs nix-darwin, a hand-made LaunchAgent
# (com.vainnor.t3-arc-forward, ~/.t3/forward) does the same on the same port;
# remove it when this one takes over.
{
  flake.modules.darwin.mac =
    { pkgs, ... }:
    {
      launchd.user.agents.t3-arc = {
        serviceConfig = {
          ProgramArguments = [
            "${pkgs.socat}/bin/socat"
            "TCP-LISTEN:13773,bind=127.0.0.1,reuseaddr,fork"
            "TCP:arc.netbird.cloud:3773"
          ];
          RunAtLoad = true;
          KeepAlive = true;
        };
      };
    };
}
