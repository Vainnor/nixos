//! `vx persist-check`: what on the RAM-backed `/` will be gone after a reboot.
//!
//! `/` is a tmpfs (hosts/arc/_disko.nix). What survives is /nix, /home,
//! /var/log, /persistent and whatever `preservation.preserveAt."/persistent"`
//! bind-mounts back in (system/impermanence.nix and the modules that add to
//! it). All of those are separate mounts in /proc/self/mountinfo, so the walk
//! simply stays on the root tmpfs and skips every other mount point.

use super::mounts::{self, Mount};
use crate::ctx::Ctx;
use crate::ui;
use anyhow::{Result, bail};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(clap::Args)]
pub struct Args {
    /// Also show noise: caches, files NixOS writes at every boot, and declared symlinks
    #[arg(long)]
    pub all: bool,
    /// Print JSON instead of a report
    #[arg(long)]
    pub json: bool,
    /// Also look in root-only directories (uses sudo)
    #[arg(long)]
    pub system: bool,
}

/// Never walked, whatever they are mounted as.
const HARD_SKIP: &[&str] = &[
    "/nix",
    "/proc",
    "/sys",
    "/dev",
    "/run",
    "/tmp",
    "/persistent",
    "/boot",
    "/.btrfs",
];

/// System paths that are rebuilt every boot or are fine to lose.
const SYSTEM_NOISE: &[&str] = &[
    "/etc/static",
    "/etc/.clean",
    "/etc/.updated",
    "/etc/.pwd.lock",
    "/etc/NIXOS",
    "/etc/passwd",
    "/etc/group",
    "/etc/shadow",
    "/etc/gshadow",
    "/etc/subuid",
    "/etc/subgid",
    "/etc/sudoers",
    "/etc/mtab",
    "/etc/resolv.conf",
    "/etc/ld.so.cache",
    "/etc/machine-id",
    "/etc/adjtime",
    "/etc/udev/hwdb.bin",
    "/var/.updated",
    "/var/empty",
    "/var/cache",
    "/var/tmp",
    "/var/db/sudo",
    "/var/lib/systemd",
    "/var/lib/nixos",
    "/var/lib/lastlog",
    "/var/lib/logrotate.status",
    "/var/lib/udisks2",
    "/var/lib/upower",
    "/var/lib/geoclue",
    "/var/lib/dhcpcd",
    "/root/.cache",
    "/root/.nix-defexpr",
    "/root/.nix-profile",
    "/root/.local/state/nix",
    "/root/.local/state/home-manager",
    "/root/.sudo_as_admin_successful",
    "/lost+found",
];

/// Home paths (relative to $HOME) that are fine to lose or rebuilt by home-manager.
const HOME_NOISE: &[&str] = &[
    ".cache",
    ".nix-defexpr",
    ".nix-profile",
    ".local/state/nix",
    ".local/state/home-manager",
    ".local/share/Trash",
    ".sudo_as_admin_successful",
    ".Xauthority",
];

