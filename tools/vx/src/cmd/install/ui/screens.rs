//! One module per screen: `rows` (what it shows) and the field actions.

mod disks;
pub mod github;
mod machine;
mod netbird;
mod options;
mod previous;
mod progress;
mod review;
mod welcome;
mod wifi;

use super::app::{Act, App, Field, Screen};
use super::form::{self, Row};
use super::theme::Theme;
use super::super::plan::{DiskInfo, DiskKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;

pub fn rows(app: &App, width: u16) -> Vec<Row> {
    let w = width as usize;
    match app.screen {
        Screen::Welcome => welcome::rows(app, w),
        Screen::GitHub => github::rows(app, w),
        Screen::Machine => machine::rows(app, w),
        Screen::Previous => previous::rows(app, w),
        Screen::NetBird => netbird::rows(app, w),
        Screen::System => disks::system_rows(app, w),
        Screen::Data => disks::data_rows(app, w),
        Screen::Options => options::rows(app, w),
        Screen::Passwords => options::password_rows(app, w),
        Screen::Review => review::rows(app, w),
        Screen::Install => vec![],
        Screen::Summary => progress::summary_rows(app, w),
        Screen::Failed => progress::failed_rows(app, w),
        Screen::Wifi => wifi::rows(app, w),
    }
}

pub fn draw_body(f: &mut Frame, area: Rect, app: &App) {
    if app.screen == Screen::Install {
        return progress::draw(f, area, app);
    }
    form::draw_rows(f, area, rows(app, area.width), app.focus, &app.theme);
}

pub fn act(app: &mut App, f: Field, a: Act) {
    match f {
        Field::FetchLatest | Field::Start => welcome::act(app, f, a),
        Field::WifiOpen | Field::WifiNet(_) | Field::WifiConnect | Field::WifiRescan | Field::WifiBack => wifi::act(app, f, a),
        Field::GhLogin | Field::GhCancel | Field::GhSkip | Field::GhPush | Field::GhFork | Field::GhQuit => github::act(app, f, a),
        Field::NbLogin | Field::NbCancel | Field::NbSkip => netbird::act(app, f, a),
        Field::RebootFirmware if app.screen == Screen::Welcome => welcome::act(app, f, a),
        Field::Host(_) | Field::NewHost | Field::Aspect(_) | Field::Gpu => machine::act(app, f, a),
        Field::Unlock | Field::SkipPrevious | Field::Carry(_) | Field::CarryNone | Field::Home(_) | Field::Backup(_) | Field::Rescan => {
            previous::act(app, f, a)
        }
        Field::Disk(_) | Field::DataUse(_) | Field::DataEncrypt(_) | Field::DataReadOnly(_) => disks::act(app, f, a),
        Field::Encrypt | Field::Tpm | Field::Swap | Field::Theme | Field::Wallpaper(_) | Field::SameAsDisk => options::act(app, f, a),
        Field::Install => {
            if a == Act::Select || a == Act::Toggle {
                app.next();
            }
        }
        Field::Next => {
            if a == Act::Select || a == Act::Toggle {
                app.next();
            }
        }
        Field::Reboot | Field::RebootFirmware | Field::Shell => progress::act(app, f, a),
        _ => {}
    }
}

/// What's on a disk, in a few words.
pub fn kind_label(d: &DiskInfo) -> &'static str {
    match d.kind {
        DiskKind::Windows { bitlocker: true, system: true } => "windows c: (bitlocker)",
        DiskKind::Windows { bitlocker: true, .. } => "windows bitlocker",
        DiskKind::Windows { system: true, .. } => "windows c:",
        DiskKind::Windows { .. } => "windows data (ntfs)",
        DiskKind::NixOs => "nixos (this flake)",
        DiskKind::Linux => "linux",
        DiskKind::Empty => "empty",
        DiskKind::Installer => "installer usb",
        DiskKind::Other => "other",
    }
}

pub fn kind_style(t: &Theme, d: &DiskInfo) -> ratatui::style::Style {
    match d.kind {
        DiskKind::Windows { .. } => t.fg(t.yellow),
        DiskKind::NixOs => t.accent(),
        DiskKind::Installer => t.muted(),
        _ => t.text(),
    }
}

/// `[ok] text` / `[--] text` / `[!!] text`
pub fn mark(t: &Theme, level: u8, text: impl Into<String>) -> Row {
    let (m, c) = match level {
        0 => ("[ok]", t.green),
        1 => ("[--]", t.yellow),
        _ => ("[!!]", t.red),
    };
    Row::info(ratatui::text::Line::from(vec![Span::styled(m, t.fg(c)), Span::styled(format!(" {}", text.into()), t.text())]))
}

/// Wrapped muted paragraph rows.
pub fn para(t: &Theme, s: &str, width: usize) -> Vec<Row> {
    form::wrap(s, width.saturating_sub(1)).into_iter().map(|l| Row::muted(t, l)).collect()
}
