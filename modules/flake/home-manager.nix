# Wires home-manager into NixOS. Every NixOS aspect (base, desktop, gaming, ...)
# automatically pulls in the home-manager module of the same name, so a feature
# file can define both halves side by side.
{
  inputs,
  config,
  lib,
  ...
}:
let
  user = config.meta.username;
  hm = config.flake.modules.homeManager;
  forward = aspect: {
    home-manager.users.${user}.imports = lib.optional (hm ? ${aspect}) hm.${aspect};
  };
in
{
  flake.modules.nixos = lib.genAttrs [ "desktop" "gaming" "dev" "privacy" "streaming" ] forward // {
    base = {
      imports = [ inputs.home-manager.nixosModules.home-manager ];
      home-manager = {
        useGlobalPkgs = true;
        useUserPackages = true;
        backupFileExtension = "hm-backup";
        users.${user}.imports = [ hm.base ];
      };
    };
  };
}