/// Directories whose children are reported one group each.
const SYSTEM_CONTAINERS: &[&str] = &[
    "/etc",
    "/var",
    "/var/lib",
    "/var/cache",
    "/var/db",
    "/var/spool",
    "/usr",
    "/usr/local",
    "/usr/local/share",
    "/opt",
    "/srv",
    "/root",
    "/root/.config",
    "/root/.local/share",
    "/root/.local/state",
];
const HOME_CONTAINERS: &[&str] = &["", ".config", ".local/share", ".local/state", ".var/app"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Link,
    Other,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Kind,
    pub link: Option<PathBuf>,
    pub size: u64,
    pub mtime: i64,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

/// What the scan knows about the machine.
pub struct Rules {
    /// $HOME, when it is on the root tmpfs (then it is walked too).
    pub home: Option<PathBuf>,
    /// Every mount point except `/`.
    pub mount_points: HashSet<PathBuf>,
    /// Files NixOS copied into /etc (listed in /etc/.clean).
    pub etc_clean: HashSet<PathBuf>,
}

impl Rules {
    /// Don't walk into this at all.
    pub fn skip(&self, p: &Path) -> bool {
        p != Path::new("/") && (self.mount_points.contains(p) || HARD_SKIP.iter().any(|s| p.starts_with(s)))
    }

    /// Inside (or at) a mount point other than `/`.
    pub fn under_mount(&self, p: &Path) -> bool {
        p.ancestors().any(|a| self.skip(a))
    }

    /// Fine to lose, or put there by the config itself.
    pub fn noise(&self, e: &Entry) -> bool {
        if e.kind == Kind::Other || self.etc_clean.contains(&e.path) {
            return true;
        }
        if e.kind == Kind::Link && e.link.as_deref().is_some_and(declared_link) {
            return true;
        }
        if let Some(rel) = self.home.as_deref().and_then(|h| e.path.strip_prefix(h).ok()) {
            return HOME_NOISE.iter().any(|n| rel.starts_with(n));
        }
        SYSTEM_NOISE.iter().any(|n| e.path.starts_with(n))
    }

    pub fn is_user(&self, p: &Path) -> bool {
        self.home.as_deref().is_some_and(|h| p.starts_with(h))
    }

    /// The directory (or file) a path is reported under, e.g. `/var/lib/foo`
    /// for `/var/lib/foo/db/x`.
    pub fn group_of(&self, p: &Path) -> PathBuf {
        let (base, containers): (PathBuf, Vec<PathBuf>) = match self.home.as_deref().filter(|h| p.starts_with(h)) {
            Some(h) => (h.to_path_buf(), HOME_CONTAINERS.iter().map(|c| h.join(c)).collect()),
            None => (
                PathBuf::from("/"),
                SYSTEM_CONTAINERS.iter().map(PathBuf::from).collect(),
            ),
        };
        let container = containers
            .iter()
            .filter(|c| p.starts_with(c) && p != c.as_path())
            .max_by_key(|c| c.components().count())
            .cloned()
            .unwrap_or(base);
        match p.strip_prefix(&container).ok().and_then(|r| r.components().next()) {
            Some(first) => container.join(first),
            None => p.to_path_buf(),
        }
    }
}

/// A symlink into the store or another persistent place was declared somewhere.
pub fn declared_link(target: &Path) -> bool {
    ["/nix/store", "/etc/static", "/run", "/persistent", "/home"]
        .iter()
        .any(|p| target.starts_with(p))
}

/// One reported directory or file and the lost files under it.
pub struct Group {
    pub path: PathBuf,
    pub user: bool,
    /// Persist the whole directory (nothing declared or mounted inside it).
    pub as_dir: bool,
    pub meta: Option<Entry>,
    pub files: Vec<Entry>,
    pub noise: bool,
}

impl Group {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
    pub fn newest(&self) -> i64 {
        self.files.iter().map(|f| f.mtime).max().unwrap_or(0)
    }
}

/// Sort entries into groups, newest group first. Directories only supply
/// ownership; groups that hold declared or mounted things get file-by-file
/// suggestions so the snippet never shadows them.
pub fn group(rules: &Rules, entries: &[Entry], all: bool) -> Vec<Group> {
    let dirs: HashMap<&Path, &Entry> = entries
        .iter()
        .filter(|e| e.kind == Kind::Dir)
        .map(|e| (e.path.as_path(), e))
        .collect();
    let mut tainted: HashSet<PathBuf> = HashSet::new();
    let mut map: BTreeMap<(bool, PathBuf), Vec<Entry>> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.kind != Kind::Dir) {
        let g = rules.group_of(&e.path);
        let noise = rules.noise(e);
        if noise {
            tainted.insert(g.clone());
            if !all {
                continue;
            }
        }
        map.entry((noise, g)).or_default().push(e.clone());
    }
    for mp in &rules.mount_points {
        if mp != Path::new("/") {
            tainted.insert(rules.group_of(mp));
        }
    }
    let mut out: Vec<Group> = map
        .into_iter()
        .map(|((noise, path), mut files)| {
            files.sort_by_key(|f| std::cmp::Reverse(f.mtime));
            let meta = dirs.get(path.as_path()).map(|e| (*e).clone());
            Group {
                user: rules.is_user(&path),
                as_dir: meta.is_some() && !tainted.contains(&path),
                meta,
                files,
                noise,
                path,
            }
        })
        .collect();
    out.sort_by(|a, b| a.noise.cmp(&b.noise).then(b.newest().cmp(&a.newest())));
    out
}

