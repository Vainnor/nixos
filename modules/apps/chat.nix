# Discord via Vesktop (Wayland-native, screenshare with audio) and TeamSpeak 6.
{
  flake.modules.nixos.desktop.unfree.allowed = [ "teamspeak6-client" ];

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      programs.vesktop.enable = true;
      home.packages = [ pkgs.teamspeak6-client ];
    };
}
