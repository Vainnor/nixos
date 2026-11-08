//! `vx status`: one screen about this machine. Every check is quick, needs no
//! sudo and is skipped when its tool or unit isn't there. Checks run in
//! parallel and print in a fixed order.

use super::util::{ago, lock_inputs, mtime, now, run_timeout};
use crate::ctx::{self, Ctx, Host};
use crate::ui;
use anyhow::Result;
use owo_colors::{OwoColorize, Stream::Stdout};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Level {
    Ok,
    Warn,
    Fail,
    Info,
}

pub type Line = (Level, String);

fn ok(s: impl Into<String>) -> Line {
    (Level::Ok, s.into())
}
fn warn(s: impl Into<String>) -> Line {
    (Level::Warn, s.into())
}
fn fail(s: impl Into<String>) -> Line {
    (Level::Fail, s.into())
}
fn info(s: impl Into<String>) -> Line {
    (Level::Info, s.into())
}

pub fn print(lines: &[Line]) {
    for (lvl, s) in lines {
        match lvl {
            Level::Ok => ui::ok(s),
            Level::Warn => ui::warn(s),
            // ui::fail goes to stderr; keep status on one stream so it reads in order.
            Level::Fail => println!("{} {s}", "[ fail ]".if_supports_color(Stdout, |t| t.red())),
            Level::Info => ui::info(s),
        }
    }
}

const T: Duration = Duration::from_millis(1500);

