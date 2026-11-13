# Helium: a Chromium browser for the sites that need one (LibreWolf stays the
# default). Packaged here from the official tarball, see _helium.nix.
#
# Policies: Helium reads Chromium's /etc/chromium/policies, which
# programs.chromium writes without installing Chromium itself. Extensions
# listed in programs.chromium.extensions (by Web Store ID) are force-installed,
# and Helium fetches them through its own Web Store proxy.
{
  flake.modules.nixos.desktop.programs.chromium.enable = true;

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      home.packages = [ (pkgs.callPackage ./_helium.nix { }) ];
    };
}
