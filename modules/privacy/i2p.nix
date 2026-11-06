# i2pd: C++ I2P router. Web console http://127.0.0.1:7070, HTTP proxy :4444,
# SOCKS :4447, all bound to localhost. The LibreWolf `i2p` profile uses it.
# Runs firewalled (no inbound port), which is fine for browsing; open the
# router port on TCP+UDP later if you want to be a better-connected peer.
{
  flake.modules.nixos.privacy = {
    services.i2pd = {
      enable = true;
      settings = {
        bandwidth = 1024; # KB/s shared with the network
        http.address = "127.0.0.1";
        httpproxy.address = "127.0.0.1";
        socksproxy.address = "127.0.0.1";
      };
    };
  };
}
