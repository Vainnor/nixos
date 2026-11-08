//! Reads this machine for the installer: disks (lsblk), previous installs,
//! firmware state, and where /home could be backed up. Owned by the UI side.
//!
//! Everything that parses is a pure function over text (tested with a fixture
//! resembling the real PC); the thin wrappers around them read /sys, /proc,
//! /dev/disk/by-id and run lsblk. It compiles anywhere; off Linux the
//! wrappers simply find nothing.

use super::plan::{DataMode, DiskInfo, DiskKind, Machine, PartInfo, Plan, Previous};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const LSBLK_COLUMNS: &str = "NAME,PATH,TYPE,SIZE,MODEL,SERIAL,TRAN,RM,FSTYPE,LABEL,PARTLABEL,UUID,PARTTYPENAME,MOUNTPOINTS";
const EFIVARS: &str = "/sys/firmware/efi/efivars";
const EFI_GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// Where an unmounted partition is mounted read-only to measure its free space.
pub const PROBE_MOUNT: &str = "/run/vx-install/probe";
/// Filesystems /home can be backed up to (they keep owners and permissions,
/// or at least the files, and mount read-write reliably).
pub const BACKUP_FS: [&str; 4] = ["ext4", "btrfs", "xfs", "exfat"];
/// Filesystems a data drive can be mounted with as it is.
pub const MOUNTABLE_FS: [&str; 5] = ["ntfs", "ext4", "btrfs", "exfat", "xfs"];

// ---------------------------------------------------------------- disks

/// All whole disks, with what's on them.
pub fn disks() -> Result<Vec<DiskInfo>> {
    let out = Command::new("lsblk")
        .args(["-J", "-b", "-o", LSBLK_COLUMNS])
        .output()
        .context("couldn't run lsblk")?;
    if !out.status.success() {
        bail!("lsblk failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    parse_lsblk(&String::from_utf8_lossy(&out.stdout), &by_id_map())
}

/// Kernel path (/dev/nvme0n1) -> its stable /dev/disk/by-id path.
fn by_id_map() -> BTreeMap<String, String> {
    let dir = Path::new("/dev/disk/by-id");
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let Ok(target) = std::fs::canonicalize(e.path()) else { continue };
            names.entry(target.to_string_lossy().into_owned()).or_default().push(e.file_name().to_string_lossy().into_owned());
        }
    }
    names.into_iter().filter_map(|(dev, n)| pick_by_id(&n).map(|n| (dev, format!("/dev/disk/by-id/{n}")))).collect()
}

/// The best by-id name for one disk: `nvme-<model>_<serial>`, `ata-...` or
/// `usb-...`; never a partition, an eui/wwn alias or a `_1` namespace duplicate.
pub fn pick_by_id(names: &[String]) -> Option<String> {
    let ok: Vec<&String> = names
        .iter()
        .filter(|n| {
            !n.contains("-part")
                && !n.starts_with("nvme-eui.")
                && !n.starts_with("nvme-nvme.")
                && !n.starts_with("wwn-")
                && !n.ends_with("_1")
        })
        .collect();
    for prefix in ["nvme-", "ata-", "usb-"] {
        if let Some(n) = ok.iter().filter(|n| n.starts_with(prefix)).min() {
            return Some((*n).clone());
        }
    }
    ok.into_iter().min().cloned()
}

