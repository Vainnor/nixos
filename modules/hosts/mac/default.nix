# The MacBook (nix-darwin). It shares the shell and dev tools with the PC
# (fish, starship, git with 1Password signing, direnv, vis, the Stylix theme)
# and leaves everything else on the Mac alone: apps, Homebrew, ~/.ssh/config.
#
# First time (once):  sudo nix run nix-darwin -- switch --flake ~/Programing/nixos
# After that:         vx switch
{ inputs, config, ... }:
let
  inherit (config) meta;
in
{
  flake.darwinConfigurations.${meta.macHost} = inputs.nix-darwin.lib.darwinSystem {
    modules = with config.flake.modules.darwin; [
      base
      mac
    ];
  };

  flake.modules.darwin.mac =
    { pkgs, ... }:
    {
      nixpkgs.hostPlatform = "aarch64-darwin";
      networking.hostName = meta.macHost;
      system.primaryUser = meta.username;
      system.stateVersion = 6;
      home-manager.users.${meta.username}.home.stateVersion = "26.11";

      users.users.${meta.username}.home = "/Users/${meta.username}";

      nix.settings = {
        experimental-features = [
          "nix-command"
          "flakes"
        ];
        trusted-users = [ "@admin" ];
      };
      nix.optimise.automatic = true;
      nix.gc = {
        automatic = true;
        options = "--delete-older-than 14d";
      };

      # Nix's fish, so /etc/fish sets up the Nix paths (your Homebrew fish keeps working too).
      programs.fish.enable = true;

      # sudo with Touch ID.
      security.pam.services.sudo_local.touchIdAuth = true;

      environment.systemPackages = with pkgs; [
        nh
        sops
        age
        ssh-to-age
      ];
    };
}
