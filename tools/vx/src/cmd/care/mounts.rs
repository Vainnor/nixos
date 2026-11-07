//! /proc/self/mountinfo parsing, and the questions vx asks of it: which mount
//! holds a path, which mount points to stay out of, and which btrfs subvolume
//! (and path inside it) a file really lives in.

use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Path inside the filesystem that is mounted here (for btrfs: `/@home`,
    /// or `/@persistent/var/lib/bluetooth` for a preservation bind mount).
    pub root: PathBuf,
    /// Where it is mounted.
    pub mount_point: PathBuf,
    pub fstype: String,
    pub source: String,
}

/// Read and parse this process's mount table.
pub fn read() -> Result<Vec<Mount>> {
    let s =
        std::fs::read_to_string("/proc/self/mountinfo").context("could not read /proc/self/mountinfo (Linux only)")?;
    parse(&s)
}

/// Parse mountinfo text (see proc(5)):
/// `36 35 98:0 /root /mnt rw,noatime master:1 - btrfs /dev/sda rw`
pub fn parse(s: &str) -> Result<Vec<Mount>> {
    let mut out = Vec::new();
    for (n, line) in s.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(' ').collect();
        let dash = fields.iter().position(|f| *f == "-");
        let (Some(dash), true) = (dash, fields.len() >= 6) else {
            bail!("mountinfo line {} is malformed: {line}", n + 1);
        };
        if dash < 6 || fields.len() < dash + 3 {
            bail!("mountinfo line {} is malformed: {line}", n + 1);
        }
        out.push(Mount {
            root: PathBuf::from(unescape(fields[3])),
            mount_point: PathBuf::from(unescape(fields[4])),
            fstype: fields[dash + 1].to_string(),
            source: unescape(fields[dash + 2]),
        });
    }
    Ok(out)
}

/// mountinfo escapes space, tab, newline and backslash as `\040` style octal.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() {
            let oct = &b[i + 1..i + 4];
            if oct.iter().all(|c| (b'0'..=b'7').contains(c))
                && let Ok(v) = u8::from_str_radix(std::str::from_utf8(oct).unwrap_or("x"), 8)
            {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The mount a path is on: the one with the longest matching mount point.
/// Later entries win on ties (a later mount hides an earlier one).
pub fn containing<'a>(mounts: &'a [Mount], path: &Path) -> Option<&'a Mount> {
    let mut best: Option<&Mount> = None;
    for m in mounts {
        if path.starts_with(&m.mount_point) {
            let deeper = best.is_none_or(|b| m.mount_point.components().count() >= b.mount_point.components().count());
            if deeper {
                best = Some(m);
            }
        }
    }
    best
}