fn s(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn n(v: &Value, k: &str) -> u64 {
    match v.get(k) {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn b(v: &Value, k: &str) -> bool {
    match v.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.trim() == "1" || s.trim() == "true",
        Some(Value::Number(n)) => n.as_u64() == Some(1),
        _ => false,
    }
}

/// MOUNTPOINTS (util-linux >= 2.37) or MOUNTPOINT, without the nulls.
fn mounts(v: &Value) -> Vec<String> {
    match v.get("mountpoints").or_else(|| v.get("mountpoint")) {
        Some(Value::Array(a)) => a.iter().filter_map(|m| m.as_str().map(str::to_string)).filter(|m| !m.is_empty()).collect(),
        Some(Value::String(m)) if !m.is_empty() => vec![m.clone()],
        _ => vec![],
    }
}

fn children(v: &Value) -> &[Value] {
    v.get("children").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// A device and everything below it (partitions, opened LUKS, ...).
fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(v);
    for c in children(v) {
        walk(c, out);
    }
}

/// Parse `lsblk -J -b -o <LSBLK_COLUMNS>` into whole disks.
pub fn parse_lsblk(json: &str, by_id: &BTreeMap<String, String>) -> Result<Vec<DiskInfo>> {
    let v: Value = serde_json::from_str(json).context("lsblk printed something that isn't JSON")?;
    let devs = v.get("blockdevices").and_then(Value::as_array).context("lsblk JSON has no blockdevices")?;
    let mut disks = vec![];
    for d in devs {
        let name = s(d, "name").unwrap_or_default();
        if s(d, "type").as_deref() != Some("disk") || name.starts_with("zram") || name.starts_with("ram") {
            continue;
        }
        let path = s(d, "path").unwrap_or_else(|| format!("/dev/{name}"));
        let partitions = children(d)
            .iter()
            .filter(|c| s(c, "type").as_deref() == Some("part"))
            .map(part_info)
            .collect();
        disks.push(DiskInfo {
            by_id: by_id.get(&path).cloned(),
            model: s(d, "model").unwrap_or_default(),
            serial: s(d, "serial").unwrap_or_default(),
            size: n(d, "size"),
            transport: s(d, "tran").unwrap_or_default(),
            removable: b(d, "rm"),
            kind: classify(d),
            partitions,
            path,
        });
    }
    Ok(disks)
}

fn part_info(c: &Value) -> PartInfo {
    // A partition's own mount, or the mount of what's inside it (an opened LUKS).
    let mut all = vec![];
    walk(c, &mut all);
    PartInfo {
        path: s(c, "path").unwrap_or_else(|| format!("/dev/{}", s(c, "name").unwrap_or_default())),
        uuid: s(c, "uuid"),
        label: s(c, "label"),
        partlabel: s(c, "partlabel"),
        fstype: s(c, "fstype"),
        size: n(c, "size"),
        mountpoint: all.iter().flat_map(|v| mounts(v)).next(),
    }
}

fn eq(a: &Option<String>, b: &str) -> bool {
    a.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(b))
}

/// What a disk holds, from the disk node and everything below it.
fn classify(disk: &Value) -> DiskKind {
    let mut all = vec![];
    walk(disk, &mut all);
    let fstype = |v: &Value| s(v, "fstype");
    let ptype = |v: &Value| s(v, "parttypename");

    let installer = all.iter().any(|v| eq(&fstype(v), "iso9660") || mounts(v).iter().any(|m| m == "/iso" || m.starts_with("/iso/")));
    if installer {
        return DiskKind::Installer;
    }

    let nixos = all
        .iter()
        .any(|v| eq(&s(v, "partlabel"), "disk-main-luks") || (eq(&fstype(v), "btrfs") && eq(&s(v, "label"), "nixos")));
    if nixos {
        return DiskKind::NixOs;
    }

    let bitlocker = all.iter().any(|v| eq(&fstype(v), "BitLocker"));
    let ntfs = all.iter().any(|v| eq(&fstype(v), "ntfs"));
    let msr = all.iter().any(|v| eq(&ptype(v), "Microsoft reserved"));
    // "Microsoft basic data" is also the type of most exFAT/FAT USB drives;
    // it only means Windows when the filesystem isn't some other known one.
    let basic_data = all.iter().any(|v| {
        eq(&ptype(v), "Microsoft basic data")
            && fstype(v).is_none_or(|f| f.eq_ignore_ascii_case("ntfs") || f.eq_ignore_ascii_case("BitLocker"))
    });
    if ntfs || bitlocker || msr || basic_data {
        let efi = all.iter().any(|v| eq(&ptype(v), "EFI System"));
        return DiskKind::Windows { bitlocker, system: efi && msr };
    }

    const LINUX_FS: [&str; 10] =
        ["ext4", "ext3", "ext2", "btrfs", "xfs", "f2fs", "swap", "crypto_LUKS", "LVM2_member", "bcachefs"];
    let linux = all.iter().any(|v| {
        fstype(v).is_some_and(|f| LINUX_FS.iter().any(|l| l.eq_ignore_ascii_case(&f)))
            || ptype(v).is_some_and(|p| p.starts_with("Linux"))
    });
    if linux {
        return DiskKind::Linux;
    }
    if children(disk).is_empty() && fstype(disk).is_none() {
        return DiskKind::Empty;
    }
    DiskKind::Other
}

// ---------------------------------------------------------------- previous installs

/// Previous installs made by this flake.
pub fn previous(disks: &[DiskInfo]) -> Vec<Previous> {
    disks
        .iter()
        .filter(|d| d.kind == DiskKind::NixOs)
        .filter_map(|d| {
            if let Some(p) = d.partitions.iter().find(|p| eq(&p.partlabel, "disk-main-luks")) {
                return Some(Previous { disk: d.path.clone(), root_part: p.path.clone(), encrypted: true });
            }
            if let Some(p) = d.partitions.iter().find(|p| eq(&p.fstype, "btrfs") && eq(&p.label, "nixos")) {
                return Some(Previous { disk: d.path.clone(), root_part: p.path.clone(), encrypted: false });
            }
            // A LUKS partition whose opened btrfs is labelled nixos.
            d.partitions
                .iter()
                .find(|p| eq(&p.fstype, "crypto_LUKS"))
                .map(|p| Previous { disk: d.path.clone(), root_part: p.path.clone(), encrypted: true })
        })
        .collect()
}

/// Disks the plan neither installs onto nor formats that still hold an old
/// NixOS install (partlabels `disk-main-*`, or a btrfs labelled `nixos`):
/// their names clash with the new install's, so the engine renames them
/// (nothing is erased). Kernel paths, e.g. /dev/nvme2n1.
pub fn clashing_old_installs(disks: &[DiskInfo], plan: &Plan) -> Vec<String> {
    let mut used = vec![plan.system.device.as_str()];
    for d in &plan.data {
        if let DataMode::Format { device, .. } = &d.mode {
            used.push(device);
        }
    }
    let in_plan = |d: &DiskInfo| used.iter().any(|u| *u == d.path || d.by_id.as_deref() == Some(*u));
    disks
        .iter()
        .filter(|d| d.kind != DiskKind::Installer && !in_plan(d))
        .filter(|d| {
            d.partitions.iter().any(|p| {
                p.partlabel.as_deref().is_some_and(|l| l.starts_with("disk-main-")) || (eq(&p.fstype, "btrfs") && eq(&p.label, "nixos"))
            })
        })
        .map(|d| d.path.clone())
        .collect()
}

// ---------------------------------------------------------------- the machine

pub fn machine() -> Machine {
    let online = std::thread::spawn(online);
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let efivar = |name: &str| std::fs::read(format!("{EFIVARS}/{name}-{EFI_GLOBAL}")).ok().and_then(|b| b.get(4).copied()) == Some(1);
    Machine {
        root: parse_uid(&read("/proc/self/status")) == Some(0),
        uefi: Path::new("/sys/firmware/efi").exists(),
        secure_boot: efivar("SecureBoot"),
        setup_mode: efivar("SetupMode"),
        tpm: Path::new("/sys/class/tpm/tpm0").exists(),
        ram_gb: parse_meminfo_gb(&read("/proc/meminfo")),
        online: online.join().unwrap_or(false),
    }
}

/// The real uid from /proc/self/status (`Uid:\t0\t0\t0\t0`).
pub fn parse_uid(status: &str) -> Option<u32> {
    status.lines().find_map(|l| l.strip_prefix("Uid:")).and_then(|r| r.split_whitespace().next()).and_then(|u| u.parse().ok())
}

/// Total RAM in GiB, rounded (`MemTotal:  65748292 kB`).
pub fn parse_meminfo_gb(meminfo: &str) -> u32 {
    let kb: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|k| k.parse().ok())
        .unwrap_or(0);
    ((kb as f64) / 1024.0 / 1024.0).round() as u32
}

