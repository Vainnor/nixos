//! Opening a previous install read-only, and what can be carried over from it.

use super::super::plan::{Carry, OLD_MOUNT, Previous, Secret, Unlocked};
use super::runner::{Cmd, Runner};
use anyhow::{Result, bail};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Device-mapper name of the old install's LUKS while it's open read-only.
pub const OLD_MAPPER: &str = "vx-old";

pub fn persistent() -> PathBuf {
    Path::new(OLD_MOUNT).join("persistent")
}
pub fn home() -> PathBuf {
    Path::new(OLD_MOUNT).join("home")
}
/// The whole btrfs (subvolid=5): to see @swap.
pub fn top() -> PathBuf {
    Path::new(OLD_MOUNT).join("top")
}

/// Never carried: the one-time install files of the old install.
pub const NEVER: &str = "install";

/// Paths (relative to @persistent) that some item other than OtherState covers.
pub fn covered() -> Vec<&'static str> {
    Carry::ALL.iter().flat_map(|c| c.paths().iter().copied()).collect()
}

/// Entries of @persistent that no item covers (what OtherState means), as
/// relative paths: a directory that only *contains* covered paths (etc/,
/// var/lib/) is looked into rather than listed.
pub fn other_entries(root: &Path) -> Vec<String> {
    let cov = covered();
    let mut out = Vec::new();
    fn walk(root: &Path, rel: &str, cov: &[&str], out: &mut Vec<String>) {
        let dir = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        for n in names {
            let p = if rel.is_empty() { n.clone() } else { format!("{rel}/{n}") };
            if p == NEVER || cov.contains(&p.as_str()) {
                continue;
            }
            let prefix = format!("{p}/");
            let is_dir = std::fs::symlink_metadata(root.join(&p)).is_ok_and(|m| m.is_dir());
            if is_dir && cov.iter().any(|c| c.starts_with(&prefix)) {
                walk(root, &p, cov, out);
            } else {
                out.push(p);
            }
        }
    }
    walk(root, "", &cov, &mut out);
    out
}

/// What's in an (already mounted) @persistent.
pub fn available(root: &Path) -> BTreeSet<Carry> {
    let mut s: BTreeSet<Carry> =
        Carry::ALL.iter().copied().filter(|c| c.paths().iter().any(|p| root.join(p).exists())).collect();
    if !other_entries(root).is_empty() {
        s.insert(Carry::OtherState);
    }
    s
}

/// Users with a password hash file.
pub fn users(root: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root.join("passwords")) else { return vec![] };
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Size of the swap file in @swap, in GiB (rounded), 0 when there is none.
pub fn swap_gb(top: &Path) -> u32 {
    let Ok(rd) = std::fs::read_dir(top.join("@swap")) else { return 0 };
    let max = rd.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).max().unwrap_or(0);
    ((max + (1 << 29)) >> 30) as u32
}

pub fn open_cmd(prev: &Previous, password: &Secret) -> Cmd {
    Cmd::new("cryptsetup")
        .args(["open", "--readonly", "--key-file", "-", &prev.root_part, OLD_MAPPER])
        .stdin(password.0.clone().into_bytes())
}

pub fn unlock(prev: &Previous, password: Option<&Secret>, r: &Runner) -> Result<Unlocked> {
    close(r)?; // a second try after going back
    let dev = if prev.encrypted {
        let Some(pw) = password.filter(|p| !p.0.is_empty()) else { bail!("this install is encrypted: type its disk password") };
        r.run(open_cmd(prev, pw)).map_err(|e| {
            if format!("{e:#}").contains("No key available") {
                anyhow::anyhow!("wrong disk password: try again")
            } else {
                e
            }
        })?;
        format!("/dev/mapper/{OLD_MAPPER}")
    } else {
        prev.root_part.clone()
    };
    let res = (|| -> Result<Unlocked> {
        for (dir, opt) in [(persistent(), "subvol=@persistent"), (home(), "subvol=@home"), (top(), "subvolid=5")] {
            r.mkdir(&dir, 0o700)?;
            let ro = format!("ro,{opt}");
            if r.check(Cmd::new("mount").args(["-t", "btrfs", "-o", &ro, &dev]).arg(dir.to_string_lossy()), true)? {
                continue;
            }
            // A read-only device can't replay a dirty log; mount without replaying it.
            let ro = format!("ro,rescue=nologreplay,{opt}");
            r.run(Cmd::new("mount").args(["-t", "btrfs", "-o", &ro, &dev]).arg(dir.to_string_lossy()))?;
        }
        if r.dry() {
            return Ok(Unlocked {
                previous: prev.clone(),
                persistent: persistent(),
                home: home(),
                available: Carry::ALL.into_iter().collect(),
                home_bytes: 120 * 1_000_000_000,
                users: vec!["vainnor".into()],
                swap_gb: 0,
            });
        }
        let du = r.query(Cmd::new("du").args(["-s", "-b", "-x"]).arg(home().to_string_lossy()))?;
        let home_bytes = du.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
        Ok(Unlocked {
            previous: prev.clone(),
            persistent: persistent(),
            home: home(),
            available: available(&persistent()),
            home_bytes,
            users: users(&persistent()),
            swap_gb: swap_gb(&top()),
        })
    })();
    if res.is_err() {
        let _ = close(r);
    }
    res
}

/// Unmount and close; fine to call when nothing is open.
pub fn close(r: &Runner) -> Result<()> {
    for dir in [top(), home(), persistent()] {
        if r.dry() || super::disk::is_mounted(r, &dir) {
            r.run(Cmd::new("umount").arg(dir.to_string_lossy()))?;
        }
    }
    let mapper = format!("/dev/mapper/{OLD_MAPPER}");
    if r.exists(Path::new(&mapper), true) {
        r.run(Cmd::new("cryptsetup").args(["close", OLD_MAPPER]))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vx-prev-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    #[test]
    fn finds_items_and_other_state() {
        let d = tmp("items");
        touch(&d, "etc/ssh/ssh_host_ed25519_key");
        touch(&d, "passwords/vainnor");
        touch(&d, "var/lib/sbctl/keys/db/db.key");
        touch(&d, "install/tpm-enroll.key");
        let a = available(&d);
        assert_eq!(a, [Carry::HostKey, Carry::SecureBoot, Carry::Password].into_iter().collect());
        assert!(other_entries(&d).is_empty());

        touch(&d, "etc/machine-id");
        touch(&d, "var/lib/nixos/uid-map");
        touch(&d, "etc/lact/config.yaml");
        assert_eq!(other_entries(&d), ["etc/lact", "etc/machine-id", "var/lib/nixos"]);
        assert!(available(&d).contains(&Carry::OtherState));
        assert_eq!(users(&d), ["vainnor"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn swap_size() {
        let d = tmp("swap");
        assert_eq!(swap_gb(&d), 0);
        std::fs::create_dir_all(d.join("@swap")).unwrap();
        let f = std::fs::File::create(d.join("@swap/swapfile")).unwrap();
        f.set_len(16 << 30).unwrap(); // sparse
        assert_eq!(swap_gb(&d), 16);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn password_goes_through_stdin() {
        let prev = Previous { disk: "/dev/nvme0n1".into(), root_part: "/dev/nvme0n1p2".into(), encrypted: true };
        let c = open_cmd(&prev, &Secret("hunter2".into()));
        assert!(c.args.iter().all(|a| !a.contains("hunter2")));
        assert_eq!(c.stdin_bytes(), Some(&b"hunter2"[..]));
        assert_eq!(c.args, ["open", "--readonly", "--key-file", "-", "/dev/nvme0n1p2", "vx-old"]);
    }
}
