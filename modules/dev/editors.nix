# Zed and vis. Both take their colours from the system palette (the base16
# slots Stylix has, plus the palette's border colour), so a new theme or
# wallpaper palette re-themes them with everything else. Language servers and
# formatters come from nixpkgs: Zed's own downloads often can't run on NixOS.
#
# Zed: the "Vainnor" theme (./_zed-theme.nix, flat, slate), Catppuccin file
# icons, Stylix's fonts. Settings are merged into ~/.config/zed/settings.json
# at each switch, so changes made in Zed stay unless they set a key set here.
# Rust uses the project's rust-analyzer (rustup in a dev shell, via direnv)
# and falls back to nixpkgs'.
#
# vis: plugins pinned below (vis-lspc for LSP, fzf file picker, commentary,
# surround), a theme from the palette, the same language servers.
#   Space p  open a file (fzf)      gd / K  definition / hover
#   Space f  format (LSP)           Space e / n / N  diagnostics
#   gcc / gc  toggle comment        cs / ds  change / delete surrounding
{ config, ... }:
let
  inherit (config) meta;
  palette = import ../../assets/themes meta;
in
{
  flake.modules.homeManager.dev =
    {
      pkgs,
      config,
      lib,
      ...
    }:
    let
      c = config.lib.stylix.colors;
      ch = config.lib.stylix.colors.withHashtag;
      border = if palette.extra.border != null then palette.extra.border else c.base02;
      isLinux = pkgs.stdenv.hostPlatform.isLinux;
      bin = pkg: name: "${lib.getBin pkg}/bin/${name}";

      # Language servers and formatters, shared by both editors.
      ls = with pkgs; {
        nixd = bin nixd "nixd";
        nixfmt = bin nixfmt "nixfmt";
        rust-analyzer = bin rust-analyzer "rust-analyzer";
        vtsls = bin vtsls "vtsls";
        eslint = bin vscode-langservers-extracted "vscode-eslint-language-server";
        json = bin vscode-langservers-extracted "vscode-json-language-server";
        css = bin vscode-langservers-extracted "vscode-css-language-server";
        html = bin vscode-langservers-extracted "vscode-html-language-server";
        tailwind = bin tailwindcss-language-server "tailwindcss-language-server";
        yaml = bin yaml-language-server "yaml-language-server";
        taplo = bin taplo "taplo";
        bash = bin bash-language-server "bash-language-server";
        shfmt = bin shfmt "shfmt";
        basedpyright = bin basedpyright "basedpyright-langserver";
        ruff = bin ruff "ruff";
        marksman = bin marksman "marksman";
        lua = bin lua-language-server "lua-language-server";
      };
      stdio = path: {
        inherit path;
        arguments = [ "--stdio" ];
      };

      # vis plugins, pinned.
      visPlugins = {
        vis-lspc = pkgs.fetchFromGitea {
          domain = "codeberg.org";
          owner = "muhq";
          repo = "vis-lspc";
          rev = "66f39ab0909e5f5facc21f592afb9ff1a6420ca6";
          hash = "sha256-sXEde9xarjQkozxewwn2WHvs0rbnzz+y5JdLW2n/CB0=";
        };
        # Plus comment strings for the languages it lacks.
        vis-commentary = pkgs.runCommand "vis-commentary" { } ''
          cp -r ${
            pkgs.fetchFromGitHub {
              owner = "lutobler";
              repo = "vis-commentary";
              rev = "0e06ed8212c12c6651cb078b822390094b396f08";
              hash = "sha256-ZsbCC4O/UOpfYm2mh4VqJXIUDGwfvFMJXl9sgENWNok=";
            }
          } $out
          chmod -R u+w $out
          substituteInPlace $out/init.lua \
            --replace-fail "caml='(*|*)'" "caml='(*|*)', nix='#', typescript='//', qml='//', jsx='//', tsx='//'"
        '';
        vis-surround = pkgs.fetchFromGitHub {
          owner = "lutobler";
          repo = "vis-surround";
          rev = "8455fbe149f37ff429f5189bcae174e0abd09c91";
          hash = "sha256-LFjI2dej0dAbJsfvkk5siHrXkRTzRNnhf/kjYd7As5Q=";
        };
        vis-fzf-open = pkgs.fetchFromGitHub {
          owner = "guillaumecherel";
          repo = "vis-fzf-open";
          rev = "d190b233e4a89e92837e065594e70988dc758444";
          hash = "sha256-MRsdjI5zjhZhwZO1rppKpIxRjSujG3ZVg2fSg612rPY=";
        };
      };
    in
    {
      programs.zed-editor = {
        enable = isLinux; # the Mac keeps its own Zed.app and settings
        extensions = [
          "nix"
          "toml"
          "dockerfile"
          "docker-compose"
          "html"
          "java"
          "php"
          "sql"
          "just"
          "marksman"
          "qml"
          "catppuccin-icons"
        ];
        # On Zed's PATH after the project's own (direnv) tools.
        extraPackages = with pkgs; [
          nixd
          nixfmt
          rust-analyzer
          shellcheck
          shfmt
          ruff
          marksman
          nodejs
          # The QML extension only looks for qmlls on PATH.
          (writeShellScriptBin "qmlls" ''
            exec ${kdePackages.qtdeclarative}/bin/qmlls "$@"
          '')
        ];
        themes.vainnor = import ./_zed-theme.nix { inherit c border; };
        userSettings = {
          theme = {
            mode = "dark";
            dark = "Vainnor";
            light = "Vainnor";
          };
          icon_theme = {
            mode = "dark";
            dark = "Catppuccin Mocha";
            light = "Catppuccin Latte";
          };
          vim_mode = false;
          auto_update = false; # Nix updates Zed
          telemetry = {
            diagnostics = false;
            metrics = false;
          };
          load_direnv = "shell_hook";

          buffer_line_height = "comfortable";
          cursor_blink = false;
          current_line_highlight = "line";
          indent_guides = {
            enabled = true;
            coloring = "fixed";
          };
          inlay_hints = {
            enabled = true;
            show_background = false;
          };
          diagnostics.inline.enabled = true;
          format_on_save = "on";
          tabs = {
            file_icons = true;
            git_status = true;
          };
          tab_bar.show_nav_history_buttons = false;
          toolbar.quick_actions = false;
          project_panel = {
            dock = "left";
            auto_fold_dirs = true;
          };
          terminal = {
            font_family = config.stylix.fonts.monospace.name;
            blinking = "off";
          };

          # Zed's own Node (for prettier and its npm-based servers) is nixpkgs'.
          node = {
            path = bin pkgs.nodejs "node";
            npm_path = bin pkgs.nodejs "npm";
          };

          lsp = {
            rust-analyzer.binary.path_lookup = true;
            nixd.binary.path = ls.nixd;
            vtsls.binary = stdio ls.vtsls;
            eslint.binary = stdio ls.eslint;
            json-language-server.binary = stdio ls.json;
            vscode-css-language-server.binary = stdio ls.css;
            vscode-html-language-server.binary = stdio ls.html;
            tailwindcss-language-server.binary = stdio ls.tailwind;
            yaml-language-server.binary = stdio ls.yaml;
            taplo.binary = {
              path = ls.taplo;
              arguments = [
                "lsp"
                "stdio"
              ];
            };
            bash-language-server.binary = {
              path = ls.bash;
              arguments = [ "start" ];
            };
            basedpyright.binary = stdio ls.basedpyright;
            ruff.binary = {
              path = ls.ruff;
              arguments = [ "server" ];
            };
            marksman.binary = {
              path = ls.marksman;
              arguments = [ "server" ];
            };
          };

          languages = {
            Nix = {
              language_servers = [
                "nixd"
                "!nil"
              ];
              formatter.external.command = ls.nixfmt;
            };
            Python = {
              language_servers = [
                "basedpyright"
                "ruff"
                "!pyright"
                "!pylsp"
              ];
              formatter = [ { language_server.name = "ruff"; } ];
            };
            "Shell Script".formatter.external = {
              command = ls.shfmt;
              arguments = [
                "--filename"
                "{buffer_path}"
              ];
            };
          };
        };
      };
      # Zed gets the theme above; Stylix still sets its fonts.
      stylix.targets.zed.colors.enable = false;

      home.packages = [
        pkgs.vis
        pkgs.shellcheck # bash-language-server runs it
      ];
      xdg.configFile =
        lib.mapAttrs' (name: src: lib.nameValuePair "vis/plugins/${name}" { source = src; }) visPlugins
        // {
          "vis/visrc.lua".text = ''
            require('vis')

            require('plugins/vis-commentary')
            require('plugins/vis-surround')
            local fzf = require('plugins/vis-fzf-open/fzf-open')
            fzf.fzf_path = '${bin pkgs.fzf "fzf"}'

            -- Language servers from nixpkgs. Rust: the project's rust-analyzer
            -- (rustup in a dev shell) when there is one, else nixpkgs'.
            local lspc = require('plugins/vis-lspc')
            local function on_path(b)
              local ok = os.execute('command -v ' .. b .. ' >/dev/null 2>&1')
              return ok == true or ok == 0
            end
            local web = {'package.json', 'tsconfig.json', 'jsconfig.json'}
            local m = lspc.ls_map
            m.nix = {name = 'nixd', cmd = '${ls.nixd}', roots = {'flake.nix'}}
            m.rust = {
              name = 'rust',
              cmd = on_path('rust-analyzer') and 'rust-analyzer' or '${ls.rust-analyzer}',
              roots = {'Cargo.toml'},
            }
            m.javascript = {name = 'vtsls', cmd = '${ls.vtsls} --stdio', roots = web}
            m.typescript = m.javascript
            m.json = {name = 'json-language-server', cmd = '${ls.json} --stdio'}
            m.css = {name = 'css-language-server', cmd = '${ls.css} --stdio'}
            m.html = {name = 'html-language-server', cmd = '${ls.html} --stdio'}
            m.yaml = {name = 'yaml-language-server', cmd = '${ls.yaml} --stdio'}
            m.toml = {name = 'taplo', cmd = '${ls.taplo} lsp stdio'}
            m.bash = {name = 'bash-language-server', cmd = '${ls.bash} start'}
            m.python = {
              name = 'basedpyright',
              cmd = '${ls.basedpyright} --stdio',
              roots = {'pyproject.toml', 'setup.py', 'requirements.txt'},
            }
            m.markdown = {name = 'marksman', cmd = '${ls.marksman} server'}
            m.lua = {
              name = 'lua-language-server',
              cmd = '${ls.lua}',
              settings = {Lua = {diagnostics = {globals = {'vis'}}, telemetry = {enable = false}}},
            }
            lspc.universal_root_globs = {'.git'}
            lspc.fallback_dirname_as_root = true
            lspc.highlight_diagnostics = 'range'
            lspc.diagnostic_styles = {
              error = 'fore:${ch.base08},underlined',
              warning = 'fore:${ch.base0A},underlined',
              information = 'fore:${ch.base0C},underlined',
              hint = 'fore:${ch.base03},underlined',
            }

            vis.events.subscribe(vis.events.INIT, function()
              vis:command('set theme palette')
              vis:map(vis.modes.NORMAL, ' p', ':fzf<Enter>', 'Open a file (fzf)')
              vis:map(vis.modes.NORMAL, ' f', ':lspc-format<Enter>', 'Format the file (LSP)')
            end)

            vis.events.subscribe(vis.events.WIN_OPEN, function(win)
              vis:command('set numbers on')
              vis:command('set cursorline on')
              vis:command('set autoindent on')
              vis:command('set tabwidth 4')
              vis:command('set expandtab on')
            end)
          '';
          # Every style vis's lexers use, from the palette (same roles as Zed).
          "vis/themes/palette.lua".text = ''
            local lexers = vis.lexers
            local bg, fg = '${ch.base00}', '${ch.base05}'
            lexers.STYLE_DEFAULT = 'fore:' .. fg .. ',back:' .. bg
            lexers.STYLE_NOTHING = 'back:' .. bg
            lexers.STYLE_IDENTIFIER = 'fore:' .. fg
            lexers.STYLE_WHITESPACE = ""
            lexers.STYLE_COMMENT = 'fore:${ch.base03},italics'
            lexers.STYLE_STRING = 'fore:${ch.base0B}'
            lexers.STYLE_REGEX = 'fore:${ch.base0C}'
            lexers.STYLE_NUMBER = 'fore:${ch.base09}'
            lexers.STYLE_CONSTANT = 'fore:${ch.base09}'
            lexers.STYLE_KEYWORD = 'fore:${ch.base0E}'
            lexers.STYLE_FUNCTION = 'fore:${ch.base0D}'
            lexers.STYLE_DEFINITION = 'fore:${ch.base0D}'
            lexers.STYLE_CLASS = 'fore:${ch.base0A}'
            lexers.STYLE_TYPE = 'fore:${ch.base0A}'
            lexers.STYLE_LABEL = 'fore:${ch.base0D}'
            lexers.STYLE_ATTRIBUTE = 'fore:${ch.base0A}'
            lexers.STYLE_PROPERTY = 'fore:${ch.base0F}'
            lexers.STYLE_VARIABLE = 'fore:' .. fg
            lexers.STYLE_OPERATOR = 'fore:${ch.base0C}'
            lexers.STYLE_PREPROCESSOR = 'fore:${ch.base0F}'
            lexers.STYLE_TAG = 'fore:${ch.base0E}'
            lexers.STYLE_EMBEDDED = 'fore:${ch.base0F}'
            lexers.STYLE_ERROR = 'fore:${ch.base08}'
            lexers.STYLE_HEADING = 'fore:${ch.base0D},bold'
            for i = 1, 6 do lexers['STYLE_HEADING_H' .. i] = lexers.STYLE_HEADING end
            lexers.STYLE_BOLD = 'bold'
            lexers.STYLE_ITALIC = 'italics'
            lexers.STYLE_LIST = 'fore:${ch.base0D}'
            lexers.STYLE_LINK = 'fore:${ch.base0C},underlined'
            lexers.STYLE_REFERENCE = 'fore:${ch.base0C}'
            lexers.STYLE_CODE = 'fore:${ch.base0B}'
            lexers.STYLE_ADDITION = 'fore:${ch.base0B}'
            lexers.STYLE_DELETION = 'fore:${ch.base08}'
            lexers.STYLE_CHANGE = 'fore:${ch.base0A}'

            lexers.STYLE_LINENUMBER = 'fore:${ch.base03},back:' .. bg
            lexers.STYLE_LINENUMBER_CURSOR = 'fore:' .. fg .. ',back:${ch.base01}'
            lexers.STYLE_CURSOR = 'back:${ch.base04},fore:' .. bg
            lexers.STYLE_CURSOR_PRIMARY = 'back:${ch.base0D},fore:' .. bg
            lexers.STYLE_CURSOR_LINE = 'back:${ch.base01}'
            lexers.STYLE_COLOR_COLUMN = 'back:${ch.base01}'
            lexers.STYLE_SELECTION = 'back:${ch.base02}'
            lexers.STYLE_STATUS = 'fore:${ch.base04},back:${ch.base01}'
            lexers.STYLE_STATUS_FOCUSED = 'fore:' .. fg .. ',back:${ch.base02}'
            lexers.STYLE_SEPARATOR = 'fore:#${border},back:' .. bg
            lexers.STYLE_INFO = 'fore:${ch.base0D}'
            lexers.STYLE_EOF = 'fore:${ch.base02}'
          '';
        };
      home.sessionVariables.EDITOR = "vis";
    };
}