/// Can we reach the binary cache? (DNS + TCP to cache.nixos.org:443, ~3 s at most.)
fn online() -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let ok = ("cache.nixos.org", 443)
            .to_socket_addrs()
            .map(|addrs| addrs.take(2).any(|a| TcpStream::connect_timeout(&a, Duration::from_secs(3)).is_ok()))
            .unwrap_or(false);
        let _ = tx.send(ok);
    });
    rx.recv_timeout(Duration::from_millis(3500)).unwrap_or(false)
}

// ---------------------------------------------------------------- data drives

/// The partition "mount as is" would use: the largest ntfs/ext4/btrfs/exfat/xfs one.
pub fn mountable_partition(d: &DiskInfo) -> Option<&PartInfo> {
    d.partitions
        .iter()
        .filter(|p| p.uuid.is_some() && p.fstype.as_deref().is_some_and(|f| MOUNTABLE_FS.iter().any(|m| m.eq_ignore_ascii_case(f))))
        .max_by_key(|p| p.size)
}

/// The fsType for the NixOS mount (the kernel's NTFS driver is ntfs3).
pub fn mount_fs_type(fstype: &str) -> String {
    match fstype.to_ascii_lowercase().as_str() {
        "ntfs" => "ntfs3".into(),
        f => f.into(),
    }
}