pub fn run(ctx: &Ctx) -> Result<()> {
    let start = Instant::now();
    let arc = ctx.is_nixos();
    type Check<'a> = Box<dyn Fn(&Ctx) -> Vec<Line> + Send + Sync + 'a>;
    let mut checks: Vec<Check> = vec![Box::new(host), Box::new(generation), Box::new(repo), Box::new(inputs)];
    if arc {
        checks.push(Box::new(units));
    }
    checks.push(Box::new(disks));
    if arc {
        checks.push(Box::new(backups));
        checks.push(Box::new(secure_boot));
        checks.push(Box::new(tpm));
    }
    checks.push(Box::new(netbird));

    let results: Vec<Vec<Line>> = std::thread::scope(|s| {
        let handles: Vec<_> = checks.iter().map(|c| s.spawn(|| c(ctx))).collect();
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    for r in &results {
        print(r);
    }
    if ctx.verbose {
        ui::dim(&format!("took {} ms", start.elapsed().as_millis()));
    }
    Ok(())
}

fn probe(c: &mut Command) -> Option<String> {
    run_timeout(c, T).and_then(|(ok, s)| ok.then_some(s))
}

fn host(ctx: &Ctx) -> Vec<Line> {
    let name = probe(&mut Command::new("hostname")).unwrap_or_else(|| "?".into());
    let os = match ctx.host {
        Host::NixOs => std::fs::read_to_string("/run/current-system/nixos-version")
            .map(|v| format!("nixos {}", v.trim()))
            .unwrap_or_else(|_| "linux".into()),
        Host::Mac => probe(Command::new("sw_vers").arg("-productVersion"))
            .map(|v| format!("macos {v}"))
            .unwrap_or_else(|| "macos".into()),
    };
    vec![info(format!("host {name} ({os})"))]
}

fn generation(ctx: &Ctx) -> Vec<Line> {
    let prof = Path::new("/nix/var/nix/profiles/system");
    let Ok(target) = std::fs::read_link(prof) else {
        return vec![warn(match ctx.host {
            Host::Mac => "no system generation: nix-darwin isn't active (first time: see README \"The Mac\")",
            Host::NixOs => "no system profile at /nix/var/nix/profiles/system",
        })];
    };
    let gen_no = Regex::new(r"system-(\d+)-link")
        .ok()
        .and_then(|re| re.captures(&target.to_string_lossy()).map(|c| c[1].to_string()))
        .unwrap_or_else(|| "?".into());
    let link = prof.parent().unwrap_or(Path::new("/")).join(&target);
    let age = mtime(&link).map(|t| format!(", built {} ago", ago(now().saturating_sub(t)))).unwrap_or_default();
    let canon = |p: &str| std::fs::canonicalize(p).ok();
    let (current, default) = (canon("/run/current-system"), canon("/nix/var/nix/profiles/system"));

    let mut lines = vec![];
    if current.is_some() && current != default {
        lines.push(warn(format!("generation {gen_no}{age} is the default, but something else is running (vx test?); next boot uses {gen_no}")));
    } else if ctx.is_nixos() {
        let booted = canon("/run/booted-system");
        if booted == current {
            lines.push(ok(format!("generation {gen_no}{age}, booted = current")));
        } else if canon("/run/booted-system/kernel") != canon("/run/current-system/kernel")
            || canon("/run/booted-system/initrd") != canon("/run/current-system/initrd")
        {
            lines.push(warn(format!("generation {gen_no}{age}; kernel or initrd changed since boot: reboot to use it")));
        } else {
            lines.push(ok(format!("generation {gen_no}{age} (switched since boot, same kernel)")));
        }
    } else {
        lines.push(ok(format!("generation {gen_no}{age}")));
    }
    lines
}

/// `## main...origin/main [ahead 1, behind 2]` → (branch, has upstream, ahead, behind).
pub fn parse_branch(line: &str) -> (String, bool, u32, u32) {
    let l = line.trim_start_matches("## ");
    if let Some(b) = l.strip_prefix("No commits yet on ") {
        return (b.to_string(), false, 0, 0);
    }
    let (head, rest) = l.split_once(' ').unwrap_or((l, ""));
    let (branch, upstream) = match head.split_once("...") {
        Some((b, _)) => (b.to_string(), true),
        None => (head.to_string(), false),
    };
    let num = |key: &str| {
        Regex::new(&format!(r"{key} (\d+)")).ok().and_then(|re| re.captures(rest).and_then(|c| c[1].parse().ok())).unwrap_or(0)
    };
    (branch, upstream, num("ahead"), num("behind"))
}

fn repo(ctx: &Ctx) -> Vec<Line> {
    let Ok(repo) = ctx.repo() else { return vec![fail(format!("no flake at {} (pass --repo or set VX_REPO)", ctx.repo.display()))] };
    let Some(out) = probe(Command::new("git").arg("-C").arg(repo).args(["status", "--porcelain=v1", "--branch"])) else {
        return vec![warn(format!("{} isn't a git checkout", repo.display()))];
    };
    let mut it = out.lines();
    let (branch, upstream, ahead, behind) = parse_branch(it.next().unwrap_or(""));
    let dirty = it.filter(|l| !l.is_empty()).count();
    let mut parts = vec![];
    if dirty > 0 {
        parts.push(format!("{dirty} uncommitted"));
    }
    if ahead > 0 {
        parts.push(format!("{ahead} unpushed"));
    }
    if behind > 0 {
        parts.push(format!("{behind} behind (vx switch --pull)"));
    }
    if !upstream {
        parts.push("no upstream".into());
    }
    let s = format!("repo {branch}{}", if parts.is_empty() { ", clean and pushed".into() } else { format!(": {}", parts.join(", ")) });
    vec![if parts.is_empty() { ok(s) } else { warn(s) }]
}

fn inputs(ctx: &Ctx) -> Vec<Line> {
    let Ok(json) = std::fs::read_to_string(ctx.repo.join("flake.lock")) else { return vec![] };
    let all = lock_inputs(&json);
    let t = now();
    let days = |i: &super::util::Input| t.saturating_sub(i.last_modified) / 86400;
    let mut lines = vec![];
    if let Some(np) = all.iter().find(|i| i.name == "nixpkgs") {
        let d = days(np);
        let s = format!("nixpkgs is {d} d old");
        lines.push(if d > 30 { warn(format!("{s}: vx update")) } else { ok(s) });
    }
    let stale: Vec<String> = all.iter().filter(|i| i.name != "nixpkgs" && days(i) > 30).map(|i| format!("{} {} d", i.name, days(i))).collect();
    if !stale.is_empty() {
        lines.push(info(format!("inputs older than 30 d: {}", stale.join(", "))));
    }
    lines
}

fn units(_ctx: &Ctx) -> Vec<Line> {
    let failed = |user: bool| {
        let mut c = Command::new("systemctl");
        if user {
            c.arg("--user");
        }
        c.args(["--failed", "--no-legend", "--plain", "--no-pager"]);
        probe(&mut c).map(|s| s.lines().filter_map(|l| l.split_whitespace().next().map(str::to_string)).collect::<Vec<_>>())
    };
    let mut lines = vec![];
    for (user, what) in [(false, "system"), (true, "user")] {
        match failed(user) {
            Some(v) if v.is_empty() => lines.push(ok(format!("no failed {what} units"))),
            Some(v) => lines.push(fail(format!(
                "failed {what} units: {} (journalctl {}-u <unit>)",
                v.join(", "),
                if user { "--user " } else { "" }
            ))),
            None => {}
        }
    }
    lines
}

/// `df -Pk` rows → (mount, size KiB, used KiB, avail KiB, percent).
pub fn parse_df(out: &str) -> Vec<(String, u64, u64, u64, u32)> {
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 6 {
                return None;
            }
            let n = f.len();
            Some((
                f[n - 1].to_string(),
                f[n - 5].parse().ok()?,
                f[n - 4].parse().ok()?,
                f[n - 3].parse().ok()?,
                f[n - 2].trim_end_matches('%').parse().ok()?,
            ))
        })
        .collect()
}

