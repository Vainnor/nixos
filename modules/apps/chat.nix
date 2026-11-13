# Discord via Vesktop (Wayland-native, screenshare with audio) and TeamSpeak 3
# (packaged here: nixpkgs dropped it, see _teamspeak3.nix).
#
# Your TeamSpeak identity comes from sops (teamspeak/identity). Export it from
# TeamSpeak (Tools > Identities > Export) and, on the Mac, run
#   vx secrets set teamspeak/identity
# pasting the identity="..." value from that .ini (or put the whole .ini there
# as a block with `vx secrets edit`, which also brings over its name/nickname).
# Each launch adds it as the default identity if it isn't there yet. Until
# that key is in secrets.yaml (key names are plain text there, so this can
# tell) nothing asks for it, and TeamSpeak just makes its own identity.
{ config, lib, ... }:
let
  inherit (config) meta;
  identityFile = "/run/secrets/teamspeak/identity";
  hasIdentity =
    meta.secretsReady
    && lib.hasInfix "\nteamspeak:\n    identity:" ("\n" + builtins.readFile ../../secrets/secrets.yaml);
in
{
  flake.modules.nixos.desktop = {
    unfree.allowed = [ "ts3client" ];
    sops.secrets = lib.mkIf hasIdentity {
      "teamspeak/identity".owner = meta.username;
    };
  };

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      programs.vesktop = {
        enable = true;
        # Square and flat like the rest of the desktop (colours come from Stylix).
        vencord.settings.useQuickCss = true;
        vencord.extraQuickCss = "* { border-radius: 0 !important; box-shadow: none !important; }";
      };
      home.packages = [
        (pkgs.callPackage ./_teamspeak3.nix {
          identityFile = if hasIdentity then identityFile else null;
          nickname = meta.username;
        })
      ];
    };
}
