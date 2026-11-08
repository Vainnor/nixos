//! 1. welcome: what this machine looks like, and where the repo comes from.

use super::super::app::{Act, App, Field, Level};
use super::super::form::{Row, button_row, toggle_row};
use super::{mark, para};

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let mut r = vec![Row::heading(t, "install nixos on this pc"), Row::blank()];
    r.extend(para(
        t,
        "pick the drives and a few options; the installer sets everything up from the flake. nothing is changed until you type YES on the review screen.",
        w,
    ));
    r.push(Row::blank());
    let Some(m) = &app.machine else {
        r.push(Row::muted(t, "checking this machine ..."));
        return r;
    };
    let real = app.backend.is_real();
    r.push(if m.root {
        mark(t, 0, "running as root")
    } else if real {
        mark(t, 2, "not root: quit and run  sudo vx install")
    } else {
        mark(t, 1, "not root (fine for a dry run)")
    });
    r.push(if m.uefi {
        mark(t, 0, "booted in uefi mode")
    } else {
        mark(t, if real { 2 } else { 1 }, "not uefi: reboot the usb with its uefi: boot menu entry")
    });
    r.push(if m.online { mark(t, 0, "online") } else { mark(t, 1, "offline: installs the repo copy on this usb") });
    r.push(if m.setup_mode {
        mark(t, 0, "secure boot in setup mode: the new system can enroll its keys")
    } else if m.secure_boot {
        mark(t, 1, "secure boot is on: turn it off in the bios before rebooting")
    } else {
        mark(t, 0, "secure boot off (turned on after the first boot)")
    });
    r.push(if m.tpm { mark(t, 0, "tpm found") } else { mark(t, 1, "no tpm: the disk unlocks with its password only") });
    let disks = match &app.probe_error {
        Some(e) => mark(t, 2, format!("couldn't list the disks: {e}")),
        None => {
            let prev = app.previous.as_ref().map(|p| format!(", nixos install on {}", super::super::app::short(&p.disk))).unwrap_or_default();
            mark(t, 0, format!("{} disks, {} gib ram{prev}", app.disks.len(), m.ram_gb))
        }
    };
    r.push(disks);
    r.push(Row::blank());
    let lbl = if app.offline {
        "use the copy on this usb (--offline)"
    } else if !m.online {
        "use the copy on this usb (no network)"
    } else if app.fetch_latest {
        "fetch the latest from github"
    } else {
        "use the copy on this usb"
    };
    r.push(toggle_row(t, Field::FetchLatest, "repo", app.fetch_latest, lbl).disabled(app.offline || !m.online));
    r.push(Row::blank());
    r.push(button_row(t, Field::Start, "start").disabled(app.probe_error.is_some()));
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    match (f, a) {
        (Field::FetchLatest, Act::Select | Act::Toggle) => {
            if !app.offline && app.machine.as_ref().is_some_and(|m| m.online) {
                app.fetch_latest = !app.fetch_latest;
            }
        }
        (Field::Start, Act::Select | Act::Toggle) => {
            let Some(m) = &app.machine else { return };
            if app.probe_error.is_some() {
                return app.say(Level::Error, "the disks couldn't be read: see above");
            }
            if app.backend.is_real() && !m.root {
                return app.say(Level::Error, "run it as root: q, then  sudo vx install");
            }
            if app.backend.is_real() && !m.uefi {
                return app.say(Level::Error, "booted without uefi: reboot the usb in uefi mode");
            }
            app.start_flow();
        }
        _ => {}
    }
}
