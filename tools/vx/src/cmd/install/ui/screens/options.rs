//! 5. options and 6. passwords.

use super::super::app::{Act, App, Field, SWAP_CHOICES};
use super::super::form::{Row, button_row, choice_row, input_row, label, radio, toggle_row};
use super::super::validate;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::path::Path;

const SWATCH_KEYS: [&str; 16] = [
    "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08", "base09", "base0A", "base0B", "base0C",
    "base0D", "base0E", "base0F",
];

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let keep = app.keeping();
    let m = app.machine.clone().unwrap_or_default();
    let mut r = vec![Row::heading(t, "disk")];
    let kept = |lbl: &str, v: String| Row::info(Line::from(vec![label(t, lbl), Span::styled(v, t.text()), Span::styled("  (kept)", t.muted())]));
    if keep {
        r.push(kept("encryption", if app.eff_encrypt() { "on".into() } else { "off".into() }));
    } else {
        r.push(toggle_row(t, Field::Encrypt, "encryption", app.encrypt, "luks: asks for the password at boot"));
    }
    if app.eff_encrypt() {
        let note = if m.tpm { "unlock without a password once secure boot is on" } else { "no tpm found on this machine" };
        r.push(toggle_row(t, Field::Tpm, "tpm unlock", app.tpm && m.tpm, note).disabled(!m.tpm));
    }
    let hint = if m.ram_gb > 0 { format!("ram: {} gib; zram is always on", m.ram_gb) } else { "zram is always on".into() };
    if keep {
        r.push(kept("swap file", format!("{} gib", app.eff_swap())));
    } else {
        r.push(choice_row(t, Field::Swap, "swap file", &if app.swap_gb == 0 { "none".into() } else { format!("{} gib", app.swap_gb) }, &hint));
    }
    r.push(Row::blank());
    r.push(Row::heading(t, "system"));
    let err = |res: Result<(), String>| res.err().map(|e| Span::styled(e, t.fg(t.red)));
    let focus = app.focus;
    let typed = |f: Field, v: &str| focus != Some(f) && !v.is_empty();
    r.push(input_row(
        t,
        Field::Hostname,
        "hostname",
        &app.hostname,
        focus == Some(Field::Hostname),
        err(validate::hostname(&app.hostname.value)).filter(|_| typed(Field::Hostname, &app.hostname.value)),
    ));
    let default_user = app.data.as_ref().map(|d| d.defaults.username.as_str()).unwrap_or("");
    let user_note = match validate::username(&app.username.value) {
        Err(e) => Some(Span::styled(e, t.fg(t.red))),
        Ok(()) if app.username.value != default_user => Some(Span::styled("the mac config uses this username too", t.fg(t.yellow))),
        Ok(()) => None,
    };
    r.push(input_row(t, Field::Username, "username", &app.username, focus == Some(Field::Username), user_note));
    r.push(input_row(
        t,
        Field::TimeZone,
        "time zone",
        &app.time_zone,
        focus == Some(Field::TimeZone),
        err(validate::time_zone(&app.time_zone.value, Some(Path::new("/etc/zoneinfo")))).filter(|_| typed(Field::TimeZone, &app.time_zone.value)),
    ));
    r.push(Row::blank());
    r.push(Row::heading(t, "look"));
    let themes = app.theme_names();
    let th = themes.get(app.theme_idx).cloned().unwrap_or_else(|| "match".into());
    r.push(choice_row(t, Field::Theme, "theme", &th, if th == "match" { "colours from the wallpaper" } else { "" }));
    let name_w = 16;
    let swatches = w >= 18 + 4 + name_w + 32;
    if let Some(d) = &app.data {
        for (i, wp) in d.wallpapers.iter().enumerate() {
            let mut spans = vec![
                label(t, if i == 0 { "wallpaper" } else { "" }),
                Span::styled(format!("{} ", radio(app.wallpaper_idx == i)), t.accent()),
                Span::styled(format!("{:w$}", wp.name, w = name_w), t.text()),
            ];
            if swatches {
                for k in SWATCH_KEYS {
                    let c = wp.colors.get(k).map(|h| t.swatch(h)).unwrap_or(t.bg);
                    spans.push(Span::styled("  ", Style::new().bg(c)));
                }
            } else {
                for k in ["base00", "base05", "base08", "base0B", "base0D"] {
                    let c = wp.colors.get(k).map(|h| t.swatch(h)).unwrap_or(t.bg);
                    spans.push(Span::styled("  ", Style::new().bg(c)));
                }
            }
            r.push(Row::field(Field::Wallpaper(i), Line::from(spans)));
        }
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Next, "next"));
    r
}

