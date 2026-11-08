//! 8. progress (step list + log), then the summary or the error.

use super::super::app::{Act, App, Exit, Field, LOG_FILE};
use super::super::form::{Row, button_row, truncate, wrap};
use super::super::super::plan::StepState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

pub fn step_line(app: &App, name: &str, state: &Option<StepState>, w: usize) -> Line<'static> {
    let t = &app.theme;
    let (m, c) = match state {
        None => ("[  ]", t.muted),
        Some(StepState::Running) => ("[..]", t.accent),
        Some(StepState::Done) => ("[ok]", t.green),
        Some(StepState::Skipped) => ("[--]", t.muted),
        Some(StepState::Failed(_)) => ("[!!]", t.red),
    };
    let text_c = match state {
        None | Some(StepState::Skipped) => t.muted,
        Some(StepState::Failed(_)) => t.red,
        _ => t.text,
    };
    Line::from(vec![Span::styled(m, t.fg(c)), Span::styled(format!(" {}", truncate(name, w.saturating_sub(5))), t.fg(text_c))])
}

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let Some(p) = &app.progress else { return };
    let wide = area.width >= 100;
    let steps_h = (p.steps.len() as u16 + 1).min(area.height / 2).max(3);
    let [sa, la] = if wide {
        Layout::horizontal([Constraint::Length(40), Constraint::Min(10)]).spacing(1).areas(area)
    } else {
        Layout::vertical([Constraint::Length(steps_h), Constraint::Min(3)]).areas(area)
    };
    // Steps: keep the running one in view.
    let running = p.steps.iter().position(|s| s.1 == Some(StepState::Running)).unwrap_or(0);
    let h = sa.height as usize;
    let off = if p.steps.len() > h { (running + 2).saturating_sub(h).min(p.steps.len() - h) } else { 0 };
    let lines: Vec<Line> = p.steps.iter().skip(off).take(h).map(|(n, s)| step_line(app, n, s, sa.width as usize)).collect();
    f.render_widget(Paragraph::new(lines), sa);

    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(t.fg(t.border))
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title(Line::from(Span::styled(if p.scroll.is_some() { " log (scrolled: end follows) " } else { " log " }, t.muted())));
    let inner = block.inner(la);
    f.render_widget(block, la);
    let lh = inner.height as usize;
    let bottom = p.scroll.unwrap_or(p.log.len().saturating_sub(1)).min(p.log.len().saturating_sub(1));
    let start = (bottom + 1).saturating_sub(lh);
    let lines: Vec<Line> = p
        .log
        .iter()
        .skip(start)
        .take(lh)
        .map(|l| {
            let s = if l.starts_with("$ ") { t.accent() } else if l.starts_with("error") { t.fg(t.red) } else { t.muted() };
            Line::from(Span::styled(truncate(l, inner.width as usize), s))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

pub fn summary_rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let Some(o) = app.progress.as_ref().and_then(|p| p.outcome.as_ref()) else { return vec![] };
    let mut r = vec![Row::colored(t.green, "nixos is installed"), Row::blank()];
    if !o.age_key.is_empty() {
        r.push(Row::info(Line::from(vec![Span::styled("age key   ", t.muted()), Span::styled(o.age_key.clone(), t.accent())])));
    }
    if o.host_key_carried {
        r.push(Row::colored(t.green, "secrets already work: the old ssh host key was carried over"));
    } else if o.secrets_pending {
        r.push(Row::colored(t.yellow, "secrets arrive once another machine re-encrypts them (see below)"));
    }
    if o.sb_enrolled {
        r.push(Row::colored(t.green, "secure boot keys enrolled (yours + microsoft's)"));
    }
    if let Some(n) = &o.netbird {
        r.push(Row::colored(t.green, format!("netbird: {n}")));
    }
    match (&o.pushed, &o.not_pushed) {
        (Some(p), _) => r.push(Row::colored(t.green, format!("pushed to github: {p}"))),
        (None, Some(why)) => {
            for l in wrap(&format!("not pushed: {why}"), w.saturating_sub(1)) {
                r.push(Row::colored(t.yellow, l));
            }
        }
        (None, None) => {}
    }
    r.push(Row::blank());
    r.push(Row::heading(t, "next"));
    for (i, s) in o.next_steps.iter().enumerate() {
        for (n, l) in wrap(s, w.saturating_sub(5)).into_iter().enumerate() {
            r.push(Row::text(t, if n == 0 { format!("{:>2}. {l}", i + 1) } else { format!("    {l}") }));
        }
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Reboot, "reboot"));
    r.push(button_row(t, Field::RebootFirmware, "reboot into bios"));
    r.push(button_row(t, Field::Shell, "shell"));
    r.push(Row::muted(t, format!("log: {LOG_FILE}")));
    r
}

pub fn failed_rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let Some(p) = &app.progress else { return vec![] };
    let mut r = vec![Row::colored(t.red, "the install failed")];
    if let Some((name, _)) = p.steps.iter().find(|s| matches!(s.1, Some(StepState::Failed(_)))) {
        r.push(Row::text(t, format!("at: {name}")));
    }
    for l in wrap(p.error.as_deref().unwrap_or(""), w.saturating_sub(1)) {
        r.push(Row::colored(t.red, l));
    }
    r.push(Row::blank());
    for l in wrap(&format!("nothing after that step ran. the full log is in {LOG_FILE}. the disks may be half set up: fix the problem and run vx install again."), w.saturating_sub(1)) {
        r.push(Row::muted(t, l));
    }
    r.push(Row::blank());
    for l in p.log.iter().rev().take(6).rev() {
        r.push(Row::muted(t, truncate(l, w)));
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Shell, "shell"));
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    if !matches!(a, Act::Select | Act::Toggle) {
        return;
    }
    app.exit = Some(match f {
        Field::Reboot => Exit::Reboot,
        Field::RebootFirmware => Exit::Firmware,
        _ => Exit::Shell,
    });
}
