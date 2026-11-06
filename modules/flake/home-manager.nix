# Wires home-manager into NixOS and nix-darwin. Every NixOS aspect (base,
# desktop, gaming, ...) automatically pulls in the home-manager module of the
# same name, so a feature file can define both halves side by side.
# The Mac gets the shared shell (base) and dev tools (dev), nothing desktop.
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
  common = {
    useGlobalPkgs = true;
    useUserPackages = true;
    backupFileExtension = "hm-backup";
  };
in
{
  flake.modules.nixos =
    lib.genAttrs [ "desktop" "gaming" "dev" "privacy" "streaming" "vm" ] forward
    // {
      base = {
        imports = [ inputs.home-manager.nixosModules.home-manager ];
        home-manager = common // {
          users.${user}.imports = [ hm.base ];
        };
      };
    };

  flake.modules.darwin.base = {
    imports = [ inputs.home-manager.darwinModules.home-manager ];
    home-manager = common // {
      users.${user}.imports = [
        hm.base
        hm.dev
      ];
    };
  };
}
