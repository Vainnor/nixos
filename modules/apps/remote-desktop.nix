# RustDesk: remote desktop client, for reaching other machines from here.
# Being controlled FROM another machine likely won't work on Mango: RustDesk
# needs the RemoteDesktop portal, which wlroots compositors don't provide.
# For that direction use Sunshine + Moonlight (gaming/sunshine.nix) or SSH
# over NetBird.
{
  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      home.packages = [ pkgs.rustdesk-flutter ];
    };
}