/// Names for uids and gids, from /etc/passwd and /etc/group.
pub struct Names {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

impl Names {
    pub fn load() -> Self {
        Self {
            users: parse_ids(&std::fs::read_to_string("/etc/passwd").unwrap_or_default()),
            groups: parse_ids(&std::fs::read_to_string("/etc/group").unwrap_or_default()),
        }
    }
    fn user(&self, id: u32) -> String {
        self.users.get(&id).cloned().unwrap_or_else(|| id.to_string())
    }
    fn group(&self, id: u32) -> String {
        self.groups.get(&id).cloned().unwrap_or_else(|| id.to_string())
    }
}

/// `name:x:id:...` lines to id -> name.
pub fn parse_ids(s: &str) -> HashMap<u32, String> {
    s.lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            let id = f.nth(1)?.parse().ok()?;
            Some((id, name.to_string()))
        })
        .collect()
}

/// The Nix list item for one path, in the style the repo uses.
pub fn nix_item(path: &Path, as_dir: bool, user_rel: Option<&Path>, meta: Option<&Entry>, names: &Names) -> String {
    let shown = user_rel.unwrap_or(path).display().to_string();
    let key = if as_dir { "directory" } else { "file" };
    let mut attrs = Vec::new();
    if let Some(m) = meta {
        // users.<name> entries already default to that user; system ones to root.
        if user_rel.is_none() && (m.uid != 0 || m.gid != 0) {
            attrs.push(format!("user = \"{}\";", names.user(m.uid)));
            attrs.push(format!("group = \"{}\";", names.group(m.gid)));
        }
        let mode = m.mode & 0o7777;
        let default = if as_dir { 0o755 } else { 0o644 };
        if mode != default {
            attrs.push(format!("mode = \"{mode:04o}\";"));
        }
    }
    if attrs.is_empty() {
        format!("\"{shown}\"")
    } else {
        format!("{{ {key} = \"{shown}\"; {} }}", attrs.join(" "))
    }
}

/// The full snippet: system entries, then the user's.
pub fn snippet(groups: &[Group], home: Option<&Path>, user: &str, names: &Names) -> String {
    let mut sys_dirs = Vec::new();
    let mut sys_files = Vec::new();
    let mut usr_dirs = Vec::new();
    let mut usr_files = Vec::new();
    for g in groups.iter().filter(|g| !g.noise) {
        let rel = |p: &Path| {
            home.filter(|_| g.user)
                .and_then(|h| p.strip_prefix(h).ok())
                .map(Path::to_path_buf)
        };
        let (dirs, files) = if g.user {
            (&mut usr_dirs, &mut usr_files)
        } else {
            (&mut sys_dirs, &mut sys_files)
        };
        if g.as_dir {
            dirs.push(nix_item(&g.path, true, rel(&g.path).as_deref(), g.meta.as_ref(), names));
        } else {
            for f in &g.files {
                files.push(nix_item(&f.path, false, rel(&f.path).as_deref(), Some(f), names));
            }
        }
    }
    let list = |name: &str, items: &[String], indent: &str| -> String {
        if items.is_empty() {
            return String::new();
        }
        let body: String = items.iter().map(|i| format!("{indent}  {i}\n")).collect();
        format!("{indent}{name} = [\n{body}{indent}];\n")
    };
    let mut s = String::from("preservation.preserveAt.\"/persistent\" = {\n");
    s += &list("directories", &sys_dirs, "  ");
    s += &list("files", &sys_files, "  ");
    if !usr_dirs.is_empty() || !usr_files.is_empty() {
        s += &format!("  users.{user} = {{\n");
        s += &list("directories", &usr_dirs, "    ");
        s += &list("files", &usr_files, "    ");
        s += "  };\n";
    }
    s += "};\n";
    s
}