pub fn password_rows(app: &App, _w: usize) -> Vec<Row> {
    let t = &app.theme;
    let focus = app.focus;
    let matches = |a: &str, b: &str| -> Option<Span<'static>> {
        if b.is_empty() {
            None
        } else if a == b {
            Some(Span::styled("match", t.fg(t.green)))
        } else {
            Some(Span::styled("doesn't match", t.fg(t.red)))
        }
    };
    let mut r = vec![Row::heading(t, "disk")];
    if app.need_disk_pw() {
        r.push(Row::muted(t, "asked for at every boot until tpm unlock is set up. don't lose it."));
        r.push(input_row(t, Field::DiskPw1, "password", &app.disk_pw1, focus == Some(Field::DiskPw1), None));
        r.push(input_row(t, Field::DiskPw2, "again", &app.disk_pw2, focus == Some(Field::DiskPw2), matches(&app.disk_pw1.value, &app.disk_pw2.value)));
    } else if app.keeping() && app.eff_encrypt() {
        r.push(Row::muted(t, "the disk keeps its password: the one you unlocked it with"));
    } else {
        r.push(Row::muted(t, "not encrypted: no disk password"));
    }
    r.push(Row::blank());
    r.push(Row::heading(t, &format!("login ({})", app.user())));
    if app.carry_password() {
        r.push(Row::muted(t, "carried over from the old install"));
    } else {
        if app.carry.contains(&super::super::super::plan::Carry::Password) && app.unlocked.is_some() {
            r.push(Row::colored(t.yellow, format!("the old install has no password for {}: set one", app.user())));
        }
        if app.same_as_disk_offered() {
            r.push(toggle_row(t, Field::SameAsDisk, "same as disk", app.same_as_disk, "use the disk password to log in too"));
        }
        if !(app.same_as_disk && app.same_as_disk_offered()) {
            r.push(input_row(t, Field::LoginPw1, "password", &app.login_pw1, focus == Some(Field::LoginPw1), None));
            r.push(input_row(t, Field::LoginPw2, "again", &app.login_pw2, focus == Some(Field::LoginPw2), matches(&app.login_pw1.value, &app.login_pw2.value)));
        }
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Next, "next"));
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    let flip = matches!(a, Act::Select | Act::Toggle);
    match f {
        Field::Encrypt if flip && !app.keeping() => app.encrypt = !app.encrypt,
        Field::Tpm if flip && app.machine.as_ref().is_some_and(|m| m.tpm) => app.tpm = !app.tpm,
        Field::Swap if !app.keeping() => {
            let i = SWAP_CHOICES.iter().position(|s| *s == app.swap_gb).unwrap_or(2);
            let n = SWAP_CHOICES.len();
            app.swap_gb = SWAP_CHOICES[if a == Act::Left { (i + n - 1) % n } else { (i + 1) % n }];
        }
        Field::Theme => {
            let n = app.theme_names().len().max(1);
            app.theme_idx = if a == Act::Left { (app.theme_idx + n - 1) % n } else { (app.theme_idx + 1) % n };
        }
        Field::Wallpaper(i) if flip => app.wallpaper_idx = i,
        Field::SameAsDisk if flip => app.same_as_disk = !app.same_as_disk,
        _ => {}
    }
}
