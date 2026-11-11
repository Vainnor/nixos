//! The netbird step: log this machine in to NetBird now (a link and a code,
//! opened on your phone), so it's on the mesh from its first boot. With a
//! carried-over login it only shows for the home restore (the backup is only
//! reachable over NetBird): the usb then joins as `<host>-usb`, and the new
//! system keeps its own login.

use super::super::app::{Act, App, Field, NbPhase};
use super::super::form::{Row, button_row, wrap};
use super::{mark, para};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let host = app.host_name();
    let mut r = vec![];
    match &app.nb.phase {
        NbPhase::Start => {
            r.push(Row::heading(t, "log in to netbird"));
            r.push(Row::blank());
            let why = if app.netbird_carried() {
                format!(
                    "the home backup is only reachable over netbird. {host} keeps its carried-over login; this usb joins \
                     as {} just for the restore (remove it from netbird's peers afterwards).",
                    app.nb_login_name()
                )
            } else {
                format!("{host} joins your netbird network now, so it's reachable from its first boot.")
            };
            r.extend(para(t, &format!("{why} you log in with a link on your phone or another computer."), w));
            r.push(Row::blank());
            if let Some(e) = &app.nb.error {
                for l in wrap(e, w.saturating_sub(1)) {
                    r.push(Row::colored(t.red, l));
                }
                r.push(Row::blank());
            }
            r.push(button_row(t, Field::NbLogin, if app.nb.error.is_some() { "try again" } else { "log in" }));
            r.push(button_row(t, Field::NbSkip, "skip: log in later with sudo netbird up"));
        }
        NbPhase::Code(l) => {
            r.push(Row::text(t, "on your phone or another computer, open"));
            // A link has no spaces to wrap at: cut it into pieces that fit.
            let url: Vec<char> = l.url.trim_start_matches("https://").chars().collect();
            for part in url.chunks(w.saturating_sub(3).max(20)) {
                let part: String = part.iter().collect();
                r.push(Row::info(Line::from(Span::styled(format!("  {part}"), t.accent().add_modifier(Modifier::BOLD)))));
            }
            if let Some(code) = &l.code {
                r.push(Row::text(t, if l.url.contains(code.as_str()) { "and check that it shows this code:" } else { "and enter this code:" }));
                r.push(Row::blank());
                for b in super::github::big(code, t.truecolor) {
                    r.push(Row::info(Line::from(Span::styled(format!("  {b}"), t.accent()))));
                }
                r.push(Row::blank());
                r.push(Row::info(Line::from(vec![Span::styled("  code  ", t.muted()), Span::styled(code.clone(), t.bold())])));
            }
            r.push(Row::blank());
            let frames: &[&str] = if t.truecolor { &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] } else { &["|", "/", "-", "\\"] };
            let s = frames[(app.tick / 2) % frames.len()];
            r.push(Row::muted(t, format!("{s} waiting for the login ...")));
            if !app.backend.is_real() {
                r.push(Row::muted(t, format!("{}: it logs in by itself in a moment", app.backend.label())));
            }
            r.push(Row::blank());
            r.push(button_row(t, Field::NbCancel, "cancel"));
        }
        NbPhase::Done(st) => {
            r.push(mark(t, 0, format!("logged in as {}", app.nb.host)));
            r.push(mark(t, 0, format!("address {}", st.show())));
            if app.nb.host != app.nb_login_name() {
                r.push(mark(t, 1, format!("the machine is called {host} now: netbird keeps the name {}", app.nb.host)));
            }
            r.push(Row::blank());
            if app.netbird_carried() {
                r.extend(para(t, "the new system keeps its carried-over login; this usb stays on netbird for the restore.", w));
            } else {
                r.extend(para(t, "the login is copied into the new system; this usb disconnects before the install.", w));
            }
            r.push(Row::blank());
            r.push(button_row(t, Field::Next, "next"));
        }
    }
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    if !matches!(a, Act::Select | Act::Toggle) {
        return;
    }
    match f {
        Field::NbLogin => app.nb_login(),
        Field::NbCancel => app.nb_cancel(),
        Field::NbSkip => {
            app.nb.skipped = true;
            app.nb.error = None;
            app.next();
        }
        _ => {}
    }
}