/// Where a path lives on btrfs: (subvolume, path inside that subvolume).
/// `/home/vainnor/x` on a mount of `/@home` at `/home` gives `("@home", "vainnor/x")`.
pub fn btrfs_location(mounts: &[Mount], path: &Path) -> Result<(String, PathBuf)> {
    let m = containing(mounts, path).context("path is not on any mount")?;
    if m.fstype != "btrfs" {
        bail!(
            "{} is on {} ({}), not the btrfs disk, so it has no snapshots",
            path.display(),
            m.mount_point.display(),
            m.fstype
        );
    }
    let rest = path.strip_prefix(&m.mount_point).unwrap_or(Path::new(""));
    let mut parts = m.root.components().filter(|c| matches!(c, Component::Normal(_)));
    let Some(subvol) = parts.next() else {
        bail!(
            "{} is on the top of the btrfs filesystem, not inside a subvolume",
            path.display()
        );
    };
    let subvol = subvol.as_os_str().to_string_lossy().into_owned();
    let mut inner: PathBuf = parts.collect();
    inner.push(rest);
    Ok((subvol, inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
22 1 0:21 / / rw,relatime shared:1 - tmpfs none rw,size=16G,mode=755
23 22 0:5 / /dev rw,nosuid shared:2 - devtmpfs devtmpfs rw
30 22 0:30 /@nix /nix rw,noatime shared:5 - btrfs /dev/mapper/cryptroot rw,compress=zstd:3,subvolid=256,subvol=/@nix
31 22 0:30 /@persistent /persistent rw,noatime shared:6 - btrfs /dev/mapper/cryptroot rw,subvol=/@persistent
32 22 0:30 /@home /home rw,noatime shared:7 - btrfs /dev/mapper/cryptroot rw,subvol=/@home
40 22 0:30 /@persistent/var/lib/bluetooth /var/lib/bluetooth rw,noatime shared:6 - btrfs /dev/mapper/cryptroot rw
41 22 0:30 /@persistent/etc/machine-id /etc/machine-id rw,noatime shared:6 - btrfs /dev/mapper/cryptroot rw
42 22 0:30 / /.btrfs rw,noatime shared:9 - btrfs /dev/mapper/cryptroot rw,subvolid=5
50 32 0:44 / /home/vainnor/My\\040Drive rw - fuse.rclone remote: rw
";

    #[test]
    fn parses_fields() {
        let m = parse(SAMPLE).unwrap();
        assert_eq!(m.len(), 9);
        assert_eq!(m[0].mount_point, PathBuf::from("/"));
        assert_eq!(m[0].fstype, "tmpfs");
        assert_eq!(m[3].root, PathBuf::from("/@persistent"));
        assert_eq!(m[5].mount_point, PathBuf::from("/var/lib/bluetooth"));
        assert_eq!(m[5].source, "/dev/mapper/cryptroot");
    }

    #[test]
    fn optional_fields_are_skipped() {
        let line = "36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 propagate_from:2 - ext3 /dev/root rw,errors=continue";
        let m = parse(line).unwrap();
        assert_eq!(m[0].fstype, "ext3");
        assert_eq!(m[0].source, "/dev/root");
        let none = "36 35 98:0 / /x rw - proc proc rw";
        assert_eq!(parse(none).unwrap()[0].fstype, "proc");
    }

    #[test]
    fn unescapes_octal() {
        let m = parse(SAMPLE).unwrap();
        assert_eq!(m[8].mount_point, PathBuf::from("/home/vainnor/My Drive"));
        assert_eq!(unescape(r"a\134b\011c"), "a\\b\tc");
        assert_eq!(unescape(r"trailing\04"), r"trailing\04");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("not a mount line").is_err());
    }

    #[test]
    fn longest_prefix_wins() {
        let m = parse(SAMPLE).unwrap();
        let at = |p: &str| containing(&m, Path::new(p)).unwrap().mount_point.clone();
        assert_eq!(at("/etc/hosts"), PathBuf::from("/"));
        assert_eq!(at("/var/lib/bluetooth/x"), PathBuf::from("/var/lib/bluetooth"));
        assert_eq!(at("/var/lib/bluetoothx"), PathBuf::from("/"));
        assert_eq!(at("/home/vainnor/a"), PathBuf::from("/home"));
    }

    #[test]
    fn btrfs_locations() {
        let m = parse(SAMPLE).unwrap();
        let loc = |p: &str| btrfs_location(&m, Path::new(p));
        assert_eq!(
            loc("/home/vainnor/Documents").unwrap(),
            ("@home".into(), PathBuf::from("vainnor/Documents"))
        );
        assert_eq!(
            loc("/var/lib/bluetooth/AA").unwrap(),
            ("@persistent".into(), PathBuf::from("var/lib/bluetooth/AA"))
        );
        assert_eq!(
            loc("/etc/machine-id").unwrap(),
            ("@persistent".into(), PathBuf::from("etc/machine-id"))
        );
        assert_eq!(loc("/persistent").unwrap(), ("@persistent".into(), PathBuf::new()));
        assert!(loc("/etc/hosts").is_err()); // tmpfs
        assert!(loc("/.btrfs/x").is_err()); // top level, no subvolume
    }
}
