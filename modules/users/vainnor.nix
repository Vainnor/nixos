# The user account (NixOS half) and the base home-manager config (shell, CLI).
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base =
    { pkgs, ... }:
    {
      programs.fish.enable = true;

      users.users.${meta.username} = {
        isNormalUser = true;
        description = meta.fullName;
        shell = pkgs.fish;
        extraGroups = [
          "wheel"
          "networkmanager"
          "video"
          "audio"
        ];
        # Password hash lives in /persistent/passwords (see system/impermanence.nix).
      };
    };

  flake.modules.homeManager.base =
    { pkgs, ... }:
    {
      home.username = meta.username;
      home.homeDirectory = "/home/${meta.username}";

      programs = {
        home-manager.enable = true;
        fish = {
          enable = true;
          interactiveShellInit = "set -g fish_greeting";
        };
        starship.enable = true;
        zoxide.enable = true;
        eza.enable = true;
        bat.enable = true;
        fzf.enable = true;
        ripgrep.enable = true;
        fd.enable = true;
        btop.enable = true;
        yazi.enable = true;
      };

      home.packages = with pkgs; [
        unzip
        p7zip
        wget
        jq
        fastfetch
      ];
    };
}
