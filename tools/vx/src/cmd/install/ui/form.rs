//! Screens are lists of rows; some rows are fields the cursor can land on.
//! This holds the row type, the little builders every screen uses, and the
//! renderer (focus fill, scrolling).

use super::app::Field;
use super::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Width of the label column.
pub const LABEL_W: usize = 18;

pub struct Row {
    /// None: text only, the cursor skips it.
    pub field: Option<Field>,
    pub line: Line<'static>,
    /// Focusable but not usable right now (drawn muted even when focused).
    pub disabled: bool,
}

impl Row {
    pub fn info(line: Line<'static>) -> Row {
        Row { field: None, line, disabled: false }
    }
    pub fn text(t: &Theme, s: impl Into<String>) -> Row {
        Row::info(Line::from(Span::styled(s.into(), t.text())))
    }
    pub fn muted(t: &Theme, s: impl Into<String>) -> Row {
        Row::info(Line::from(Span::styled(s.into(), t.muted())))
    }
    pub fn colored(c: Color, s: impl Into<String>) -> Row {
        Row::info(Line::from(Span::styled(s.into(), Style::new().fg(c))))
    }
    pub fn blank() -> Row {
        Row::info(Line::default())
    }
    pub fn heading(t: &Theme, s: &str) -> Row {
        Row::info(Line::from(Span::styled(s.to_string(), t.bold())))
    }
    pub fn field(f: Field, line: Line<'static>) -> Row {
        Row { field: Some(f), line, disabled: false }
    }
    pub fn disabled(mut self, d: bool) -> Row {
        self.disabled = d;
        self
    }
}

/// A line of text the user types into. `masked`: shown as stars.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub value: String,
    pub masked: bool,
}

impl Input {
    pub fn new(s: &str) -> Input {
        Input { value: s.to_string(), masked: false }
    }
    pub fn secret() -> Input {
        Input { value: String::new(), masked: true }
    }
    pub fn shown(&self) -> String {
        if self.masked { "*".repeat(self.value.chars().count()) } else { self.value.clone() }
    }
    pub fn clear(&mut self) {
        self.value.clear();
    }
}

pub fn pad(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w { truncate(s, w) } else { format!("{s}{}", " ".repeat(w - n)) }
}

pub fn truncate(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        s.to_string()
    } else if w <= 1 {
        s.chars().take(w).collect()
    } else {
        format!("{}~", s.chars().take(w - 1).collect::<String>())
    }
}

pub fn label(t: &Theme, s: &str) -> Span<'static> {
    Span::styled(pad(s, LABEL_W), t.muted())
}

pub fn check(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}

pub fn radio(on: bool) -> &'static str {
    if on { "(*)" } else { "( )" }
}

/// `label   [x] text`
pub fn toggle_row(t: &Theme, f: Field, lbl: &str, on: bool, text: &str) -> Row {
    let mut spans = vec![label(t, lbl), Span::styled(check(on), t.accent())];
    if !text.is_empty() {
        spans.push(Span::styled(format!(" {text}"), t.text()));
    }
    Row::field(f, Line::from(spans))
}

/// `label   < value >  note`
pub fn choice_row(t: &Theme, f: Field, lbl: &str, value: &str, note: &str) -> Row {
    let mut spans = vec![label(t, lbl), Span::styled(format!("< {value} >"), t.text())];
    if !note.is_empty() {
        spans.push(Span::styled(format!("  {note}"), t.muted()));
    }
    Row::field(f, Line::from(spans))
}

/// `label   value_` with a cursor when focused.
pub fn input_row(t: &Theme, f: Field, lbl: &str, input: &Input, focused: bool, note: Option<Span<'static>>) -> Row {
    let mut spans = vec![label(t, lbl)];
    let shown = input.shown();
    if shown.is_empty() && !focused {
        spans.push(Span::styled("-", t.muted()));
    } else {
        spans.push(Span::styled(shown, t.text()));
    }
    if focused {
        spans.push(Span::styled("_", t.text()));
    }
    if let Some(n) = note {
        spans.push(Span::raw("  "));
        spans.push(n);
    }
    Row::field(f, Line::from(spans))
}

pub fn button_row(t: &Theme, f: Field, text: &str) -> Row {
    Row::field(f, Line::from(Span::styled(format!("[ {text} ]"), t.text())))
}

/// Word-wrap plain text to `width` columns.
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let width = width.max(10);
    let mut out = vec![];
    let mut cur = String::new();
    for word in s.split(' ') {
        let need = if cur.is_empty() { word.chars().count() } else { cur.chars().count() + 1 + word.chars().count() };
        if need > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// Draw rows into `area`, the focused one filled with the accent, scrolled so it's visible.
pub fn draw_rows(f: &mut Frame, area: Rect, rows: Vec<Row>, focus: Option<Field>, t: &Theme) {
    let h = area.height as usize;
    if h == 0 {
        return;
    }
    let focus_i = rows.iter().position(|r| r.field.is_some() && r.field == focus);
    // Keep the focused row in view with a line of context below it; the last
    // row (usually [ next ]) pulls the view to the bottom.
    let offset = match focus_i {
        Some(i) if rows.len() > h => (i + 2).saturating_sub(h).min(rows.len() - h),
        _ => 0,
    };
    let more_below = rows.len() > offset + h;
    for (n, row) in rows.into_iter().skip(offset).take(h).enumerate() {
        let r = Rect { x: area.x, y: area.y + n as u16, width: area.width, height: 1 };
        let focused = focus_i == Some(n + offset);
        let mut line = row.line;
        let style = if focused && row.disabled {
            Style::new().bg(t.surface).fg(t.muted)
        } else if focused {
            for s in &mut line.spans {
                if s.style.bg.is_none() {
                    s.style = s.style.fg(t.on_accent);
                }
            }
            t.selected()
        } else if row.disabled {
            for s in &mut line.spans {
                s.style = s.style.fg(t.muted);
            }
            Style::new()
        } else {
            Style::new()
        };
        f.render_widget(Paragraph::new(line).style(style), r);
    }
    if more_below && area.width > 8 {
        let r = Rect { x: area.x + area.width - 6, y: area.y + area.height - 1, width: 6, height: 1 };
        f.render_widget(Paragraph::new(Span::styled(" more ", t.muted())), r);
    }
    if offset > 0 && area.width > 8 {
        let r = Rect { x: area.x + area.width - 6, y: area.y, width: 6, height: 1 };
        f.render_widget(Paragraph::new(Span::styled(" more ", t.muted())), r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_truncates() {
        assert_eq!(wrap("one two three four", 9), vec!["one two", "three", "four"]);
        assert_eq!(truncate("CT1000P310SSD8", 8), "CT1000P~");
        assert_eq!(pad("ab", 4), "ab  ");
        assert_eq!(Input { value: "pw".into(), masked: true }.shown(), "**");
    }
}
