# wlsunset: warmer colours after sunset at meta.location. Super+Alt+N toggles
# it (colour-accurate work, or when a stream's colours matter).
{ config, ... }:
let
  inherit (config.meta) location;
in
{
  flake.modules.homeManager.desktop = {
    services.wlsunset = {
      enable = true;
      latitude = location.latitude;
      longitude = location.longitude;
      temperature = {
        day = 6500;
        night = 4000;
      };
    };
    wayland.windowManager.mango.settings.bind = [
      "SUPER+ALT,n,spawn,pkill -USR1 wlsunset"
    ];
  };
}
