//! 7. review: everything, what gets erased in red, type YES.

use super::super::app::{App, Field, HomeKind, Use, short};
use super::super::form::{Row, input_row, pad, wrap};
use super::super::super::plan::{Carry, GitHubMode, HostPlan};
use super::super::super::probe;
use super::super::validate::human_bytes;
use super::kind_label;
use ratatui::text::{Line, Span};

const W: usize = 12;

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let mut r = vec![];
    let kv = |r: &mut Vec<Row>, k: &str, v: String| {
        for (n, l) in wrap(&v, w.saturating_sub(W + 1)).into_iter().enumerate() {
            r.push(Row::info(Line::from(vec![Span::styled(pad(if n == 0 { k } else { "" }, W), t.muted()), Span::styled(l, t.text())])));
        }
    };

    // What gets erased first: it's the point of this screen.
    let keep = app.keeping();
    let erased = app.erased_disks();
    r.push(Row::heading(t, if erased.is_empty() { "nothing is erased" } else { "will be ERASED" }));
    for i in &erased {
        let d = &app.disks[*i];
        r.push(Row::colored(t.red, format!("  {}  {}  {}  {}", short(&d.path), human_bytes(d.size), d.model, kind_label(d))));
    }
    if keep && let Some(d) = app.system_disk() {
        r.push(Row::colored(t.yellow, format!("  {}: system and /persistent replaced, /home and the store kept", short(&d.path))));
    }
    if let Ok(plan) = app.build_plan() {
        for d in probe::clashing_old_installs(&app.disks, &plan) {
            let msg = format!(
                "old nixos install on {} is left as is: its partition names clash with the new install; it will be renamed (nothing erased)",
                short(&d)
            );
            for l in wrap(&msg, w.saturating_sub(3)) {
                r.push(Row::colored(t.yellow, format!("  {l}")));
            }
        }
    }
    r.push(Row::blank());

    match app.host_plan() {
        Ok(HostPlan::New(h)) => {
            let f: Vec<&str> = h.aspects.iter().map(String::as_str).filter(|a| *a != "base").collect();
            let f = if f.is_empty() { "base only".to_string() } else { f.join(" ") };
            kv(&mut r, "machine", format!("new: {} (modules/hosts/{}) · {f} · {} graphics", h.name, h.name, h.gpu.name()));
        }
        Ok(HostPlan::Existing(n)) => kv(&mut r, "machine", format!("{n}: reinstall (its hardware config stays)")),
        Err(e) => kv(&mut r, "machine", e),
    }
    let g = app.github_plan();
    let login = g.login.clone().unwrap_or_default();
    let gh = match &g.mode {
        GitHubMode::Owner if g.push => format!("{login} (owner) · pushes to {} main", g.upstream),
        GitHubMode::Owner => format!("{login} (owner) · not pushed: it stays in ~/nixos"),
        GitHubMode::Fork { url, branch } => {
            let f = url.trim_start_matches("https://github.com/").trim_end_matches(".git");
            format!("{login} · your fork {f} · pushes to its {branch} · meta.nix made yours")
        }
        GitHubMode::Offline => "offline · committed in ~/nixos only: push later with vx sync".to_string(),
    };
    kv(&mut r, "github", gh);
    if g.mode != GitHubMode::Offline && app.prepared_with.as_ref().is_some_and(|(fetch, _)| !fetch) {
        kv(&mut r, "", "the repo is the usb's copy (own history): it won't be pushed".into());
    }

    if let Some(d) = app.system_disk() {
        let mut v = format!("{} {}", short(&d.path), d.model);
        v += if app.eff_encrypt() { ", encrypted" } else { ", not encrypted" };
        if app.eff_encrypt() && app.tpm && app.machine.as_ref().is_some_and(|m| m.tpm) {
            v += ", tpm unlock";
        }
        let swap = app.eff_swap();
        v += &if swap == 0 { ", no swap file".to_string() } else { format!(", {swap} gib swap") };
        kv(&mut r, "system", v);
    }
    let mut data = vec![];
    for i in app.data_disks() {
        let (d, row) = (&app.disks[i], &app.data_rows[i]);
        match row.use_ {
            Use::NotUsed => {}
            Use::Format => data.push(format!(
                "{} formatted at {}{}",
                row.name.value,
                row.mount.value,
                if app.data_encrypt(i) { " (encrypted)" } else { "" }
            )),
            Use::Mount => {
                let p = probe::mountable_partition(d).map(|p| short(&p.path).to_string()).unwrap_or_default();
                data.push(format!("{} ({p}) at {}{}", row.name.value, row.mount.value, if row.read_only { " read-only" } else { "" }));
            }
        }
    }
    kv(&mut r, "data", if data.is_empty() { "none".into() } else { data.join("; ") });
    if app.unlocked.is_some() {
        let c: Vec<&str> = Carry::ALL
            .iter()
            .filter(|c| app.carry.contains(c))
            .map(|c| match c {
                Carry::HostKey => "ssh host key",
                Carry::NetBird => "netbird",
                Carry::SecureBoot => "secure boot keys",
                Carry::Password => "password",
                Carry::Pairings => "pairings",
                Carry::OtherState => "other state",
            })
            .collect();
        kv(&mut r, "carry over", if c.is_empty() { "nothing".into() } else { c.join(", ") });
        let home = match app.home {
            HomeKind::Keep if keep => "kept as it is".to_string(),
            HomeKind::Backup => format!("copied to {}, then fresh", app.backup_sel.as_deref().map(short).unwrap_or("?")),
            _ => "fresh".to_string(),
        };
        kv(&mut r, "/home", home);
    }
    let wp = app.data.as_ref().and_then(|d| d.wallpapers.get(app.wallpaper_idx)).map(|w| w.name.clone()).unwrap_or_default();
    let th = app.theme_names().get(app.theme_idx).cloned().unwrap_or_default();
    kv(&mut r, "settings", format!("{} · {} · {} · {wp} ({th})", app.host_name(), app.username.value, app.time_zone.value));
    let repo = match &app.prepared_with {
        Some((true, origin)) => format!("latest from {}", origin.trim_start_matches("https://").trim_end_matches(".git")),
        _ => "the copy on this usb".to_string(),
    };
    let mode = match app.backend.label() {
        "" => String::new(),
        l => format!(" · {l}: nothing is really done"),
    };
    kv(&mut r, "repo", format!("{repo}{mode}"));
    r.push(Row::blank());
    let ok = app.confirm.value == "YES";
    r.push(input_row(t, Field::ConfirmYes, "type YES", &app.confirm, app.focus == Some(Field::ConfirmYes), None));
    r.push(Row::field(Field::Install, Line::from(Span::styled("[ install ]", if ok { t.fg(t.red) } else { t.muted() }))).disabled(!ok));
    r
}
