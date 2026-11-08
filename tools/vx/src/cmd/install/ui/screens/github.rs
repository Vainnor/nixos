//! The github step: log in (a code typed on github.com/login/device), then use the
//! upstream repo (the owner) or your fork (anyone else; forked for you if needed).

use super::super::app::{Act, App, Field, GhPhase, Level};
use super::super::form::{Row, button_row, toggle_row};
use super::{mark, para};
use ratatui::text::{Line, Span};

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let (owner, repo) = app.upstream();
    let mut r = vec![];
    match &app.gh.phase {
        GhPhase::Start => {
            r.push(Row::heading(t, "log in to github"));
            r.push(Row::blank());
            r.extend(para(
                t,
                &format!(
                    "the install commits this machine's settings and pushes them to github: to {owner}/{repo} if it's yours, \
                     otherwise to your own fork. you log in with a code on your phone or another computer; nothing to type here."
                ),
                w,
            ));
            r.push(Row::blank());
            if let Some(e) = &app.gh.error {
                for l in super::super::form::wrap(e, w.saturating_sub(1)) {
                    r.push(Row::colored(t.red, l));
                }
                r.push(Row::blank());
            }
            r.push(button_row(t, Field::GhLogin, if app.gh.error.is_some() { "try again" } else { "log in" }));
            let skip = button_row(t, Field::GhSkip, "skip: install offline").disabled(!app.gh.unreachable);
            r.push(skip);
            if app.gh.unreachable {
                r.extend(para(t, "offline: the settings are committed in ~/nixos only; push them later with vx sync.", w));
            } else {
                r.push(Row::muted(t, "skipping only works when github can't be reached"));
            }
        }
        GhPhase::Code(dc) => {
            r.push(Row::text(t, "on your phone or another computer, open"));
            r.push(Row::info(Line::from(Span::styled(
                format!("  {}", dc.uri.trim_start_matches("https://")),
                t.accent().add_modifier(ratatui::style::Modifier::BOLD),
            ))));
            r.push(Row::text(t, "and enter this code:"));
            r.push(Row::blank());
            for l in big(&dc.user_code, t.truecolor) {
                r.push(Row::info(Line::from(Span::styled(format!("  {l}"), t.accent()))));
            }
            r.push(Row::blank());
            r.push(Row::info(Line::from(vec![Span::styled("  code  ", t.muted()), Span::styled(dc.user_code.clone(), t.bold())])));
            r.push(Row::blank());
            let frames: &[&str] = if t.truecolor { &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] } else { &["|", "/", "-", "\\"] };
            let s = frames[(app.tick / 2) % frames.len()];
            r.push(Row::muted(t, format!("{s} waiting for the code to be entered ...")));
            if app.backend.is_demo() || !app.backend.is_real() {
                r.push(Row::muted(t, format!("{}: it logs in by itself in a moment", app.backend.label())));
            }
            r.push(Row::blank());
            r.push(button_row(t, Field::GhCancel, "cancel"));
        }
        GhPhase::Account(a) => {
            let who = a.name.as_deref().filter(|n| *n != a.login).map(|n| format!(" ({n})")).unwrap_or_default();
            r.push(mark(t, 0, format!("logged in as {}{who}", a.login)));
            let keys = match a.keys.len() {
                0 => "no ssh keys on github: ssh into this machine stays closed until you add one".to_string(),
                1 => "1 ssh key on github".to_string(),
                n => format!("{n} ssh keys on github"),
            };
            if a.owner {
                r.push(mark(t, 0, format!("you own {owner}/{repo}: the install pushes there")));
                r.push(mark(t, if a.keys.is_empty() { 1 } else { 0 }, keys));
                r.push(Row::blank());
                r.push(toggle_row(t, Field::GhPush, "push", app.gh.push, &format!("push the install commit to {owner}/{repo} main")));
                if !app.gh.push {
                    r.push(Row::colored(t.yellow, format!("{:18}it stays in ~/nixos: push it yourself later", "")));
                }
            } else if let Some(f) = &a.fork {
                r.push(mark(t, 0, format!("your fork: {} ({}): cloned from and pushed to", f.full_name, f.branch)));
                r.push(mark(t, if a.keys.is_empty() { 1 } else { 0 }, keys));
                r.push(Row::blank());
                r.extend(para(
                    t,
                    &format!(
                        "the config is made yours: your name, email and github ssh keys go into meta.nix (none of {owner}'s). \
                         secrets, backups and the binary cache start off: they need your own."
                    ),
                    w,
                ));
            } else {
                r.push(Row::blank());
                r.push(Row::text(t, format!("this config belongs to {owner}. to use it, fork it:")));
                r.push(Row::muted(t, format!("a copy at github.com/{}/{repo} that the install pushes to", a.login)));
                r.push(Row::blank());
                if let Some(e) = &app.gh.error {
                    for l in super::super::form::wrap(e, w.saturating_sub(1)) {
                        r.push(Row::colored(t.red, l));
                    }
                }
                r.push(button_row(t, Field::GhFork, "fork it for me"));
                r.push(button_row(t, Field::GhQuit, "quit"));
                return r;
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
        Field::GhLogin => app.gh_login(),
        Field::GhCancel => app.gh_cancel(),
        Field::GhSkip => {
            if !app.gh.unreachable {
                return app.say(Level::Error, "github is reachable: log in (skipping is for offline installs)");
            }
            app.gh.skipped = true;
            app.next();
        }
        Field::GhPush => app.gh.push = !app.gh.push,
        Field::GhFork => app.gh_fork(),
        Field::GhQuit => app.request_quit(),
        _ => {}
    }
}

