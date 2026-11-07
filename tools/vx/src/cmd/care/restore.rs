//! `vx restore`: get a file or directory back from the hourly btrbk snapshots
//! (system/backups.nix). btrbk snapshots @home and @persistent into
//! /.btrfs/@snapshots as `<subvolume>.<YYYYMMDDTHHMM>` (timestamp_format long),
//! so a path is mapped to its subvolume through mountinfo, then looked up in
//! each snapshot. `--restic` keeps the old off-site restore.

use super::mounts;
use super::persist::{ago, human, now};
use crate::ctx::{Ctx, Host};
use crate::ui;
use anyhow::{Context, Result, bail};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const SNAPSHOT_DIR: &str = "/.btrfs/@snapshots";
/// The subvolumes btrbk snapshots (services.btrbk.instances.local).
const SNAPSHOTTED: &[&str] = &["@home", "@persistent"];

#[derive(clap::Args)]
pub struct Args {
    /// File or directory to bring back (it may already be deleted)
    pub path: PathBuf,
    /// Snapshot to take it from (e.g. 20261107T1400 or @home.20261107T1400); skips the prompt
    #[arg(long)]
    pub from: Option<String>,
    /// Replace the path itself instead of writing <path>.restored-<date>
    #[arg(long)]
    pub overwrite: bool,
    /// Don't ask before replacing
    #[arg(long, short)]
    pub yes: bool,
    /// Restore from the restic backup on the homelab instead (into /tmp/restore; --from takes a restic snapshot id)
    #[arg(long)]
    pub restic: bool,
}

/// A snapshot directory name: `@home.20261107T1400` or `@home.20261107T1400_1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snap {
    pub name: String,
    pub stamp: String,
}

pub fn parse_snap(name: &str, subvol: &str) -> Option<Snap> {
    let rest = name.strip_prefix(subvol)?.strip_prefix('.')?;
    let stamp = rest.split('_').next()?;
    let ok = stamp.len() >= 13
        && stamp.as_bytes()[8] == b'T'
        && stamp[..8].bytes().all(|c| c.is_ascii_digit())
        && stamp[9..].bytes().all(|c| c.is_ascii_digit());
    ok.then(|| Snap {
        name: name.to_string(),
        stamp: stamp.to_string(),
    })
}

/// `20261107T1400` -> `2026-11-07 14:00`
pub fn pretty(stamp: &str) -> String {
    if stamp.len() < 13 {
        return stamp.to_string();
    }
    format!(
        "{}-{}-{} {}:{}",
        &stamp[..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13]
    )
}

/// `20261107T1400` -> `2026-11-07T1400` for file names.
pub fn file_date(stamp: &str) -> String {
    if stamp.len() < 9 {
        return stamp.to_string();
    }
    format!("{}-{}-{}{}", &stamp[..4], &stamp[4..6], &stamp[6..8], &stamp[8..])
}

/// Does `--from` name this snapshot?
pub fn matches_from(s: &Snap, from: &str) -> bool {
    s.name == from || s.stamp == from || s.name.ends_with(&format!(".{from}"))
}

/// Make the path absolute and resolve symlinks in the part that still exists
/// (the rest may be deleted, which is the usual reason to restore).
pub fn resolve(path: &Path) -> Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut existing = abs.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(c) = existing.canonicalize() {
            let mut out = c;
            out.extend(tail.iter().rev());
            return Ok(out);
        }
        match (existing.parent(), existing.file_name()) {
            (Some(p), Some(n)) => {
                tail.push(n.to_owned());
                existing = p;
            }
            _ => return Ok(abs),
        }
    }
}

/// One version of the path in a snapshot.
#[derive(Debug, Clone)]
pub struct Version {
    pub snap: Snap,
    pub src: PathBuf,
    pub dir: bool,
    pub size: u64,
    pub mtime: i64,
    /// Older snapshots that hold the same version.
    pub same: usize,
}

/// Fold runs of identical versions (same size and mtime) into the newest one.
pub fn collapse(mut vs: Vec<Version>) -> Vec<Version> {
    vs.sort_by(|a, b| b.snap.stamp.cmp(&a.snap.stamp).then(b.snap.name.cmp(&a.snap.name)));
    let mut out: Vec<Version> = Vec::new();
    for v in vs {
        match out.last_mut() {
            Some(last) if last.size == v.size && last.mtime == v.mtime && last.dir == v.dir => last.same += 1,
            _ => out.push(v),
        }
    }
    out
}

