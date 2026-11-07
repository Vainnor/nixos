# ONE place for the look of everything. Change `meta.theme` (flake/meta.nix)
# and every themed app follows: GTK/Qt, Ghostty, the shell (desktop/shell.nix),
# the boot splash, LibreWolf, Vesktop, Zed, btop, bat, fish, ... plus Mango and vis,
# which read the palette below by hand because Stylix has no target for them.
#
# Wallpapers live in assets/wallpapers/<name>/ (wide.png for the ultrawide,
# 4k.png for the left monitor and the Mac, theme.nix with a matching palette).
# `vx wallpaper <name>` swaps both, and everything re-themes to match.
# The style is flat and square everywhere: no rounded corners, shadows or blur.
#
# Known gaps: 1Password, Cider, RustDesk and TeamSpeak draw their own theme and
# ignore all of this. Set each one to its dark mode by hand, once, in its settings.
{
  inputs,
  config,

  ...
}:
let
  inherit (config) meta;

  palette = import ../assets/themes meta;

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
          if palette.wallpapers != null then
            palette.wallpapers.wide
          else if builtins.isPath meta.wallpaper then
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
        # Set here so home-manager picks it up too (GTK, and Qt below).
        icons = {
          enable = true;
          package = pkgs.papirus-icon-theme;
          dark = "Papirus-Dark";
          light = "Papirus-Light";
        };
      };
      fonts.packages = with pkgs; [
        noto-fonts-cjk-sans
        noto-fonts-cjk-serif
        nerd-fonts.symbols-only # icon glyphs for apps that don't ask for a Nerd Font by name
      ];
    };

  # Same palette and fonts on the Mac (terminal tools, bat, btop, fzf, vis, ...).
  flake.modules.darwin.base =
    { pkgs, ... }:
    {
      imports = [ inputs.stylix.darwinModules.stylix ];
      stylix = stylixFor pkgs;
      # Keep the Mac's own starship.toml, and skip Linux-only theming.
      home-manager.sharedModules = [
        (
          { lib, ... }:
          {
            stylix.targets = {
              starship.enable = false;
              rofi.enable = false;
              gtk.enable = false;
              xresources.enable = false;
              blender.enable = false;
            };
            # The 4K wallpaper on every Mac screen and Space.
            home.activation.wallpaper = lib.mkIf (palette.wallpapers != null) (
              lib.hm.dag.entryAfter [ "writeBoundary" ] ''
                run ${pkgs.desktoppr}/bin/desktoppr all ${palette.wallpapers.uhd} || true
              ''
            );
          }
        )
      ];
    };

  flake.modules.homeManager.desktop =
    {
      config,
      pkgs,
      lib,
      ...
    }:
    let
      c = config.lib.stylix.colors;
      argb = hex: "0x${hex}ff";
      wallpaper = pkgs.writeShellApplication {
        name = "set-wallpaper";
        runtimeInputs = [
          pkgs.swaybg
          pkgs.wlr-randr
          pkgs.jq
        ];
        text =
          if palette.wallpapers == null then
            "exec swaybg -m fill -i ${config.stylix.image}"
          else
            ''
              # Wait for the outputs, then one image per screen by its shape.
              for _ in $(seq 20); do wlr-randr --json >/dev/null 2>&1 && break; sleep 0.25; done
              args=()
              while read -r name w h; do
                if [ $((w * 10 / h)) -ge 20 ]; then img=${palette.wallpapers.wide}; else img=${palette.wallpapers.uhd}; fi
                args+=(-o "$name" -m fill -i "$img")
              done < <(wlr-randr --json | jq -r '.[] | select(.enabled) | . as $o | ($o.modes[] | select(.current)) as $m | "\($o.name) \($m.width) \($m.height)"')
              pkill -x swaybg || true # one started some other way (an older login)
              if [ ''${#args[@]} -eq 0 ]; then exec swaybg -m fill -i ${palette.wallpapers.wide}; fi
              exec swaybg "''${args[@]}"
            '';
      };
      border = if palette.extra.border != null then palette.extra.border else c.base02;

      # Qt colours for qt5ct/qt6ct, one entry per QPalette role in Qt's order:
      # WindowText Button Light Midlight Dark Mid Text BrightText ButtonText
      # Base Window Shadow Highlight HighlightedText Link LinkVisited
      # AlternateBase NoRole ToolTipBase ToolTipText PlaceholderText Accent
      qtColors =
        let
          row = roles: lib.concatMapStringsSep ", " (r: "#ff${c.${r}}") roles;
          active = row [
            "base05"
            "base01"
            "base03"
            "base02"
            "base00"
            "base02"
            "base05"
            "base07"
            "base05"
            "base01"
            "base00"
            "base00"
            "base0D"
            "base00"
            "base0D"
            "base0E"
            "base00"
            "base00"
            "base01"
            "base05"
            "base04"
            "base0D"
          ];
          disabled = row [
            "base04"
            "base01"
            "base03"
            "base02"
            "base00"
            "base02"
            "base04"
            "base04"
            "base04"
            "base01"
            "base00"
            "base00"
            "base03"
            "base05"
            "base04"
            "base04"
            "base00"
            "base00"
            "base01"
            "base04"
            "base03"
            "base03"
          ];
        in
        pkgs.writeText "base16.conf" ''
          [ColorScheme]
          active_colors=${active}
          inactive_colors=${active}
          disabled_colors=${disabled}
        '';
      qtctSettings = {
        Appearance = {
          style = "Fusion";
          custom_palette = true;
          color_scheme_path = "${qtColors}";
          icon_theme = config.stylix.icons.dark;
          standard_dialogs = "xdgdesktopportal";
        };
        Fonts = with config.stylix.fonts; {
          fixed = ''"${monospace.name},${toString sizes.applications}"'';
          general = ''"${sansSerif.name},${toString sizes.applications}"'';
        };
      };
    in
    {
      stylix.targets.rofi.enable = false; # not used; silences an upstream warning
      stylix.targets.gtk.extraCss = "* { border-radius: 0; box-shadow: none; }";

      # Qt: Fusion with the palette, set through qt5ct/qt6ct, instead of Stylix's
      # Kvantum theme. That theme draws its round corners and menu shadows as
      # curves inside an SVG (not as rx/ry or a setting), so there is nothing
      # small to patch; Fusion has no shadows or blur and follows the palette.
      # Fusion still rounds buttons by about 2px; Qt has no setting for that.
      stylix.targets.qt.enable = false;
      qt = {
        enable = true;
        platformTheme.name = "qtct";
        qt5ctSettings = qtctSettings;
        qt6ctSettings = qtctSettings;
      };
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
        cursor_theme = config.stylix.cursor.name;
        cursor_size = 24;
      };

      # Wallpaper from meta.wallpaper (see flake/meta.nix): with a wallpaper
      # folder, wide screens (the ultrawide) get wide.png and the rest get 4k.png.
      # A user service, so a rebuild with a new wallpaper restarts it right away
      # (an autostart line would only pick it up at the next login).
      systemd.user.services.wallpaper = {
        Unit = {
          Description = "Wallpaper (swaybg)";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" ];
        };
        Service = {
          ExecStart = lib.getExe wallpaper;
          Restart = "on-failure";
          RestartSec = 2;
        };
        Install.WantedBy = [ "graphical-session.target" ];
      };
    };
}
