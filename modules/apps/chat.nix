# Discord via Vesktop (Wayland-native, screenshare with audio) and TeamSpeak 6.
{
  flake.modules.nixos.desktop.unfree.allowed = [ "teamspeak6-client" ];

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      programs.vesktop = {
        enable = true;
        # Square and flat like the rest of the desktop (colours come from Stylix).
        vencord.settings.useQuickCss = true;
        vencord.extraQuickCss = "* { border-radius: 0 !important; box-shadow: none !important; }";
      };
      home.packages = [ pkgs.teamspeak6-client ];
    };
}