// ---------------------------------------------------------------- backup destinations

/// A partition /home can be copied to before anything is erased.
#[derive(Clone, Debug, PartialEq)]
pub struct BackupTarget {
    pub partition: String,
    /// The whole disk it's on.
    pub disk: String,
    pub model: String,
    pub fstype: String,
    pub label: Option<String>,
    pub size: u64,
    /// Free bytes; None when it couldn't be measured (not root, mount failed).
    pub free: Option<u64>,
}

/// Partitions with a backup-capable filesystem on disks that aren't being
/// erased (`erased`: kernel paths of whole disks) and aren't the installer.
pub fn backup_candidates(disks: &[DiskInfo], erased: &[String]) -> Vec<BackupTarget> {
    disks
        .iter()
        .filter(|d| d.kind != DiskKind::Installer && !erased.contains(&d.path))
        .flat_map(|d| {
            d.partitions.iter().filter_map(move |p| {
                let fs = p.fstype.as_deref()?;
                BACKUP_FS.iter().any(|b| b.eq_ignore_ascii_case(fs)).then(|| BackupTarget {
                    partition: p.path.clone(),
                    disk: d.path.clone(),
                    model: d.model.clone(),
                    fstype: fs.to_ascii_lowercase(),
                    label: p.label.clone(),
                    size: p.size,
                    free: None,
                })
            })
        })
        .collect()
}

/// [`backup_candidates`] with their free space measured: `df` where already
/// mounted, else a temporary read-only mount at [`PROBE_MOUNT`].
pub fn backup_targets(disks: &[DiskInfo], erased: &[String]) -> Vec<BackupTarget> {
    let mut out = backup_candidates(disks, erased);
    for t in &mut out {
        let mounted = disks.iter().flat_map(|d| &d.partitions).find(|p| p.path == t.partition).and_then(|p| p.mountpoint.clone());
        t.free = match mounted {
            Some(m) => df_avail(&m),
            None => probe_free(&t.partition, &t.fstype),
        };
    }
    out
}

fn df_avail(dir: &str) -> Option<u64> {
    let out = Command::new("df").args(["-B1", "--output=avail", dir]).output().ok()?;
    out.status.success().then(|| parse_df_avail(&String::from_utf8_lossy(&out.stdout))).flatten()
}

/// `df -B1 --output=avail` prints a header line, then the number.
pub fn parse_df_avail(out: &str) -> Option<u64> {
    out.lines().nth(1)?.trim().parse().ok()
}

