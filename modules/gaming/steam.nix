# Steam + Proton-GE, gamescope, gamemode, MangoHud.
# Per-game launch options, e.g.:  gamemoderun mangohud %command%
#                                 gamescope -W 3440 -H 1440 -r 180 --adaptive-sync -- %command%
{
  flake.modules.nixos.gaming =
    { pkgs, ... }:
    {
      unfree.allowed = [
        "steam"
        "steam-unwrapped"
      ];

      programs.steam = {
        enable = true;
        # Remote Play and LAN game transfers: their ports are opened below on
        # NetBird only, not on the LAN or Wi-Fi (shared networks). Over NetBird
        # Remote Play has no LAN discovery, so Steam connects through your
        # account; Moonlight (gaming/sunshine.nix) is the main remote screen.
        remotePlay.openFirewall = false;
        localNetworkGameTransfers.openFirewall = false;
        extraCompatPackages = [ pkgs.proton-ge-bin ];
      };
      programs.gamescope = {
        enable = true;
        capSysNice = true;
      };
      programs.gamemode.enable = true;

      networking.firewall.interfaces.wt0 = {
        allowedTCPPorts = [
          27036 # Remote Play
          27040 # LAN game transfers
        ];
        allowedUDPPortRanges = [
          {
            from = 27031;
            to = 27036;
          }
        ];
      };
    };

  flake.modules.homeManager.gaming = {
    programs.mangohud.enable = true;

    # Let games tear for lowest latency when fullscreen (VRR still applies).
    wayland.windowManager.mango.settings.window_rule = [
      "force_tearing:1,app_id:^steam_app_.*$"
    ];
  };
}