/// 3x5 glyphs for a GitHub code (A-Z, 0-9, -): `#` is ink.
fn glyph(c: char) -> [&'static str; 5] {
    match c.to_ascii_uppercase() {
        '0' | 'O' => ["###", "# #", "# #", "# #", "###"],
        '1' => [" # ", "## ", " # ", " # ", "###"],
        '2' => ["###", "  #", "###", "#  ", "###"],
        '3' => ["###", "  #", "###", "  #", "###"],
        '4' => ["# #", "# #", "###", "  #", "  #"],
        '5' | 'S' => ["###", "#  ", "###", "  #", "###"],
        '6' => ["###", "#  ", "###", "# #", "###"],
        '7' => ["###", "  #", "  #", "  #", "  #"],
        '8' => ["###", "# #", "###", "# #", "###"],
        '9' => ["###", "# #", "###", "  #", "###"],
        'A' => ["###", "# #", "###", "# #", "# #"],
        'B' => ["## ", "# #", "## ", "# #", "## "],
        'C' => ["###", "#  ", "#  ", "#  ", "###"],
        'D' => ["## ", "# #", "# #", "# #", "## "],
        'E' => ["###", "#  ", "## ", "#  ", "###"],
        'F' => ["###", "#  ", "## ", "#  ", "#  "],
        'G' => ["###", "#  ", "# #", "# #", "###"],
        'H' => ["# #", "# #", "###", "# #", "# #"],
        'I' => ["###", " # ", " # ", " # ", "###"],
        'J' => ["  #", "  #", "  #", "# #", "###"],
        'K' => ["# #", "# #", "## ", "# #", "# #"],
        'L' => ["#  ", "#  ", "#  ", "#  ", "###"],
        'M' => ["# #", "###", "###", "# #", "# #"],
        'N' => ["## ", "# #", "# #", "# #", "# #"],
        'P' => ["###", "# #", "###", "#  ", "#  "],
        'Q' => ["###", "# #", "# #", "###", "  #"],
        'R' => ["## ", "# #", "## ", "# #", "# #"],
        'T' => ["###", " # ", " # ", " # ", " # "],
        'U' => ["# #", "# #", "# #", "# #", "###"],
        'V' => ["# #", "# #", "# #", "# #", " # "],
        'W' => ["# #", "# #", "###", "###", "# #"],
        'X' => ["# #", "# #", " # ", "# #", "# #"],
        'Y' => ["# #", "# #", " # ", " # ", " # "],
        'Z' => ["###", "  #", " # ", "#  ", "###"],
        '-' => ["   ", "   ", "###", "   ", "   "],
        _ => ["   ", "   ", "   ", "   ", "   "],
    }
}

/// The code in big letters (five lines), each glyph doubled in width so it reads at a distance.
pub fn big(code: &str, blocks: bool) -> Vec<String> {
    let ink = if blocks { "██" } else { "##" };
    (0..5)
        .map(|row| {
            code.chars()
                .map(|c| glyph(c)[row].chars().map(|p| if p == '#' { ink } else { "  " }).collect::<String>())
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn big_code() {
        let b = super::big("A1-Z", false);
        assert_eq!(b.len(), 5);
        assert_eq!(b[0], "######    ##            ######");
        assert_eq!(b[2], "######    ##    ######    ##");
        // 9 glyphs of 6 columns plus gaps fit 80 columns.
        assert!(super::big("WDJB-MJHT", true)[0].chars().count() <= 70);
    }
}