fn probe_free(part: &str, fstype: &str) -> Option<u64> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    std::fs::create_dir_all(PROBE_MOUNT).ok()?;
    // Never replay a journal on a disk we only look at.
    let opts = match fstype {
        "ext4" => "ro,noload",
        "xfs" => "ro,norecovery",
        _ => "ro",
    };
    let ok = Command::new("mount").args(["-t", fstype, "-o", opts, part, PROBE_MOUNT]).output().is_ok_and(|o| o.status.success());
    if !ok {
        return None;
    }
    let free = df_avail(PROBE_MOUNT);
    let _ = Command::new("umount").arg(PROBE_MOUNT).output();
    free
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `lsblk -J -b -o ...` on the real PC (serials and UUIDs made up), booted
    /// from the installer USB with the old NixOS disk still locked.
    pub const FIXTURE: &str = r#"{
   "blockdevices": [
      {"name":"loop0", "path":"/dev/loop0", "type":"loop", "size":1182343168, "model":null, "serial":null, "tran":null, "rm":false, "fstype":"squashfs", "label":null, "partlabel":null, "uuid":null, "parttypename":null, "mountpoints":["/nix/.ro-store"]},
      {"name":"sda", "path":"/dev/sda", "type":"disk", "size":2000398934016, "model":"ST2000DM008-2UB102", "serial":"ZFL4ABCD", "tran":"sata", "rm":false, "fstype":null, "label":null, "partlabel":null, "uuid":null, "parttypename":null, "mountpoints":[null],
         "children": [
            {"name":"sda1", "path":"/dev/sda1", "type":"part", "size":16777216, "model":null, "serial":null, "tran":null, "rm":false, "fstype":null, "label":null, "partlabel":"Microsoft reserved partition", "uuid":null, "parttypename":"Microsoft reserved", "mountpoints":[null]},
            {"name":"sda2", "path":"/dev/sda2", "type":"part", "size":2000381018112, "model":null, "serial":null, "tran":null, "rm":false, "fstype":"ntfs", "label":"Data", "partlabel":"Basic data partition", "uuid":"5E8C2F1A8C2EEE4B", "parttypename":"Microsoft basic data", "mountpoints":[null]}
         ]
      },
      {"name":"sdb", "path":"/dev/sdb", "type":"disk", "size":30752636928, "model":"Ultra", "serial":"4C530001231115117283", "tran":"usb", "rm":true, "fstype":"iso9660", "label":"nixos-minimal-25.05-x86_64", "partlabel":null, "uuid":"1970-01-01-00-00-01-00", "parttypename":null, "mountpoints":[null],
         "children": [
            {"name":"sdb1", "path":"/dev/sdb1", "type":"part", "size":1221591040, "model":null, "serial":null, "tran":null, "rm":true, "fstype":"iso9660", "label":"nixos-minimal-25.05-x86_64", "partlabel":null, "uuid":"1970-01-01-00-00-01-00", "parttypename":"Basic data", "mountpoints":["/iso"]},
            {"name":"sdb2", "path":"/dev/sdb2", "type":"part", "size":3145728, "model":null, "serial":null, "tran":null, "rm":true, "fstype":"vfat", "label":"EFIBOOT", "partlabel":null, "uuid":"1234-5678", "parttypename":"EFI (FAT-12/16/32)", "mountpoints":[null]}
         ]
      },
      {"name":"nvme0n1", "path":"/dev/nvme0n1", "type":"disk", "size":1000204886016, "model":"CT1000P310SSD8", "serial":"24304A1B2C3D", "tran":"nvme", "rm":false, "fstype":null, "label":null, "partlabel":null, "uuid":null, "parttypename":null, "mountpoints":[null],
         "children": [
            {"name":"nvme0n1p1", "path":"/dev/nvme0n1p1", "type":"part", "size":1073741824, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":"vfat", "label":null, "partlabel":"disk-main-ESP", "uuid":"9A1B-2C3D", "parttypename":"EFI System", "mountpoints":[null]},
            {"name":"nvme0n1p2", "path":"/dev/nvme0n1p2", "type":"part", "size":999129399296, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":"crypto_LUKS", "label":null, "partlabel":"disk-main-luks", "uuid":"0f6a3c1e-7d42-4a5e-9b1f-3c2d1e0a9b8c", "parttypename":"Linux filesystem", "mountpoints":[null]}
         ]
      },
      {"name":"nvme1n1", "path":"/dev/nvme1n1", "type":"disk", "size":2000398934016, "model":"Sabrent Rocket 4.0 2TB", "serial":"A1B2C3D4E5F6G7H8", "tran":"nvme", "rm":false, "fstype":null, "label":null, "partlabel":null, "uuid":null, "parttypename":null, "mountpoints":[null],
         "children": [
            {"name":"nvme1n1p1", "path":"/dev/nvme1n1p1", "type":"part", "size":104857600, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":"vfat", "label":null, "partlabel":"EFI system partition", "uuid":"4C2E-1F0A", "parttypename":"EFI System", "mountpoints":[null]},
            {"name":"nvme1n1p2", "path":"/dev/nvme1n1p2", "type":"part", "size":16777216, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":null, "label":null, "partlabel":"Microsoft reserved partition", "uuid":null, "parttypename":"Microsoft reserved", "mountpoints":[null]},
            {"name":"nvme1n1p3", "path":"/dev/nvme1n1p3", "type":"part", "size":1999634579456, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":"ntfs", "label":null, "partlabel":"Basic data partition", "uuid":"7A3E5B2C3E5AE7F1", "parttypename":"Microsoft basic data", "mountpoints":[null]},
            {"name":"nvme1n1p4", "path":"/dev/nvme1n1p4", "type":"part", "size":643825664, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":"ntfs", "label":null, "partlabel":null, "uuid":"2A4C6E8F0A1B3C5D", "parttypename":"Windows recovery environment", "mountpoints":[null]}
         ]
      },
      {"name":"nvme2n1", "path":"/dev/nvme2n1", "type":"disk", "size":1000204886016, "model":"Sabrent", "serial":"48790123456789012345", "tran":"nvme", "rm":false, "fstype":null, "label":null, "partlabel":null, "uuid":null, "parttypename":null, "mountpoints":[null],
         "children": [
            {"name":"nvme2n1p1", "path":"/dev/nvme2n1p1", "type":"part", "size":16777216, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":null, "label":null, "partlabel":"Microsoft reserved partition", "uuid":null, "parttypename":"Microsoft reserved", "mountpoints":[null]},
            {"name":"nvme2n1p2", "path":"/dev/nvme2n1p2", "type":"part", "size":1000186314752, "model":null, "serial":null, "tran":"nvme", "rm":false, "fstype":"ntfs", "label":"L", "partlabel":"Basic data partition", "uuid":"B2D4F6A8C0E1F3A5", "parttypename":"Microsoft basic data", "mountpoints":[null]}
         ]
      },
      {"name":"zram0", "path":"/dev/zram0", "type":"disk", "size":16777216000, "model":null, "serial":null, "tran":null, "rm":false, "fstype":"swap", "label":null, "partlabel":null, "uuid":null, "parttypename":null, "mountpoints":["[SWAP]"]}
   ]
}"#;

    pub fn by_id() -> BTreeMap<String, String> {
        [
            ("/dev/nvme0n1", "nvme-CT1000P310SSD8_24304A1B2C3D"),
            ("/dev/nvme1n1", "nvme-Sabrent_Rocket_4.0_2TB_A1B2C3D4E5F6G7H8"),
            ("/dev/nvme2n1", "nvme-Sabrent_48790123456789012345"),
            ("/dev/sda", "ata-ST2000DM008-2UB102_ZFL4ABCD"),
            ("/dev/sdb", "usb-SanDisk_Ultra_4C530001231115117283-0:0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), format!("/dev/disk/by-id/{v}")))
        .collect()
    }

    pub fn fixture_disks() -> Vec<DiskInfo> {
        parse_lsblk(FIXTURE, &by_id()).unwrap()
    }

    fn disk<'a>(d: &'a [DiskInfo], path: &str) -> &'a DiskInfo {
        d.iter().find(|d| d.path == path).unwrap()
    }

    #[test]
    fn parses_whole_disks_only() {
        let d = fixture_disks();
        let paths: Vec<_> = d.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["/dev/sda", "/dev/sdb", "/dev/nvme0n1", "/dev/nvme1n1", "/dev/nvme2n1"]);
        let c = disk(&d, "/dev/nvme0n1");
        assert_eq!(c.model, "CT1000P310SSD8");
        assert_eq!(c.size, 1000204886016);
        assert_eq!(c.transport, "nvme");
        assert_eq!(c.by_id.as_deref(), Some("/dev/disk/by-id/nvme-CT1000P310SSD8_24304A1B2C3D"));
        assert_eq!(c.partitions.len(), 2);
        assert!(disk(&d, "/dev/sdb").removable);
    }

    #[test]
    fn classifies_the_pc() {
        let d = fixture_disks();
        assert_eq!(disk(&d, "/dev/nvme0n1").kind, DiskKind::NixOs);
        assert_eq!(disk(&d, "/dev/nvme1n1").kind, DiskKind::Windows { bitlocker: false, system: true });
        assert_eq!(disk(&d, "/dev/sda").kind, DiskKind::Windows { bitlocker: false, system: false });
        assert_eq!(disk(&d, "/dev/nvme2n1").kind, DiskKind::Windows { bitlocker: false, system: false });
        assert_eq!(disk(&d, "/dev/sdb").kind, DiskKind::Installer);
    }

    #[test]
    fn classifies_other_layouts() {
        let one = |children: &str, extra: &str| {
            let j = format!(
                r#"{{"blockdevices":[{{"name":"vda","path":"/dev/vda","type":"disk","size":1,"rm":false{extra},"mountpoints":[null],"children":[{children}]}}]}}"#
            );
            parse_lsblk(&j, &BTreeMap::new()).unwrap().remove(0).kind
        };
        let part = |fs: &str, pt: &str, label: &str| {
            format!(r#"{{"name":"vda1","path":"/dev/vda1","type":"part","size":1,"fstype":{fs},"parttypename":{pt},"label":{label},"mountpoints":[null]}}"#)
        };
        assert_eq!(one("", ""), DiskKind::Empty);
        assert_eq!(one(&part(r#""ext4""#, r#""Linux filesystem""#, "null"), ""), DiskKind::Linux);
        assert_eq!(one(&part(r#""btrfs""#, r#""Linux filesystem""#, r#""nixos""#), ""), DiskKind::NixOs);
        assert_eq!(
            one(&part(r#""BitLocker""#, r#""Microsoft basic data""#, "null"), ""),
            DiskKind::Windows { bitlocker: true, system: false }
        );
        // An exFAT stick has a "basic data" partition but isn't Windows.
        assert_eq!(one(&part(r#""exfat""#, r#""Microsoft basic data""#, "null"), ""), DiskKind::Other);
        // Old lsblk: strings for numbers and booleans, MOUNTPOINT.
        let old = r#"{"blockdevices":[{"name":"sdc","type":"disk","size":"512","rm":"1","mountpoint":null}]}"#;
        let d = parse_lsblk(old, &BTreeMap::new()).unwrap();
        assert_eq!((d[0].size, d[0].removable, d[0].path.as_str(), &d[0].kind), (512, true, "/dev/sdc", &DiskKind::Empty));
    }

    #[test]
    fn finds_the_previous_install() {
        let p = previous(&fixture_disks());
        assert_eq!(p, vec![Previous { disk: "/dev/nvme0n1".into(), root_part: "/dev/nvme0n1p2".into(), encrypted: true }]);
    }

    #[test]
    fn old_installs_left_in_the_pc() {
        use super::super::plan::{DataDisk, HomePlan, Settings, SystemDisk};
        let d = fixture_disks();
        let plan = |device: &str, data: Vec<DataDisk>| Plan {
            flake: "/tmp/x".into(),
            host: "arc".into(),
            system: SystemDisk { device: device.into(), encrypt: true, tpm: false, swap_gb: 0 },
            data,
            settings: Settings {
                hostname: "arc".into(),
                username: "vainnor".into(),
                time_zone: "UTC".into(),
                wallpaper: "eye".into(),
                theme: "match".into(),
            },
            disk_password: None,
            user_password: None,
            previous: None,
            carry: Default::default(),
            home: HomePlan::Fresh,
        };
        // Installing onto the old disk (by-id): nothing left over.
        assert!(clashing_old_installs(&d, &plan("/dev/disk/by-id/nvme-CT1000P310SSD8_24304A1B2C3D", vec![])).is_empty());
        // Installing onto the Sabrent: the Crucial still holds disk-main-*.
        assert_eq!(clashing_old_installs(&d, &plan("/dev/disk/by-id/ata-ST2000DM008-2UB102_ZFL4ABCD", vec![])), vec!["/dev/nvme0n1"]);
        // ...unless it's formatted as a data drive.
        let fmt = DataDisk {
            name: "games".into(),
            mount: "/home/vainnor/games".into(),
            mode: DataMode::Format { device: "/dev/nvme0n1".into(), encrypt: true },
        };
        assert!(clashing_old_installs(&d, &plan("/dev/sda", vec![fmt])).is_empty());
    }

    #[test]
    fn picks_stable_ids() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            pick_by_id(&names(&[
                "nvme-eui.002538b231b0a1b2",
                "nvme-CT1000P310SSD8_24304A1B2C3D_1",
                "nvme-CT1000P310SSD8_24304A1B2C3D",
                "nvme-CT1000P310SSD8_24304A1B2C3D-part1",
                "nvme-nvme.c0a9-3234",
            ])),
            Some("nvme-CT1000P310SSD8_24304A1B2C3D".into())
        );
        assert_eq!(
            pick_by_id(&names(&["wwn-0x5000c500e1234567", "ata-ST2000DM008-2UB102_ZFL4ABCD"])),
            Some("ata-ST2000DM008-2UB102_ZFL4ABCD".into())
        );
        assert_eq!(pick_by_id(&names(&["wwn-0x5000c500e1234567"])), None);
    }

    #[test]
    fn data_drive_helpers() {
        let d = fixture_disks();
        let p = mountable_partition(disk(&d, "/dev/nvme1n1")).unwrap();
        assert_eq!(p.path, "/dev/nvme1n1p3", "the big C: partition, not recovery");
        assert_eq!(mount_fs_type(p.fstype.as_deref().unwrap()), "ntfs3");
        assert!(mountable_partition(disk(&d, "/dev/nvme0n1")).is_none(), "LUKS can't be mounted as is");
    }

    #[test]
    fn backup_skips_erased_ntfs_and_installer() {
        let mut d = fixture_disks();
        // Nothing on the real PC qualifies (all NTFS or erased)...
        assert!(backup_candidates(&d, &["/dev/nvme0n1".into()]).is_empty());
        // ...until an ext4 partition shows up.
        d[0].partitions[1].fstype = Some("ext4".into());
        let t = backup_candidates(&d, &["/dev/nvme0n1".into()]);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].partition.as_str(), t[0].fstype.as_str()), ("/dev/sda2", "ext4"));
        assert!(backup_candidates(&d, &["/dev/sda".into()]).is_empty());
    }

    #[test]
    fn proc_parsing() {
        assert_eq!(parse_uid("Name:\tvx\nUid:\t0\t0\t0\t0\nGid:\t0\n"), Some(0));
        assert_eq!(parse_uid("Uid:\t1000\t1000\t1000\t1000\n"), Some(1000));
        assert_eq!(parse_meminfo_gb("MemTotal:       65748292 kB\nMemFree: 1 kB\n"), 63);
        assert_eq!(parse_df_avail("   Avail\n123456789\n"), Some(123456789));
    }
}
