# ONE place for the look of everything. Change `meta.theme` (flake/meta.nix)
# and every themed app follows: GTK/Qt, Ghostty, fuzzel, waybar, swaync,
# swaylock, the boot splash, LibreWolf, Vesktop, Zed, btop, bat, fish, ... plus Mango and vis,
# which read the palette below by hand because Stylix has no target for them.
{ inputs, config, ... }:
let
  inherit (config) meta;

  stylixFor = pkgs: {
    enable = true;
    image =
      if builtins.isPath meta.wallpaper then
        meta.wallpaper
      else
        pkgs.nixos-artwork.wallpapers.${meta.wallpaper}.gnomeFilePath;
    base16Scheme = "${pkgs.base16-schemes}/share/themes/${meta.theme}.yaml";
    polarity = "dark";

    fonts = {
      monospace = {
        package = pkgs.nerd-fonts.jetbrains-mono;
        name = "JetBrainsMono Nerd Font";
      };
      sansSerif = {
        package = pkgs.inter;
        name = "Inter";
      };
      serif = {
        package = pkgs.noto-fonts;
        name = "Noto Serif";
      };
      emoji = {
        package = pkgs.noto-fonts-color-emoji;
        name = "Noto Color Emoji";
      };
      sizes = {
        terminal = 12;
        applications = 11;
        desktop = 11;
        popups = 11;
      };
    };
  };
in
{
  flake.modules.nixos.desktop =
    { pkgs, ... }:
    {
      imports = [ inputs.stylix.nixosModules.stylix ];
      stylix = stylixFor pkgs // {
        cursor = {
          package = pkgs.bibata-cursors;
          name = "Bibata-Modern-Classic";
          size = 24;
        };
      };
      fonts.packages = [ pkgs.noto-fonts-cjk-sans ];
    };

  # Same palette and fonts on the Mac (terminal tools, bat, btop, fzf, vis, ...).
  flake.modules.darwin.base =
    { pkgs, ... }:
    {
      imports = [ inputs.stylix.darwinModules.stylix ];
      stylix = stylixFor pkgs;
      # Keep the Mac's own starship.toml, and skip Linux-only theming.
      home-manager.sharedModules = [
        {
          stylix.targets = {
            starship.enable = false;
            rofi.enable = false;
            gtk.enable = false;
            xresources.enable = false;
            blender.enable = false;
          };
        }
      ];
    };

  flake.modules.homeManager.desktop =
    { config, pkgs, ... }:
    let
      c = config.lib.stylix.colors;
      argb = hex: "0x${hex}ff";
    in
    {
      stylix.targets.rofi.enable = false; # not used; silences an upstream warning
      stylix.targets.librewolf.profileNames = [
        "default"
        "i2p"
      ];

      wayland.windowManager.mango.settings = {
        root_color = argb c.base00;
        border_color = argb c.base02;
        focus_color = argb c.base0D;
        urgent_color = argb c.base08;
        maximized_screen_color = argb c.base0B;
        scratchpad_color = argb c.base0E;
        global_color = argb c.base0C;
        overlay_color = argb c.base0A;
        cursor_size = 24;
      };

      # Wallpaper from meta.wallpaper (see flake/meta.nix).
      wayland.windowManager.mango.autostart_sh = ''
        ${pkgs.swaybg}/bin/swaybg -m fill -i ${config.stylix.image} &
      '';
    };
}
