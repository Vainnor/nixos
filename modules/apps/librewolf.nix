# LibreWolf. Two profiles: `default` for normal browsing and `i2p`, which sends
# everything through the local i2pd HTTP proxy so .i2p traffic never mixes with
# your normal session. Launch it with `librewolf -P i2p` (or Super+Shift+B).
{
  flake.modules.homeManager.desktop = {
    programs.librewolf = {
      enable = true;
      settings = {
        "privacy.resistFingerprinting" = true;
        "privacy.clearOnShutdown.history" = false;
        "privacy.clearOnShutdown.downloads" = false;
        "webgl.disabled" = false;
        "media.ffmpeg.vaapi.enabled" = true; # hardware video decode on the 7900 XTX
      };
      profiles = {
        default = {
          id = 0;
          isDefault = true;
        };
        i2p = {
          id = 1;
          settings = {
            "network.proxy.type" = 1;
            "network.proxy.http" = "127.0.0.1";
            "network.proxy.http_port" = 4444;
            "network.proxy.ssl" = "127.0.0.1";
            "network.proxy.ssl_port" = 4444;
            "network.proxy.no_proxies_on" = "";
            "network.proxy.allow_hijacking_localhost" = true;
            "media.peerconnection.enabled" = false; # no WebRTC IP leaks
            "browser.startup.homepage" = "http://127.0.0.1:7070";
          };
        };
      };
    };

    wayland.windowManager.mango.settings.bind = [ "SUPER+SHIFT,b,spawn,librewolf -P i2p --no-remote" ];
  };
}