fn human(kib: u64) -> String {
    let g = kib as f64 / 1024.0 / 1024.0;
    if g >= 1000.0 { format!("{:.1} T", g / 1024.0) } else if g >= 10.0 { format!("{g:.0} G") } else { format!("{g:.1} G") }
}

fn disks(_ctx: &Ctx) -> Vec<Line> {
    let paths: Vec<&str> = ["/", "/nix", "/persistent"].into_iter().filter(|p| Path::new(p).exists()).collect();
    let Some(out) = probe(Command::new("df").arg("-Pk").args(&paths)) else { return vec![] };
    // Group mounts that share one pool (btrfs subvolumes report the same numbers).
    let mut groups: Vec<(Vec<String>, u64, u64, u32)> = vec![];
    for (mount, size, _used, avail, pct) in parse_df(&out) {
        match groups.iter_mut().find(|g| g.1 == size && g.2 == avail) {
            Some(g) => {
                if !g.0.contains(&mount) {
                    g.0.push(mount)
                }
            }
            None => groups.push((vec![mount], size, avail, pct)),
        }
    }
    groups
        .into_iter()
        .map(|(mounts, _, avail, pct)| {
            let s = format!("disk {} {pct}% used, {} free", mounts.join(" "), human(avail));
            match pct {
                p if p >= 95 => fail(format!("{s}: vx clean")),
                p if p >= 85 => warn(format!("{s}: vx clean")),
                _ => ok(s),
            }
        })
        .collect()
}

/// `systemctl show` output (blocks of key=value split by blank lines).
pub fn parse_show(out: &str) -> Vec<HashMap<String, String>> {
    out.split("\n\n")
        .map(|b| b.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>())
        .filter(|m| !m.is_empty())
        .collect()
}

fn backups(_ctx: &Ctx) -> Vec<Line> {
    // Unit names from modules/system/backups.nix: services.btrbk.instances.local
    // and services.restic.backups.homelab.
    let jobs = [("snapshots", "btrbk-local", 3 * 3600), ("restic", "restic-backups-homelab", 2 * 86400)];
    let mut c = Command::new("systemctl");
    c.args(["show", "--timestamp=unix", "-p", "Id,LoadState,LastTriggerUSec,Result,ActiveState"]);
    for (_, u, _) in &jobs {
        c.arg(format!("{u}.timer")).arg(format!("{u}.service"));
    }
    let Some(out) = probe(&mut c) else { return vec![] };
    let units = parse_show(&out);
    let get = |id: &str| units.iter().find(|m| m.get("Id").map(String::as_str) == Some(id));
    let mut lines = vec![];
    for (what, u, max_age) in jobs {
        let (Some(timer), Some(svc)) = (get(&format!("{u}.timer")), get(&format!("{u}.service"))) else { continue };
        if timer.get("LoadState").map(String::as_str) != Some("loaded") {
            if what == "restic" {
                lines.push(info("restic backups are off (meta.resticReady)"));
            }
            continue;
        }
        let last = timer.get("LastTriggerUSec").and_then(|v| v.strip_prefix('@')).and_then(|v| v.parse::<u64>().ok());
        let result = svc.get("Result").map(String::as_str).unwrap_or("?");
        let running = svc.get("ActiveState").map(String::as_str) == Some("activating");
        lines.push(match last {
            None => warn(format!("{what} ({u}) never ran yet")),
            Some(_) if running => info(format!("{what} running now")),
            Some(t) if result != "success" => fail(format!("{what} last run {} ago failed ({result}): journalctl -u {u}", ago(now().saturating_sub(t)))),
            Some(t) if now().saturating_sub(t) > max_age => warn(format!("{what} last ran {} ago", ago(now().saturating_sub(t)))),
            Some(t) => ok(format!("{what} {} ago", ago(now().saturating_sub(t)))),
        });
    }
    lines
}

/// The `Secure Boot:` line of `bootctl status`, lowercased.
pub fn parse_secure_boot(out: &str) -> Option<String> {
    out.lines().find_map(|l| l.trim().strip_prefix("Secure Boot:").map(|v| v.trim().to_lowercase()))
}

