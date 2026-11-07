# Screenshots: grim captures, slurp picks a region, satty annotates
# (arrows, boxes, blur, text, crop), then copies and/or saves.
#   Print              region -> annotate
#   Shift + Print      the focused monitor -> annotate
#   Super + Print      the focused window -> annotate
#   Super + Shift + S  region -> clipboard immediately (no editor)
#   Super + Shift + C  colour picker -> hex to clipboard
{
  flake.modules.homeManager.desktop =
    {
      config,
      pkgs,
      lib,
      ...
    }:
    let
      c = config.lib.stylix.colors;
      dir = "$HOME/Pictures/Screenshots";
      jq = lib.getExe pkgs.jq;
      # slurp in the palette: dimmed background, accent outline, no fill in
      # the selection, other windows shaded with the surface colour.
      slurp = lib.concatStringsSep " " [
        (lib.getExe pkgs.slurp)
        "-b '#${c.base00}88'"
        "-c '#${c.base0D}ff'"
        "-s '#00000000'"
        "-B '#${c.base01}88'"
        "-w 2"
        "-F '${config.stylix.fonts.monospace.name}'"
      ];
      annotate = pkgs.writeShellScript "screenshot-annotate" ''
        set -eu
        mkdir -p "${dir}"
        out=()
        case "''${1:-}" in
          monitor)
            # The monitor Mango has focused (mmsg: Mango's IPC, from the session).
            name="$(mmsg get all-monitors | ${jq} -r '.monitors[] | select(.active) | .name')"
            [ -n "$name" ] || exit 0
            out=(-o "$name")
            ;;
          window)
            # The focused window, inside its border (border_px = 2 in mango.nix).
            geo="$(mmsg get focusing-client | ${jq} -er 'select(.width) | "\(.x + 2),\(.y + 2) \(.width - 4)x\(.height - 4)"')" || exit 0
            out=(-g "$geo")
            ;;
          *)
            geo="$(${slurp} -d)" || exit 0
            out=(-g "$geo")
            ;;
        esac
        ${lib.getExe pkgs.grim} "''${out[@]}" - | ${lib.getExe pkgs.satty} --filename - \
          --output-filename "${dir}/$(date +%F_%H-%M-%S).png"
      '';
      quick = pkgs.writeShellScript "screenshot-quick" ''
        set -eu
        geo="$(${slurp} -d)" || exit 0
        ${lib.getExe pkgs.grim} -g "$geo" - | ${pkgs.wl-clipboard}/bin/wl-copy --type image/png
        ${pkgs.libnotify}/bin/notify-send -t 2000 "screenshot copied"
      '';
      picker = pkgs.writeShellScript "color-picker" ''
        set -eu
        geo="$(${slurp} -p)" || exit 0
        hex="$(${lib.getExe pkgs.grim} -g "$geo" -t ppm - | ${pkgs.imagemagick}/bin/magick - -format '#%[hex:p{0,0}]' info:)"
        printf '%s' "''${hex:0:7}" | ${pkgs.wl-clipboard}/bin/wl-copy
        ${pkgs.libnotify}/bin/notify-send -t 2000 "colour copied" "''${hex:0:7}"
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
          corner-roundness = 0;
        };
      };

      wayland.windowManager.mango.settings = {
        bind = [
          "NONE,Print,spawn,${annotate}"
          "SHIFT,Print,spawn,${annotate} monitor"
          "SUPER,Print,spawn,${annotate} window"
          "SUPER+SHIFT,s,spawn,${quick}"
          "SUPER+SHIFT,c,spawn,${picker}"
        ];
        window_rule = [ "is_floating:1,app_id:com.gabm.satty" ];
      };
    };
}