pub fn human(n: u64) -> String {
    let units = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n}B")
    } else {
        format!("{v:.1}{}", units[u])
    }
}

pub fn ago(now: i64, t: i64) -> String {
    let d = (now - t).max(0);
    match d {
        0..60 => format!("{d}s ago"),
        60..3600 => format!("{}m ago", d / 60),
        3600..86400 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86400),
    }
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn entry_from(path: &Path, md: &std::fs::Metadata) -> Entry {
    let ft = md.file_type();
    let kind = if ft.is_symlink() {
        Kind::Link
    } else if ft.is_dir() {
        Kind::Dir
    } else if ft.is_file() {
        Kind::File
    } else {
        Kind::Other
    };
    Entry {
        path: path.to_path_buf(),
        kind,
        link: (kind == Kind::Link).then(|| std::fs::read_link(path).ok()).flatten(),
        size: if kind == Kind::File { md.len() } else { 0 },
        mtime: md.mtime(),
        uid: md.uid(),
        gid: md.gid(),
        mode: md.mode(),
    }
}

/// Walk the root tmpfs as this user. Returns the entries and the directories
/// that couldn't be read.
fn walk(rules: &Rules) -> (Vec<Entry>, Vec<PathBuf>) {
    let mut entries = Vec::new();
    let mut denied = Vec::new();
    let it = walkdir::WalkDir::new("/")
        .follow_links(false)
        .same_file_system(true)
        .into_iter();
    for item in it.filter_entry(|e| !rules.skip(e.path())) {
        match item {
            Ok(e) => {
                if e.path() == Path::new("/") {
                    continue;
                }
                if let Ok(md) = e.path().symlink_metadata() {
                    entries.push(entry_from(e.path(), &md));
                }
            }
            Err(err) => {
                if let Some(p) = err.path() {
                    denied.push(p.to_path_buf());
                }
            }
        }
    }
    (entries, denied)
}

/// `find -printf` output: `type\tsize\tmtime\tuid\tgid\tmode\tlink\tpath\0`.
pub fn parse_find(out: &str) -> Vec<Entry> {
    out.split('\0')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.splitn(8, '\t').collect();
            if f.len() != 8 {
                return None;
            }
            let kind = match f[0] {
                "f" => Kind::File,
                "d" => Kind::Dir,
                "l" => Kind::Link,
                _ => Kind::Other,
            };
            Some(Entry {
                path: PathBuf::from(f[7]),
                kind,
                link: (kind == Kind::Link).then(|| PathBuf::from(f[6])),
                size: if kind == Kind::File {
                    f[1].parse().unwrap_or(0)
                } else {
                    0
                },
                mtime: f[2].split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0),
                uid: f[3].parse().unwrap_or(0),
                gid: f[4].parse().unwrap_or(0),
                mode: u32::from_str_radix(f[5], 8).unwrap_or(0o755),
            })
        })
        .collect()
}