fn secure_boot(_ctx: &Ctx) -> Vec<Line> {
    if !ctx::has("bootctl") {
        return vec![];
    }
    // bootctl may exit non-zero without root (ESP not readable) but still prints the firmware part.
    let Some((_, out)) = run_timeout(Command::new("bootctl").args(["status", "--no-pager"]), T) else { return vec![] };
    match parse_secure_boot(&out) {
        Some(s) if s.starts_with("enabled") => vec![ok(format!("secure boot {s}"))],
        Some(s) if s.contains("setup") => vec![warn(format!("secure boot {s}: reboot so lanzaboote enrolls the keys, then turn it on in the BIOS"))],
        Some(s) => vec![warn(format!("secure boot {s} (README \"Secure Boot + TPM\")"))],
        None => vec![],
    }
}

fn tpm(_ctx: &Ctx) -> Vec<Line> {
    // Reading this boot's journal works for wheel users; no sudo.
    let Some(out) = probe(Command::new("journalctl").args(["-b", "-q", "-o", "cat", "--no-pager", "-u", "systemd-cryptsetup@cryptroot.service"])) else {
        return vec![];
    };
    if out.trim().is_empty() {
        return vec![];
    }
    if out.contains("TPM2 token unlocks volume") || (out.contains("TPM2") && out.contains("unlock")) {
        vec![ok("disk unlocked by the tpm this boot")]
    } else {
        vec![info("disk unlocked with the passphrase this boot (vx tpm-enroll once secure boot is on)")]
    }
}

/// Mac: the hand-installed NetBird app ships its CLI inside the bundle.
pub fn netbird_bin() -> Option<String> {
    if ctx::has("netbird") {
        return Some("netbird".into());
    }
    let app = "/Applications/NetBird.app/Contents/MacOS/netbird";
    Path::new(app).exists().then(|| app.into())
}

pub fn parse_netbird(out: &str) -> HashMap<String, String> {
    out.lines().filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

fn netbird(_ctx: &Ctx) -> Vec<Line> {
    let Some(bin) = netbird_bin() else { return vec![] };
    let Some((_, out)) = run_timeout(Command::new(bin).arg("status"), T) else {
        return vec![warn("netbird status timed out")];
    };
    let m = parse_netbird(&out);
    let mgmt = m.get("Management").map(String::as_str).unwrap_or("");
    let peers = m.get("Peers count").map(|p| format!(", {} peers", p.trim_end_matches(" Connected"))).unwrap_or_default();
    let name = m.get("FQDN").map(|f| format!(", {f}")).unwrap_or_default();
    if mgmt.starts_with("Connected") {
        vec![ok(format!("netbird connected{peers}{name}"))]
    } else if let Some(d) = m.get("Daemon status") {
        vec![warn(format!("netbird {}: sudo netbird up", d.to_lowercase()))]
    } else {
        vec![warn(format!("netbird {}: sudo netbird up", if mgmt.is_empty() { "not connected".into() } else { mgmt.to_lowercase() }))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch() {
        assert_eq!(parse_branch("## main...origin/main [ahead 1, behind 2]"), ("main".into(), true, 1, 2));
        assert_eq!(parse_branch("## main...origin/main"), ("main".into(), true, 0, 0));
        assert_eq!(parse_branch("## feat"), ("feat".into(), false, 0, 0));
        assert_eq!(parse_branch("## No commits yet on main"), ("main".into(), false, 0, 0));
        assert_eq!(parse_branch("## main...origin/main [behind 3]"), ("main".into(), true, 0, 3));
    }

    #[test]
    fn df() {
        let out = "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                   /dev/mapper/cryptroot 976000000 400000000 570000000 42% /\n\
                   /dev/mapper/cryptroot 976000000 400000000 570000000 42% /nix\n";
        let v = parse_df(out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[1], ("/nix".into(), 976000000, 400000000, 570000000, 42));
        assert_eq!(human(570000000), "544 G");
    }

    #[test]
    fn show() {
        let out = "Id=btrbk-local.timer\nLoadState=loaded\nLastTriggerUSec=@1700000000\n\nId=btrbk-local.service\nResult=success\n";
        let v = parse_show(out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["LastTriggerUSec"], "@1700000000");
        assert_eq!(v[1]["Result"], "success");
    }

    #[test]
    fn secureboot() {
        let out = "System:\n      Firmware: UEFI 2.80\n   Secure Boot: enabled (user)\n  TPM2 Support: yes\n";
        assert_eq!(parse_secure_boot(out).as_deref(), Some("enabled (user)"));
        assert_eq!(parse_secure_boot("nothing"), None);
    }

    #[test]
    fn nb() {
        let m = parse_netbird("Daemon version: 0.82.0\nManagement: Connected\nFQDN: arc-211-139.netbird.cloud\nPeers count: 2/3 Connected\n");
        assert_eq!(m["Management"], "Connected");
        assert_eq!(m["Peers count"], "2/3 Connected");
    }
}
