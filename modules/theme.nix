# ONE place for the look of everything. Change `meta.theme` (flake/meta.nix)
# and every themed app follows: GTK/Qt, Ghostty, fuzzel, waybar, swaync,
# swaylock, the boot splash, LibreWolf, Vesktop, Zed, btop, bat, fish, ... plus Mango and vis,
# which read the palette below by hand because Stylix has no target for them.
#
# Themes: our own in assets/themes (vainnor-slate, caelus), or any base16 name.
# The style is flat and square everywhere: no rounded corners, shadows or blur.
{
  inputs,
  config,

  ...
}:
let
  inherit (config) meta;

  palette = import ../assets/themes meta.theme;

  # A flat background in the theme's colours, with a faint dot grid.
  dots =
    pkgs:
    let
      c = palette.scheme;
    in
    pkgs.runCommand "wallpaper.png" { nativeBuildInputs = [ pkgs.imagemagick ]; } ''
      magick -size 24x24 xc:'#${c.base00}' -fill '#${c.base02}' -draw 'circle 3,3 4.2,3' tile.png
      magick -size 3440x1440 tile:tile.png "$out"
    '';

  stylixFor = pkgs: {
    enable = true;
    # Local themes are plain Nix; base16 names come straight from the source
    # download, so evaluating the Mac's config on Linux (or the reverse) needs no build.
    base16Scheme =
      if palette.scheme != null then
        palette.scheme
      else
        "${pkgs.base16-schemes.src}/base16/${meta.theme}.yaml";
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
        image =
          if builtins.isPath meta.wallpaper then
            meta.wallpaper
          else if meta.wallpaper == "dots" && palette.scheme != null then
            dots pkgs
          else
            pkgs.nixos-artwork.wallpapers.${meta.wallpaper}.gnomeFilePath;
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
      border = if palette.extra.border != null then palette.extra.border else c.base02;
    in
    {
      stylix.targets.rofi.enable = false; # not used; silences an upstream warning
      stylix.targets.gtk.extraCss = "* { border-radius: 0; box-shadow: none; }";
      stylix.targets.librewolf.profileNames = [
        "default"
        "i2p"
      ];

      wayland.windowManager.mango.settings = {
        root_color = argb c.base00;
        border_color = argb border;
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
