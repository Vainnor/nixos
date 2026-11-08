//! The frame around every screen: step indicator, bordered body, status in
//! the bottom border, help line, and modals.

use super::app::{App, Level, Modal, STEPS, Screen, WinPurpose, short};
use super::screens;
use super::validate::human_bytes;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Wrap};

pub fn draw(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = f.area();
    f.render_widget(Block::new().style(t.base()), area);
    let [head, body, help] = Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)]).areas(area);
    header(f, head, app);

    let mut block = Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(t.fg(t.border))
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(format!(" {} ", app.screen.title()), t.bold())));
    if let Some(task) = &app.task {
        let frames: &[&str] = if t.truecolor { &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] } else { &["|", "/", "-", "\\"] };
        let s = frames[(app.tick / 2) % frames.len()];
        block = block.title_bottom(Line::from(Span::styled(format!(" {s} {} ", task.label), t.accent())));
    } else if let Some(s) = &app.status {
        let c = match s.level {
            Level::Info => t.text,
            Level::Ok => t.green,
            Level::Warn => t.yellow,
            Level::Error => t.red,
        };
        let w = body.width.saturating_sub(4) as usize;
        block = block.title_bottom(Line::from(Span::styled(super::form::truncate(&format!(" {} ", s.text), w), t.fg(c))));
    }
    let inner = block.inner(body);
    f.render_widget(block, body);
    screens::draw_body(f, inner, app);

    f.render_widget(Paragraph::new(Span::styled(help_text(app), t.muted())), Rect { x: help.x + 1, width: help.width.saturating_sub(1), ..help });
    if let Some(m) = &app.modal {
        modal(f, area, app, m);
    }
}

fn header(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let cur = app.screen.step();
    let used: Vec<usize> = app.order().iter().map(|s| s.step()).collect();
    let mut spans = vec![];
    for (i, name) in STEPS.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", t.muted()));
        }
        let skipped = i < STEPS.len() - 1 && !used.contains(&i);
        spans.push(if i == cur {
            Span::styled(format!(" {name} "), t.selected())
        } else if i < cur && !skipped {
            Span::styled(name.to_string(), t.text())
        } else {
            Span::styled(name.to_string(), t.muted())
        });
    }
    let steps = Line::from(spans);
    let sw = steps.width() as u16;
    let mode = app.backend.label();
    let title = if mode.is_empty() { " vx install".to_string() } else { format!(" vx install · {mode}") };
    if area.width >= sw + title.len() as u16 + 4 {
        f.render_widget(Paragraph::new(Span::styled(title, t.accent())), area);
        f.render_widget(Paragraph::new(steps).right_aligned(), Rect { width: area.width - 1, ..area });
    } else if area.width >= sw {
        f.render_widget(Paragraph::new(steps).centered(), area);
    } else {
        f.render_widget(Paragraph::new(Span::styled(format!(" vx install · {}", STEPS[cur]), t.accent())), area);
    }
}

fn help_text(app: &App) -> String {
    let (up, lr) = if app.theme.truecolor { ("↑↓", "←→") } else { ("up/down", "left/right") };
    if app.modal.is_some() {
        return "type · enter confirm · esc cancel".into();
    }
    if app.task.is_some() {
        return "working ... · q quit".into();
    }
    match app.screen {
        Screen::Install if app.installing() => format!("{up} scroll the log · pgup/pgdn · end follows · can't quit until it finishes"),
        Screen::Install => format!("{up} scroll the log · q quit"),
        Screen::Summary | Screen::Failed => format!("{up} move · enter select · q quit"),
        _ if app.focus.is_some_and(|f| f.is_input()) => format!("type · enter next · {up}/tab move · ctrl+u clear · esc back"),
        Screen::Welcome => format!("{up} move · space toggle · enter select · q quit"),
        Screen::GitHub => format!("{up} move · enter select · esc back · q quit"),
        _ => format!("{up} move · space toggle · {lr} change · enter select · esc back · q quit"),
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn modal(f: &mut Frame, area: Rect, app: &App, m: &Modal) {
    let t = &app.theme;
    let (title, color, lines): (&str, _, Vec<Line>) = match m {
        Modal::Quit => {
            let done = app.progress.as_ref().is_some_and(|p| p.outcome.is_some());
            (
                " quit ",
                t.accent,
                vec![
                    Line::from(Span::styled("quit to a shell?", t.bold())),
                    Line::default(),
                    Line::from(Span::styled(
                        if done { "the new system is installed; reboot when ready." } else { "the log stays in /tmp/vx-install/install.log." },
                        t.muted(),
                    )),
                    Line::default(),
                    Line::from(Span::styled("y quit · any other key stays", t.text())),
                ],
            )
        }
        Modal::Windows { disk, purpose, input } => {
            let d = &app.disks[*disk];
            let why = match purpose {
                WinPurpose::System => "install nixos onto it",
                WinPurpose::Format => "format it as a data drive",
            };
            (
                " erase windows? ",
                t.red,
                vec![
                    Line::from(Span::styled(format!("{}  {}  {}", short(&d.path), human_bytes(d.size), d.model), t.bold())),
                    Line::from(Span::styled(format!("it holds {}. to {why}, everything on it is erased.", screens::kind_label(d)), t.fg(t.red))),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("type windows to confirm:  ", t.text()),
                        Span::styled(format!("{}_", input.value), t.accent()),
                    ]),
                    Line::default(),
                    Line::from(Span::styled("enter confirm · esc cancel", t.muted())),
                ],
            )
        }
    };
    let r = centered(area, 64, lines.len() as u16 + 3);
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(t.fg(color))
        .padding(Padding::horizontal(1))
        .style(t.base())
        .title(Line::from(Span::styled(title, t.fg(color))));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(block), r);
}
