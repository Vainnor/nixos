# ONE place for the look of everything. Change `meta.theme` (flake/meta.nix)
# and every themed app follows: GTK/Qt, Ghostty, fuzzel, waybar, mako,
# swaylock, LibreWolf, Vesktop, Zed, btop, bat, fish, ... plus Mango and vis,
# which read the palette below by hand because Stylix has no target for them.
{ inputs, config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.desktop =
    { pkgs, ... }:
    {
      imports = [ inputs.stylix.nixosModules.stylix ];

      stylix = {
        enable = true;
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

        cursor = {
          package = pkgs.bibata-cursors;
          name = "Bibata-Modern-Classic";
          size = 24;
        };
      };

      fonts.packages = [ pkgs.noto-fonts-cjk-sans ];
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

      # Solid-colour wallpaper from the palette. Swap for `swaybg -i <image>`.
      wayland.windowManager.mango.autostart_sh = ''
        ${pkgs.swaybg}/bin/swaybg -c '#${c.base00}' &
      '';
    };
}
