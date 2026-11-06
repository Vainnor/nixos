# fuzzel: app launcher, and the picker for everything else:
#   Super+Space   apps                 Super+;   emoji (copied)
#   Super+V       clipboard history    Super+C   calculator (result copied)
#   Super+/       find a file in ~ and open it
{
  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    let
      calc = pkgs.writeShellApplication {
        name = "fuzzel-calc";
        runtimeInputs = with pkgs; [
          fuzzel
          libqalculate
          wl-clipboard
          libnotify
        ];
        text = ''
          # Each answer becomes the next prompt, so you can keep going; Esc to stop.
          prompt="calc ❯ "
          while expr=$(fuzzel --dmenu --prompt-only "$prompt" </dev/null) && [ -n "$expr" ]; do
            res=$(qalc -t -- "$expr")
            printf '%s' "$res" | wl-copy
            prompt="$res ❯ "
          done
          [ -n "''${res:-}" ] && notify-send -a Calculator "= $res" "Copied to clipboard"
          exit 0
        '';
      };
      files = pkgs.writeShellApplication {
        name = "fuzzel-files";
        runtimeInputs = with pkgs; [
          fd
          fuzzel
          xdg-utils
        ];
        text = ''
          cd "$HOME"
          pick=$(fd --type f --hidden --follow \
            --exclude .git --exclude .cache --exclude node_modules --exclude target \
            --exclude .local/share/Steam --exclude .steam --exclude .var \
            | fuzzel --dmenu --prompt "file ❯ " --width 80) || exit 0
          setsid -f xdg-open "$HOME/$pick" >/dev/null 2>&1
        '';
      };
    in
    {
      programs.fuzzel = {
        enable = true;
        settings.main = {
          terminal = "ghostty -e";
          layer = "overlay";
          width = 40;
          lines = 12;
          prompt = "'❯ '";
        };
      };

      home.packages = [
        calc
        files
        pkgs.rofimoji
      ];

      wayland.windowManager.mango.settings.bind = [
        "SUPER,semicolon,spawn,rofimoji --selector fuzzel --clipboarder wl-copy --action copy"
        "SUPER,c,spawn,fuzzel-calc"
        "SUPER,slash,spawn,fuzzel-files"
      ];
    };
}
