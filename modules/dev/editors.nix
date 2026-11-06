# Zed (LSPs come from Nix, since Zed's downloaded binaries often can't run on
# NixOS) and vis (config in Lua, colours from the Stylix palette).
{
  flake.modules.homeManager.dev =
    { pkgs, config, ... }:
    let
      c = config.lib.stylix.colors.withHashtag;
    in
    {
      programs.zed-editor = {
        enable = pkgs.stdenv.hostPlatform.isLinux; # the Mac keeps its own Zed.app and settings
        extensions = [
          "nix"
          "toml"
          "dockerfile"
          "docker-compose"
          "java"
          "php"
          "sql"
        ];
        extraPackages = with pkgs; [
          nixd
          nixfmt
          rust-analyzer
        ];
        userSettings = {
          vim_mode = false;
          telemetry = {
            diagnostics = false;
            metrics = false;
          };
          load_direnv = "shell_hook";
          lsp.rust-analyzer.binary.path_lookup = true;
          languages.Nix.language_servers = [
            "nixd"
            "!nil"
          ];
        };
      };

      home.packages = [ pkgs.vis ];
      xdg.configFile."vis/visrc.lua".text = ''
        require('vis')

        vis.events.subscribe(vis.events.INIT, function()
          vis:command('set theme stylix')
        end)

        vis.events.subscribe(vis.events.WIN_OPEN, function(win)
          vis:command('set number')
          vis:command('set autoindent on')
          vis:command('set tabwidth 4')
          vis:command('set expandtab on')
        end)
      '';
      # Minimal theme built from the shared palette.
      xdg.configFile."vis/themes/stylix.lua".text = ''
        local lexers = vis.lexers
        local fg, bg = '${c.base05}', '${c.base00}'
        lexers.STYLE_DEFAULT = 'fore:' .. fg .. ',back:' .. bg
        lexers.STYLE_NOTHING = 'back:' .. bg
        lexers.STYLE_COMMENT = 'fore:${c.base03},italics'
        lexers.STYLE_STRING = 'fore:${c.base0B}'
        lexers.STYLE_NUMBER = 'fore:${c.base09}'
        lexers.STYLE_KEYWORD = 'fore:${c.base0E},bold'
        lexers.STYLE_FUNCTION = 'fore:${c.base0D}'
        lexers.STYLE_TYPE = 'fore:${c.base0A}'
        lexers.STYLE_CONSTANT = 'fore:${c.base09}'
        lexers.STYLE_OPERATOR = 'fore:${c.base0C}'
        lexers.STYLE_LINENUMBER = 'fore:${c.base03},back:' .. bg
        lexers.STYLE_LINENUMBER_CURSOR = 'fore:${c.base05},back:' .. bg
        lexers.STYLE_CURSOR = 'reverse'
        lexers.STYLE_CURSOR_LINE = 'back:${c.base01}'
        lexers.STYLE_SELECTION = 'back:${c.base02}'
        lexers.STYLE_STATUS = 'fore:${c.base04},back:${c.base01}'
        lexers.STYLE_STATUS_FOCUSED = 'fore:${c.base05},back:${c.base02}'
        lexers.STYLE_SEPARATOR = 'fore:${c.base02}'
        lexers.STYLE_INFO = 'fore:${c.base0D}'
      '';
      home.sessionVariables.EDITOR = "vis";
    };
}
