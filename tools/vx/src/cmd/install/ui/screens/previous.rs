//! 2. previous install: unlock it, pick what to carry over, decide /home.

use super::super::app::{Act, App, Field, HomeKind, short};
use super::super::form::{Row, button_row, check, input_row, radio};
use super::super::validate::human_bytes;
use super::para;
use ratatui::text::{Line, Span};

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let Some(prev) = &app.previous else { return vec![] };
    let disk = app.disks.iter().find(|d| d.path == prev.disk);
    let what = disk.map(|d| format!("{}  {}  {}", short(&d.path), human_bytes(d.size), d.model)).unwrap_or_else(|| prev.disk.clone());

    let Some(u) = &app.unlocked else {
        let mut r = vec![
            Row::heading(t, "found a nixos install made by this flake"),
            Row::info(Line::from(Span::styled(format!("  {what}"), t.accent()))),
            Row::blank(),
        ];
        r.extend(para(
            t,
            "unlock it to carry things over (the ssh host key keeps secrets working, netbird, secure boot keys, your password) and to keep or back up /home. it's mounted read-only.",
            w,
        ));
        r.push(Row::blank());
        if prev.encrypted {
            r.push(input_row(t, Field::UnlockPassword, "disk password", &app.unlock_pw, app.focus == Some(Field::UnlockPassword), None));
        }
        if let Some(e) = &app.unlock_error {
            r.push(Row::colored(t.red, format!("{:18}{e}", "")));
        }
        r.push(Row::blank());
        r.push(button_row(t, Field::Unlock, if prev.encrypted { "unlock" } else { "open it" }));
        r.push(button_row(t, Field::SkipPrevious, "skip: carry nothing over"));
        if app.backend.is_demo() {
            r.push(Row::blank());
            r.push(Row::muted(t, "demo: any password works"));
        }
        return r;
    };

    let mut r = vec![
        Row::muted(t, format!("mounted read-only: {} and {}", u.persistent.display(), u.home.display())),
        Row::blank(),
        Row::heading(t, "carry over"),
    ];
    if u.available.is_empty() {
        r.push(Row::muted(t, "  nothing found to carry over"));
    }
    for c in &u.available {
        let on = app.carry.contains(c);
        r.push(Row::field(
            Field::Carry(*c),
            Line::from(vec![Span::styled(format!("  {}", check(on)), t.accent()), Span::styled(format!(" {}", c.label()), t.text())]),
        ));
    }
    if !u.available.is_empty() {
        r.push(button_row(t, Field::CarryNone, "none"));
    }
    r.push(Row::blank());
    r.push(Row::info(Line::from(vec![
        Span::styled("/home", t.bold()),
        Span::styled(format!("  {} used", human_bytes(u.home_bytes)), t.muted()),
    ])));
    let home = |k: HomeKind, name: &str, note: String| {
        Row::field(
            Field::Home(k),
            Line::from(vec![
                Span::styled(format!("  {}", radio(app.home == k)), t.accent()),
                Span::styled(format!(" {name:9}"), t.text()),
                Span::styled(note, t.muted()),
            ]),
        )
    };
    r.push(home(HomeKind::Fresh, "fresh", "a new, empty /home".into()));
    r.push(home(HomeKind::Keep, "keep", format!("reinstall onto {} and leave /home as it is", short(&u.previous.disk))));
    r.push(home(HomeKind::Backup, "back up", "copy it to a drive that isn't erased, then fresh".into()));
    if app.home == HomeKind::Keep {
        r.push(Row::muted(t, "               encryption and swap stay as they were"));
    }
    if app.home == HomeKind::Backup {
        match &app.backup_targets {
            None => r.push(Row::muted(t, "               looking ...")),
            Some(ts) if ts.is_empty() => {
                for l in super::super::form::wrap("no ext4, btrfs, xfs or exfat partition outside the disks being erased. plug a drive in and rescan.", w.saturating_sub(16)) {
                    r.push(Row::colored(t.yellow, format!("               {l}")));
                }
            }
            Some(ts) => {
                for (i, x) in ts.iter().enumerate() {
                    let fits = x.free.map(|f| f >= u.home_bytes);
                    let (room, c) = match (x.free, fits) {
                        (Some(f), Some(true)) => (format!("{} free, fits", human_bytes(f)), t.green),
                        (Some(f), _) => (format!("{} free, too small", human_bytes(f)), t.red),
                        (None, _) => ("free space unknown".into(), t.yellow),
                    };
                    let name = format!("{} {}", short(&x.partition), x.label.as_deref().map(|l| format!("\"{l}\"")).unwrap_or_default());
                    r.push(Row::field(
                        Field::Backup(i),
                        Line::from(vec![
                            Span::styled(format!("      {}", radio(app.backup_sel.as_ref() == Some(&x.partition))), t.accent()),
                            Span::styled(format!(" {:16} {:6} ", super::super::form::truncate(&name, 16), x.fstype), t.text()),
                            Span::styled(room, t.fg(c)),
                        ]),
                    ));
                }
            }
        }
        r.push(button_row(t, Field::Rescan, "rescan"));
    }
    r.push(Row::blank());
    r.push(button_row(t, Field::Next, "next"));
    r
}

pub fn act(app: &mut App, f: Field, a: Act) {
    if !matches!(a, Act::Select | Act::Toggle) {
        return;
    }
    match f {
        Field::Unlock => app.unlock(),
        Field::SkipPrevious => {
            app.skipped_previous = true;
            app.next();
        }
        Field::Carry(c) => {
            if !app.carry.remove(&c) {
                app.carry.insert(c);
            }
        }
        Field::CarryNone => app.carry.clear(),
        Field::Home(k) => {
            app.home = k;
            if k == HomeKind::Keep
                && let Some(u) = &app.unlocked
                && let Some(i) = app.disks.iter().position(|d| d.path == u.previous.disk)
            {
                app.system = Some(i);
                app.data_rows[i].use_ = super::super::app::Use::NotUsed;
            }
            if k == HomeKind::Backup && app.backup_targets.is_none() {
                app.scan_backup();
            }
        }
        Field::Backup(i) => {
            if let Some(x) = app.backup_targets.as_ref().and_then(|t| t.get(i)) {
                app.backup_sel = Some(x.partition.clone());
            }
        }
        Field::Rescan => app.scan_backup(),
        _ => {}
    }
}