/// The same walk, as root, for the directories the user couldn't read.
fn walk_as_root(ctx: &Ctx, dirs: &[PathBuf]) -> Result<Vec<Entry>> {
    if dirs.is_empty() {
        return Ok(Vec::new());
    }
    let mut c = ctx.sudo("find");
    c.args(dirs)
        .args(["-xdev", "-printf", "%y\\t%s\\t%T@\\t%U\\t%G\\t%m\\t%l\\t%p\\0"]);
    c.stderr(std::process::Stdio::null());
    if ctx.verbose {
        ui::dim(&format!(
            "$ sudo find {} -xdev -printf ...",
            dirs.iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    let out = c.output()?;
    Ok(parse_find(&String::from_utf8_lossy(&out.stdout)))
}

fn read_etc_clean() -> HashSet<PathBuf> {
    std::fs::read_to_string("/etc/.clean")
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            if l.starts_with('/') {
                PathBuf::from(l)
            } else {
                Path::new("/etc").join(l)
            }
        })
        .collect()
}

pub fn rules_from(mounts: &[Mount], home: &Path, etc_clean: HashSet<PathBuf>) -> Rules {
    let root = mounts.iter().rev().find(|m| m.mount_point == Path::new("/"));
    let home_on_root = mounts::containing(mounts, home).map(|m| m.mount_point.as_path()) == Some(Path::new("/"));
    Rules {
        home: home_on_root.then(|| home.to_path_buf()),
        mount_points: mounts
            .iter()
            .filter(|m| m.mount_point != Path::new("/"))
            .map(|m| m.mount_point.clone())
            .collect(),
        etc_clean: if root.is_some() { etc_clean } else { HashSet::new() },
    }
}

pub fn run(ctx: &Ctx, a: Args) -> Result<()> {
    ctx.need(crate::ctx::Host::Arc)?;
    let mounts = mounts::read()?;
    let root = mounts.iter().rev().find(|m| m.mount_point == Path::new("/"));
    if root.is_none_or(|m| m.fstype != "tmpfs") {
        bail!(
            "/ is not a tmpfs here ({}), so nothing is wiped at boot",
            root.map_or("unknown", |m| m.fstype.as_str())
        );
    }
    let home = PathBuf::from(std::env::var("HOME")?);
    let user = std::env::var("USER").unwrap_or_else(|_| "vainnor".into());
    let rules = rules_from(&mounts, &home, read_etc_clean());

    let (mut entries, denied) = walk(&rules);
    // walkdir reports the unreadable directory itself; drop ones under mounts.
    let denied: Vec<PathBuf> = denied.into_iter().filter(|d| !rules.under_mount(d)).collect();
    if a.system {
        let extra = walk_as_root(ctx, &denied)?;
        entries.extend(extra.into_iter().filter(|e| !rules.under_mount(&e.path)));
    }
    let groups = group(&rules, &entries, a.all);
    let names = Names::load();
    let now = now();

    if a.json {
        let gs: Vec<_> = groups
            .iter()
            .map(|g| {
                serde_json::json!({
                    "path": g.path,
                    "scope": if g.user { "user" } else { "system" },
                    "kind": if g.as_dir { "directory" } else { "files" },
                    "noise": g.noise,
                    "size": g.size(),
                    "newest": g.newest(),
                    "files": g.files.iter().map(|f| serde_json::json!({"path": f.path, "size": f.size, "mtime": f.mtime, "symlink": f.link})).collect::<Vec<_>>(),
                })
            })
            .collect();
        let v = serde_json::json!({
            "home_on_tmpfs": rules.home.is_some(),
            "groups": gs,
            "unreadable": if a.system { Vec::new() } else { denied.clone() },
            "nix": snippet(&groups, rules.home.as_deref(), &user, &names),
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    if rules.home.is_none() {
        ui::dim(&format!(
            "{} is on its own subvolume, so it survives reboots and isn't checked",
            home.display()
        ));
    }
    if groups.is_empty() {
        ui::ok("nothing on / will be lost at reboot");
    } else {
        ui::heading("lost at reboot");
    }
    for g in &groups {
        let tag = match (g.noise, g.user) {
            (true, _) => " (noise)",
            (false, true) => " (user)",
            _ => "",
        };
        println!(
            "  {}{}  {} files  {}  newest {}",
            g.path.display(),
            tag,
            g.files.len(),
            human(g.size()),
            ago(now, g.newest())
        );
        for f in g.files.iter().take(5) {
            let shown = f
                .path
                .strip_prefix(&g.path)
                .ok()
                .filter(|r| !r.as_os_str().is_empty())
                .map_or(f.path.clone(), |r| r.to_path_buf());
            ui::dim(&format!(
                "      {}  {}  {}",
                shown.display(),
                human(f.size),
                ago(now, f.mtime)
            ));
        }
        if g.files.len() > 5 {
            ui::dim(&format!("      +{} more", g.files.len() - 5));
        }
    }
    if groups.iter().any(|g| !g.noise) {
        println!();
        ui::heading("to keep them, add what you want to a nixos module");
        print!("{}", snippet(&groups, rules.home.as_deref(), &user, &names));
        ui::dim("(system/impermanence.nix, or next to the service in its own module, like fans.nix does)");
    }
    if !a.system && !denied.is_empty() {
        ui::info(&format!(
            "{} directories need root to read ({}); add --system to look in them",
            denied.len(),
            denied
                .iter()
                .take(3)
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(home_on_tmpfs: bool) -> Rules {
        let mi = format!(
            "1 0 0:1 / / rw - tmpfs none rw\n\
             2 1 0:2 /@nix /nix rw - btrfs /dev/mapper/cryptroot rw\n\
             3 1 0:2 /@persistent/var/lib/bluetooth /var/lib/bluetooth rw - btrfs /dev/mapper/cryptroot rw\n\
             4 1 0:2 /@persistent/etc/NetworkManager/system-connections /etc/NetworkManager/system-connections rw - btrfs /dev/mapper/cryptroot rw\n\
             5 1 0:3 / /proc rw - proc proc rw\n{}",
            if home_on_tmpfs {
                ""
            } else {
                "6 1 0:2 /@home /home rw - btrfs /dev/mapper/cryptroot rw\n"
            }
        );
        let m = mounts::parse(&mi).unwrap();
        rules_from(
            &m,
            Path::new("/home/vainnor"),
            HashSet::from([PathBuf::from("/etc/hosts")]),
        )
    }

    fn file(p: &str, mtime: i64) -> Entry {
        Entry {
            path: p.into(),
            kind: Kind::File,
            link: None,
            size: 10,
            mtime,
            uid: 0,
            gid: 0,
            mode: 0o100644,
        }
    }
    fn dir(p: &str, uid: u32, mode: u32) -> Entry {
        Entry {
            path: p.into(),
            kind: Kind::Dir,
            link: None,
            size: 0,
            mtime: 0,
            uid,
            gid: uid,
            mode: 0o40000 | mode,
        }
    }
    fn link(p: &str, to: &str) -> Entry {
        Entry {
            path: p.into(),
            kind: Kind::Link,
            link: Some(to.into()),
            size: 0,
            mtime: 0,
            uid: 0,
            gid: 0,
            mode: 0o120777,
        }
    }

    #[test]
    fn skips_mounts_and_hard_paths() {
        let r = rules(false);
        assert!(!r.skip(Path::new("/")));
        assert!(r.skip(Path::new("/nix")));
        assert!(r.skip(Path::new("/var/lib/bluetooth")));
        assert!(r.skip(Path::new("/home")));
        assert!(r.skip(Path::new("/tmp/x")));
        assert!(!r.skip(Path::new("/var/lib/bluetoothx")));
        assert!(!r.skip(Path::new("/etc/NetworkManager")));
        assert!(r.under_mount(Path::new("/var/lib/bluetooth/AA/info")));
        assert!(!r.under_mount(Path::new("/var/lib/foo")));
    }

    #[test]
    fn home_only_walked_when_on_tmpfs() {
        assert!(rules(false).home.is_none());
        assert_eq!(rules(true).home, Some(PathBuf::from("/home/vainnor")));
        assert!(!rules(true).skip(Path::new("/home")));
    }

    #[test]
    fn noise() {
        let r = rules(true);
        assert!(r.noise(&file("/etc/passwd", 0)));
        assert!(r.noise(&file("/etc/hosts", 0))); // from /etc/.clean
        assert!(r.noise(&file("/var/cache/man/index.db", 0)));
        assert!(r.noise(&file("/var/lib/systemd/random-seed", 0)));
        assert!(r.noise(&file("/home/vainnor/.cache/x", 0)));
        assert!(r.noise(&link("/etc/zshrc", "/etc/static/zshrc")));
        assert!(r.noise(&link("/home/vainnor/.bashrc", "/nix/store/abc-hm/.bashrc")));
        assert!(!r.noise(&link("/etc/foo", "bar")));
        assert!(!r.noise(&file("/var/lib/foo/db", 0)));
        assert!(!r.noise(&file("/home/vainnor/.config/foo/x", 0)));
        assert!(!r.noise(&file("/etc/cachefoo", 0)));
    }

    #[test]
    fn groups_paths() {
        let r = rules(true);
        let g = |p: &str| r.group_of(Path::new(p));
        assert_eq!(g("/var/lib/foo/db/x"), PathBuf::from("/var/lib/foo"));
        assert_eq!(g("/var/lib/foo"), PathBuf::from("/var/lib/foo"));
        assert_eq!(g("/etc/NetworkManager/conf.d/x"), PathBuf::from("/etc/NetworkManager"));
        assert_eq!(g("/srv/www/a"), PathBuf::from("/srv/www"));
        assert_eq!(g("/weird/x"), PathBuf::from("/weird"));
        assert_eq!(g("/root/.config/foo/bar"), PathBuf::from("/root/.config/foo"));
        assert_eq!(
            g("/home/vainnor/.config/foo/bar"),
            PathBuf::from("/home/vainnor/.config/foo")
        );
        assert_eq!(g("/home/vainnor/notes.txt"), PathBuf::from("/home/vainnor/notes.txt"));
        assert_eq!(g("/home/vainnor/proj/a/b"), PathBuf::from("/home/vainnor/proj"));
    }

    #[test]
    fn groups_and_snippet() {
        let r = rules(true);
        let entries = vec![
            dir("/var/lib/foo", 990, 0o700),
            file("/var/lib/foo/state", 100),
            file("/var/lib/foo/sub/x", 300),
            dir("/etc/NetworkManager", 0, 0o755),
            file("/etc/NetworkManager/NetworkManager-intern.conf", 200),
            file("/etc/passwd", 999),
            dir("/home/vainnor/.config/app", 1000, 0o755),
            file("/home/vainnor/.config/app/s.json", 50),
        ];
        let gs = group(&r, &entries, false);
        let paths: Vec<_> = gs.iter().map(|g| g.path.display().to_string()).collect();
        assert_eq!(
            paths,
            ["/var/lib/foo", "/etc/NetworkManager", "/home/vainnor/.config/app"]
        );
        assert!(gs[0].as_dir);
        assert_eq!(gs[0].files[0].path, PathBuf::from("/var/lib/foo/sub/x")); // newest first
        assert!(!gs[1].as_dir, "has a preserved mount inside, so files only");
        assert!(gs[2].user && gs[2].as_dir);

        let names = Names {
            users: HashMap::from([(990, "foo".into())]),
            groups: HashMap::from([(990, "foo".into())]),
        };
        let s = snippet(&gs, r.home.as_deref(), "vainnor", &names);
        assert!(
            s.contains(r#"{ directory = "/var/lib/foo"; user = "foo"; group = "foo"; mode = "0700"; }"#),
            "{s}"
        );
        assert!(s.contains(r#""/etc/NetworkManager/NetworkManager-intern.conf""#), "{s}");
        assert!(s.contains("users.vainnor = {"), "{s}");
        assert!(
            s.contains(
                r#"directories = [
      ".config/app""#
            ),
            "{s}"
        );
        assert!(!s.contains("passwd"));

        let all = group(&r, &entries, true);
        assert!(all.last().unwrap().noise);
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn parses_find_output() {
        let out = "f\t12\t1700000000.5\t0\t0\t644\t\t/root/a b\0l\t0\t1\t0\t0\t777\t/nix/store/x\t/root/l\0d\t4096\t1\t0\t0\t700\t\t/root\0junk\0";
        let e = parse_find(out);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].path, PathBuf::from("/root/a b"));
        assert_eq!((e[0].size, e[0].mtime, e[0].mode), (12, 1_700_000_000, 0o644));
        assert_eq!(e[1].link, Some(PathBuf::from("/nix/store/x")));
        assert_eq!(e[2].kind, Kind::Dir);
    }

    #[test]
    fn ids_and_formatting() {
        let ids = parse_ids("root:x:0:0::/root:/bin/sh\nvainnor:x:1000:100::/home/vainnor:/bin/fish\nbad\n");
        assert_eq!(ids.get(&1000).map(String::as_str), Some("vainnor"));
        assert_eq!(human(512), "512B");
        assert_eq!(human(1536), "1.5K");
        assert_eq!(ago(1000, 1000 - 7200), "2h ago");
    }
}
