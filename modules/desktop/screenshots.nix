# Screenshots: grim captures, slurp picks a region, satty annotates
# (arrows, boxes, blur, text, crop), then copies and/or saves.
#   Print              region -> annotate
#   Shift + Print      whole desktop -> annotate
#   Super + Shift + S  region -> clipboard immediately (no editor)
#   Super + Shift + C  colour picker -> hex to clipboard
{
  flake.modules.homeManager.desktop =
    { pkgs, lib, ... }:
    let
      dir = "$HOME/Pictures/Screenshots";
      annotate = pkgs.writeShellScript "screenshot-annotate" ''
        set -eu
        mkdir -p "${dir}"
        if [ "''${1:-}" = full ]; then geo=""; else geo="$(${lib.getExe pkgs.slurp} -d)" || exit 0; fi
        ${lib.getExe pkgs.grim} ''${geo:+-g "$geo"} - | ${lib.getExe pkgs.satty} --filename - \
          --output-filename "${dir}/$(date +%F_%H-%M-%S).png"
      '';
      quick = pkgs.writeShellScript "screenshot-quick" ''
        set -eu
        geo="$(${lib.getExe pkgs.slurp} -d)" || exit 0
        ${lib.getExe pkgs.grim} -g "$geo" - | ${pkgs.wl-clipboard}/bin/wl-copy --type image/png
        ${pkgs.libnotify}/bin/notify-send -t 2000 "Screenshot copied"
      '';
      picker = pkgs.writeShellScript "color-picker" ''
        set -eu
        geo="$(${lib.getExe pkgs.slurp} -p)" || exit 0
        hex="$(${lib.getExe pkgs.grim} -g "$geo" -t ppm - | ${pkgs.imagemagick}/bin/magick - -format '#%[hex:p{0,0}]' info:)"
        printf '%s' "''${hex:0:7}" | ${pkgs.wl-clipboard}/bin/wl-copy
        ${pkgs.libnotify}/bin/notify-send -t 2000 "Colour copied" "''${hex:0:7}"
      '';
    in
    {
      home.packages = [ pkgs.satty ];

      xdg.configFile."satty/config.toml".source = (pkgs.formats.toml { }).generate "satty.toml" {
        general = {
          early-exit = true; # close after copy/save
          copy-command = "wl-copy";
          initial-tool = "arrow";
          actions-on-enter = [ "save-to-clipboard" ];
          actions-on-escape = [ "exit" ];
          save-after-copy = true;
        };
      };

      wayland.windowManager.mango.settings = {
        bind = [
          "NONE,Print,spawn,${annotate}"
          "SHIFT,Print,spawn,${annotate} full"
          "SUPER+SHIFT,s,spawn,${quick}"
          "SUPER+SHIFT,c,spawn,${picker}"
        ];
        window_rule = [ "is_floating:1,app_id:com.gabm.satty" ];
      };
    };
}
