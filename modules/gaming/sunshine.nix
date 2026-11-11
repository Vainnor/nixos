# Sunshine: the PC's remote desktop. It streams the whole desktop (or a game)
# to Moonlight on the Mac, a phone or a TV, with sound, keyboard, mouse and
# controllers, over NetBird only: its ports are open on wt0 alone, never on the
# LAN or Wi-Fi, and the NetBird policy decides which peers get there.
#
# It captures through Mango's screencopy protocol and encodes on the GPU
# (VA-API: H.264, HEVC, AV1), so it needs no extra privileges.
#
# First run, once per client:
#   1. Admin login: on the PC open https://localhost:47990 (or from the Mac,
#      `ssh -L 47990:localhost:47990 arc` and open https://localhost:47990).
#   2. In Moonlight add the PC by its NetBird name (arc.netbird.cloud),
#      then type the PIN it shows into the web UI's PIN page.
{
  flake.modules.nixos.gaming = {
    services.sunshine = {
      enable = true;
      autoStart = true;
      openFirewall = false;
    };
    # Sunshine's ports (base 47989): HTTPS 47984, HTTP 47989, web UI 47990,
    # RTSP 48010; video, control and audio on UDP 47998-48000, plus 48002 and
    # 48010 (Sunshine's documented list).
    networking.firewall.interfaces.wt0 = {
      allowedTCPPorts = [
        47984
        47989
        47990
        48010
      ];
      allowedUDPPortRanges = [
        {
          from = 47998;
          to = 48000;
        }
      ];
      allowedUDPPorts = [
        48002
        48010
      ];
    };
  };
}
