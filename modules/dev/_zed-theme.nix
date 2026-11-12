# Zed's "Vainnor" theme, built from the system palette: the same base16 slots
# and border colour the shell (shell/common/Theme.qml), Ghostty and LibreWolf
# use, so a new wallpaper palette re-themes Zed too. Flat: editor, panels and
# active tab share the background; bars and inactive tabs sit one step up;
# edges are the palette's border colour. (Zed has no corner-radius setting.)
#
# c: base16 colours without '#'; border: the palette's extra border colour.
{ c, border }:
let
  h = k: "#${c.${k}}";
  a = k: alpha: "#${c.${k}}${alpha}";
  edge = "#${border}";

  bg = h "base00";
  surface = h "base01";
  raised = h "base02";
  muted = h "base03";
  dim = h "base04";
  text = h "base05";
  accent = h "base0D";

  status = name: color: {
    ${name} = color;
    "${name}.background" = "${color}1f";
    "${name}.border" = "${color}66";
  };
  syn = color: extra: { inherit color; } // extra;
in
{
  "$schema" = "https://zed.dev/schema/themes/v0.2.0.json";
  name = "Vainnor";
  author = "vainnor";
  themes = [
    {
      name = "Vainnor";
      appearance = "dark";
      style = {
        background = bg;
        "surface.background" = surface;
        "elevated_surface.background" = surface;
        "panel.background" = bg;
        "panel.focused_border" = accent;
        "pane.focused_border" = accent;
        "pane_group.border" = edge;

        border = edge;
        "border.variant" = edge;
        "border.focused" = accent;
        "border.selected" = accent;
        "border.transparent" = "#00000000";
        "border.disabled" = raised;

        "element.background" = surface;
        "element.hover" = raised;
        "element.active" = a "base02" "ff";
        "element.selected" = raised;
        "element.disabled" = surface;
        "drop_target.background" = a "base0D" "33";
        "ghost_element.background" = "#00000000";
        "ghost_element.hover" = raised;
        "ghost_element.active" = raised;
        "ghost_element.selected" = raised;
        "ghost_element.disabled" = "#00000000";

        inherit text;
        "text.muted" = dim;
        "text.placeholder" = muted;
        "text.disabled" = muted;
        "text.accent" = accent;
        icon = text;
        "icon.muted" = dim;
        "icon.disabled" = muted;
        "icon.placeholder" = muted;
        "icon.accent" = accent;
        "link_text.hover" = accent;

        "title_bar.background" = surface;
        "title_bar.inactive_background" = bg;
        "status_bar.background" = surface;
        "toolbar.background" = bg;
        "tab_bar.background" = surface;
        "tab.inactive_background" = surface;
        "tab.active_background" = bg;
        "search.match_background" = a "base0A" "40";

        "scrollbar.thumb.background" = a "base03" "4d";
        "scrollbar.thumb.hover_background" = a "base03" "80";
        "scrollbar.thumb.border" = "#00000000";
        "scrollbar.track.background" = "#00000000";
        "scrollbar.track.border" = "#00000000";

        "editor.foreground" = text;
        "editor.background" = bg;
        "editor.gutter.background" = bg;
        "editor.subheader.background" = surface;
        "editor.active_line.background" = a "base01" "ff";
        "editor.highlighted_line.background" = surface;
        "editor.line_number" = muted;
        "editor.active_line_number" = text;
        "editor.invisible" = a "base03" "80";
        "editor.wrap_guide" = a "base02" "ff";
        "editor.active_wrap_guide" = edge;
        "editor.indent_guide" = raised;
        "editor.indent_guide_active" = edge;
        "editor.document_highlight.read_background" = a "base0D" "1f";
        "editor.document_highlight.write_background" = a "base0D" "33";
        "editor.document_highlight.bracket_background" = a "base0D" "33";

        "terminal.background" = bg;
        "terminal.foreground" = text;
        "terminal.bright_foreground" = h "base07";
        "terminal.dim_foreground" = dim;
        "terminal.ansi.black" = surface;
        "terminal.ansi.bright_black" = muted;
        "terminal.ansi.dim_black" = bg;
        "terminal.ansi.red" = h "base08";
        "terminal.ansi.bright_red" = h "base08";
        "terminal.ansi.dim_red" = a "base08" "b3";
        "terminal.ansi.green" = h "base0B";
        "terminal.ansi.bright_green" = h "base0B";
        "terminal.ansi.dim_green" = a "base0B" "b3";
        "terminal.ansi.yellow" = h "base0A";
        "terminal.ansi.bright_yellow" = h "base0A";
        "terminal.ansi.dim_yellow" = a "base0A" "b3";
        "terminal.ansi.blue" = h "base0D";
        "terminal.ansi.bright_blue" = h "base0D";
        "terminal.ansi.dim_blue" = a "base0D" "b3";
        "terminal.ansi.magenta" = h "base0E";
        "terminal.ansi.bright_magenta" = h "base0E";
        "terminal.ansi.dim_magenta" = a "base0E" "b3";
        "terminal.ansi.cyan" = h "base0C";
        "terminal.ansi.bright_cyan" = h "base0C";
        "terminal.ansi.dim_cyan" = a "base0C" "b3";
        "terminal.ansi.white" = text;
        "terminal.ansi.bright_white" = h "base07";
        "terminal.ansi.dim_white" = dim;

        hidden = muted;
        ignored = muted;
        predictive = muted;
        unreachable = muted;
        conflict = h "base09";
        modified = h "base0A";
        renamed = h "base0C";
      }
      // status "created" (h "base0B")
      // status "deleted" (h "base08")
      // status "error" (h "base08")
      // status "warning" (h "base0A")
      // status "info" (h "base0C")
      // status "hint" (h "base03")
      // status "success" (h "base0B")
      // {
        players =
          map
            (k: {
              cursor = h k;
              background = h k;
              selection = a k "3d";
            })
            [
              "base0D"
              "base0C"
              "base0B"
              "base0A"
              "base09"
              "base0E"
              "base08"
              "base0F"
            ];

        syntax = {
          attribute = syn (h "base0A") { };
          boolean = syn (h "base09") { };
          comment = syn muted { font_style = "italic"; };
          "comment.doc" = syn dim { font_style = "italic"; };
          constant = syn (h "base09") { };
          constructor = syn (h "base0D") { };
          embedded = syn text { };
          emphasis = syn accent { font_style = "italic"; };
          "emphasis.strong" = syn accent { font_weight = 700; };
          enum = syn (h "base0A") { };
          function = syn (h "base0D") { };
          hint = syn muted { };
          keyword = syn (h "base0E") { };
          label = syn (h "base0D") { };
          link_text = syn accent { };
          link_uri = syn (h "base0C") { };
          number = syn (h "base09") { };
          operator = syn (h "base0C") { };
          predictive = syn muted { font_style = "italic"; };
          preproc = syn (h "base0F") { };
          primary = syn text { };
          property = syn (h "base0F") { };
          punctuation = syn dim { };
          "punctuation.bracket" = syn dim { };
          "punctuation.delimiter" = syn dim { };
          "punctuation.list_marker" = syn accent { };
          "punctuation.special" = syn (h "base0C") { };
          string = syn (h "base0B") { };
          "string.escape" = syn (h "base0C") { };
          "string.regex" = syn (h "base0C") { };
          "string.special" = syn (h "base0C") { };
          "string.special.symbol" = syn (h "base0A") { };
          tag = syn (h "base0E") { };
          "text.literal" = syn (h "base0B") { };
          title = syn accent { font_weight = 700; };
          type = syn (h "base0A") { };
          variable = syn text { };
          "variable.special" = syn (h "base08") { };
          variant = syn (h "base0A") { };
        };
      };
    }
  ];
}
