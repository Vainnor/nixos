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
        remotePlay.openFirewall = true;
        localNetworkGameTransfers.openFirewall = true;
        extraCompatPackages = [ pkgs.proton-ge-bin ];
      };
      programs.gamescope = {
        enable = true;
        capSysNice = true;
      };
      programs.gamemode.enable = true;
    };

  flake.modules.homeManager.gaming = {
    programs.mangohud.enable = true;

    # Let games tear for lowest latency when fullscreen (VRR still applies).
    wayland.windowManager.mango.settings.window_rule = [
      "force_tearing:1,app_id:^steam_app_.*$"
    ];
  };
}