/// Run, keep stdout whatever the exit status (find and du still print what
/// they could when some paths are missing).
fn lenient(ctx: &Ctx, sudo: bool, prog: &str, args: &[String]) -> Result<String> {
    let mut c = if sudo { ctx.sudo(prog) } else { Command::new(prog) };
    c.args(args).stderr(Stdio::null());
    if ctx.verbose {
        ui::dim(&format!(
            "$ {}{prog} {}",
            if sudo { "sudo " } else { "" },
            args.join(" ")
        ));
    }
    let out = c.output().with_context(|| format!("could not start {prog}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn run(ctx: &Ctx, a: Args) -> Result<()> {
    ctx.need(Host::Arc)?;
    let path = resolve(&a.path)?;
    if a.restic {
        return restic(ctx, &path, a.from.as_deref());
    }
    let mounts = mounts::read()?;
    let (subvol, inner) = mounts::btrfs_location(&mounts, &path)?;
    if !SNAPSHOTTED.contains(&subvol.as_str()) {
        bail!(
            "{} is in {subvol}, which isn't snapshotted (only {}); try --restic",
            path.display(),
            SNAPSHOTTED.join(" and ")
        );
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    if a.overwrite && (inner.components().count() <= 1 || home.as_deref() == Some(path.as_path())) {
        bail!(
            "won't --overwrite {} as a whole; restore it next to itself, or restore what's inside",
            path.display()
        );
    }

    // Snapshots are readable as the user when permissions allow; else use sudo.
    let snapdir = Path::new(SNAPSHOT_DIR);
    let sudo = match std::fs::read_dir(snapdir) {
        Ok(_) => false,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => true,
        Err(e) => bail!("can't open {SNAPSHOT_DIR} ({e}); is /.btrfs mounted?"),
    };
    let names = lenient(
        ctx,
        sudo,
        "find",
        &[
            SNAPSHOT_DIR.into(),
            "-mindepth".into(),
            "1".into(),
            "-maxdepth".into(),
            "1".into(),
            "-printf".into(),
            "%f\\n".into(),
        ],
    )?;
    let snaps: Vec<Snap> = names.lines().filter_map(|n| parse_snap(n, &subvol)).collect();
    if snaps.is_empty() {
        bail!("no snapshots of {subvol} yet; take one with: vx snapshot");
    }

    // Stat the path in every snapshot in one go.
    let srcs: Vec<String> = snaps
        .iter()
        .map(|s| snapdir.join(&s.name).join(&inner).display().to_string())
        .collect();
    let mut args = srcs.clone();
    args.extend([
        "-maxdepth".into(),
        "0".into(),
        "-printf".into(),
        "%y\\t%s\\t%T@\\t%p\\n".into(),
    ]);
    let stats = lenient(ctx, sudo, "find", &args)?;
    let mut versions: Vec<Version> = Vec::new();
    for line in stats.lines() {
        let f: Vec<&str> = line.splitn(4, '\t').collect();
        if f.len() != 4 {
            continue;
        }
        let Some(i) = srcs.iter().position(|s| s == f[3]) else {
            continue;
        };
        versions.push(Version {
            snap: snaps[i].clone(),
            src: PathBuf::from(f[3]),
            dir: f[0] == "d",
            size: f[1].parse().unwrap_or(0),
            mtime: f[2].split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0),
            same: 0,
        });
    }
    if versions.is_empty() {
        bail!(
            "{} isn't in any of the {} snapshots of {subvol}",
            path.display(),
            snaps.len()
        );
    }
    // Directory sizes: what's inside, not the directory entry.
    let dirs: Vec<String> = versions
        .iter()
        .filter(|v| v.dir)
        .map(|v| v.src.display().to_string())
        .collect();
    if !dirs.is_empty() {
        let mut du = vec!["-sb".to_string()];
        du.extend(dirs);
        for line in lenient(ctx, sudo, "du", &du)?.lines() {
            if let Some((n, p)) = line.split_once('\t')
                && let Some(v) = versions.iter_mut().find(|v| v.src.as_os_str() == p)
            {
                v.size = n.parse().unwrap_or(v.size);
            }
        }
    }
    let versions = collapse(versions);

    let pick = match a.from.as_deref() {
        Some(from) => versions
            .iter()
            .find(|v| matches_from(&v.snap, from))
            .or_else(|| versions.iter().find(|v| v.snap.stamp.starts_with(from)))
            .with_context(|| {
                format!("no snapshot matching {from} holds a distinct version; run without --from to list them")
            })?
            .clone(),
        None => choose(&path, &versions)?,
    };

    let dest = if a.overwrite {
        path.clone()
    } else {
        let name = path.file_name().context("can't restore /")?.to_string_lossy();
        path.with_file_name(format!("{name}.restored-{}", file_date(&pick.snap.stamp)))
    };
    if dest.symlink_metadata().is_ok() {
        if !a.overwrite {
            bail!("{} already exists; move it away or pass --overwrite", dest.display());
        }
        if !a.yes
            && !ui::confirm(&format!(
                "replace {} with the version from {}?",
                dest.display(),
                pretty(&pick.snap.stamp)
            ))
        {
            bail!("nothing changed");
        }
        let mut rm = if sudo { ctx.sudo("rm") } else { Command::new("rm") };
        ctx.run(rm.arg("-rf").arg("--").arg(&dest))?;
    }
    let mut cp = if sudo { ctx.sudo("cp") } else { Command::new("cp") };
    ctx.run(cp.args(["-a", "--reflink=auto", "--"]).arg(&pick.src).arg(&dest))?;
    ui::ok(&format!(
        "restored {} from {} to {}",
        path.display(),
        pretty(&pick.snap.stamp),
        dest.display()
    ));
    Ok(())
}

/// Numbered list, newest first; Enter picks the newest.
fn choose(path: &Path, vs: &[Version]) -> Result<Version> {
    let now = now();
    ui::heading(&format!("versions of {}", path.display()));
    for (i, v) in vs.iter().enumerate() {
        let same = if v.same > 0 {
            format!("  (same in {} older)", v.same)
        } else {
            String::new()
        };
        println!(
            "  {:>2}  {}  {:>8}  changed {}{same}",
            i + 1,
            pretty(&v.snap.stamp),
            human(v.size),
            ago(now, v.mtime)
        );
    }
    if vs.len() == 1 || !std::io::stdin().is_terminal() {
        if !std::io::stdin().is_terminal() && vs.len() > 1 {
            bail!("pick one with --from <snapshot>");
        }
        return Ok(vs[0].clone());
    }
    use std::io::Write;
    print!("restore which? [1] ");
    std::io::stdout().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    let n: usize = match s.trim() {
        "" => 1,
        t => t
            .parse()
            .ok()
            .filter(|n| (1..=vs.len()).contains(n))
            .context("not a number from the list")?,
    };
    Ok(vs[n - 1].clone())
}

/// The old `just restore`: from restic into /tmp/restore.
fn restic(ctx: &Ctx, path: &Path, from: Option<&str>) -> Result<()> {
    if !crate::ctx::has("restic-homelab") {
        bail!("restic isn't set up; fill restic/* in the secrets and set meta.resticReady = true");
    }
    let target = "/tmp/restore";
    ctx.run(
        ctx.sudo("restic-homelab")
            .args(["restore", from.unwrap_or("latest"), "--include"])
            .arg(path)
            .args(["--target", target]),
    )?;
    ui::ok(&format!("restored into {target}{}", path.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_names() {
        assert_eq!(
            parse_snap("@home.20261107T1400", "@home"),
            Some(Snap {
                name: "@home.20261107T1400".into(),
                stamp: "20261107T1400".into()
            })
        );
        assert_eq!(
            parse_snap("@home.20261107T1400_1", "@home").unwrap().stamp,
            "20261107T1400"
        );
        assert!(parse_snap("@persistent.20261107T1400", "@home").is_none());
        assert!(parse_snap("@homex.20261107T1400", "@home").is_none());
        assert!(parse_snap("@home.garbage", "@home").is_none());
        assert!(parse_snap("@home", "@home").is_none());
    }

    #[test]
    fn dates() {
        assert_eq!(pretty("20261107T1405"), "2026-11-07 14:05");
        assert_eq!(file_date("20261107T1405"), "2026-11-07T1405");
    }

    #[test]
    fn from_matching() {
        let s = parse_snap("@home.20261107T1400", "@home").unwrap();
        assert!(matches_from(&s, "20261107T1400"));
        assert!(matches_from(&s, "@home.20261107T1400"));
        assert!(!matches_from(&s, "20261107T1500"));
    }

    fn v(stamp: &str, size: u64, mtime: i64) -> Version {
        Version {
            snap: Snap {
                name: format!("@home.{stamp}"),
                stamp: stamp.into(),
            },
            src: PathBuf::new(),
            dir: false,
            size,
            mtime,
            same: 0,
        }
    }

    #[test]
    fn collapses_identical_runs() {
        let out = collapse(vec![
            v("20261107T1200", 10, 1),
            v("20261107T1400", 20, 2),
            v("20261107T1300", 10, 1),
            v("20261107T1100", 20, 2),
        ]);
        let got: Vec<_> = out.iter().map(|v| (v.snap.stamp.as_str(), v.same)).collect();
        assert_eq!(got, [("20261107T1400", 0), ("20261107T1300", 1), ("20261107T1100", 0)]);
    }

    #[test]
    fn resolves_missing_tail() {
        let tmp = std::env::temp_dir().canonicalize().unwrap();
        let p = resolve(&tmp.join("vx-does-not-exist/deeper")).unwrap();
        assert_eq!(p, tmp.join("vx-does-not-exist/deeper"));
    }
}
