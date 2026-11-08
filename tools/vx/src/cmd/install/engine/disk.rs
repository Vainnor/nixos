//! Block devices: lsblk, mounts, and the safety checks run before anything is touched.

use super::super::plan::{DataMode, HomePlan, OLD_MOUNT, Plan, WORK};
use super::runner::{Cmd, Runner};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One node of `lsblk -J` output.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Blk {
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub fstype: Option<String>,
    #[serde(default)]
    pub partlabel: Option<String>,
    #[serde(default)]
    pub mountpoints: Vec<Option<String>>,
    #[serde(default)]
    pub children: Vec<Blk>,
}

impl Blk {
    /// This node and everything under it.
    pub fn walk(&self) -> Vec<&Blk> {
        let mut v = vec![self];
        for c in &self.children {
            v.extend(c.walk());
        }
        v
    }

    pub fn mounts(&self) -> Vec<String> {
        self.mountpoints.iter().flatten().filter(|m| !m.is_empty()).cloned().collect()
    }
}

#[derive(Deserialize)]
struct Lsblk {
    blockdevices: Vec<Blk>,
}

pub fn parse_lsblk(json: &str) -> Result<Vec<Blk>> {
    Ok(serde_json::from_str::<Lsblk>(json).context("could not read lsblk output")?.blockdevices)
}

/// What DryRun pretends a disk looks like: ESP + LUKS, like this flake's layout.
fn fake_tree(dev: &str) -> String {
    format!(
        r#"{{"blockdevices":[{{"name":"{dev}","type":"disk","fstype":null,"partlabel":null,"mountpoints":[null],
        "children":[
          {{"name":"{dev}-part1","type":"part","fstype":"vfat","partlabel":"disk-main-ESP","mountpoints":[null]}},
          {{"name":"{dev}-part2","type":"part","fstype":"crypto_LUKS","partlabel":"disk-main-luks","mountpoints":[null]}}
        ]}}]}}"#
    )
}

/// `lsblk -J` of one device and what's under it.
pub fn tree(r: &Runner, dev: &str) -> Result<Blk> {
    let out = r.query(
        Cmd::new("lsblk").args(["-J", "-p", "-o", "NAME,TYPE,FSTYPE,PARTLABEL,MOUNTPOINTS", dev]).fake(fake_tree(dev)),
    )?;
    parse_lsblk(&out)?.into_iter().next().with_context(|| format!("lsblk shows nothing for {dev}"))
}

/// Follow /dev/disk/by-id/... symlinks to the kernel name. DryRun: unchanged.
pub fn resolve(r: &Runner, dev: &str) -> Result<String> {
    if r.dry() {
        return Ok(dev.to_string());
    }
    let p = std::fs::canonicalize(dev)
        .with_context(|| format!("{dev} isn't there anymore: go back and pick the drive again"))?;
    Ok(p.to_string_lossy().into_owned())
}

/// The partition of `disk` with this partlabel, else the first with this fstype.
pub fn find_part(t: &Blk, partlabel: &str, fstype: &str) -> Option<String> {
    let parts: Vec<&Blk> = t.walk().into_iter().filter(|b| b.kind == "part").collect();
    parts
        .iter()
        .find(|b| b.partlabel.as_deref() == Some(partlabel))
        .or_else(|| parts.iter().find(|b| b.fstype.as_deref() == Some(fstype)))
        .map(|b| b.name.clone())
}

/// Device-mapper (crypt) nodes under a device, deepest first.
pub fn crypt_children(t: &Blk) -> Vec<String> {
    let mut v: Vec<String> = t.walk().into_iter().filter(|b| b.kind == "crypt").map(|b| b.name.clone()).collect();
    v.reverse();
    v
}

/// Mount points in /proc/self/mounts at or under `prefix`.
pub fn mounted_under(prefix: &Path) -> Vec<PathBuf> {
    let Ok(m) = std::fs::read_to_string("/proc/self/mounts") else { return vec![] };
    m.lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(|p| PathBuf::from(p.replace("\\040", " ").replace("\\011", "\t")))
        .filter(|p| p.starts_with(prefix))
        .collect()
}

pub fn is_mounted(r: &Runner, p: &Path) -> bool {
    !r.dry() && mounted_under(p).iter().any(|m| m == p)
}

/// Mounts the installer itself makes (fine to find on a target disk).
fn ours(m: &str) -> bool {
    let m = Path::new(m);
    [Path::new("/mnt"), Path::new(OLD_MOUNT), Path::new("/run/vx-install"), Path::new(WORK)].iter().any(|p| m.starts_with(p))
}

/// Why `t` can't be used, if it is in use by something other than the installer.
pub fn busy(t: &Blk) -> Option<String> {
    for b in t.walk() {
        for m in b.mounts() {
            if m == "[SWAP]" {
                return Some(format!("{} is in use as swap: run swapoff {} or pick another drive", b.name, b.name));
            }
            if m.starts_with("/iso") || m == "/nix/.ro-store" {
                return Some(format!("{} is the installer usb", b.name));
            }
            if !ours(&m) {
                return Some(format!("{} is mounted at {m}: run umount {m} or pick another drive", b.name));
            }
        }
    }
    None
}

/// The kernel names (partition and its disk) the installer runs from, if any.
fn installer_devices(r: &Runner) -> Vec<String> {
    let mut v = Vec::new();
    let src = r.query(Cmd::new("findmnt").args(["-n", "-o", "SOURCE", "/iso"]).fake("/dev/sdz1"));
    let Ok(src) = src else { return v };
    let src = src.trim().to_string();
    if src.is_empty() {
        return v;
    }
    let src = resolve(r, &src).unwrap_or(src);
    if let Ok(parent) = r.query(Cmd::new("lsblk").args(["-n", "-d", "-p", "-o", "PKNAME", &src]).fake("/dev/sdz")) {
        let p = parent.trim();
        if !p.is_empty() {
            v.push(p.to_string());
        }
    }
    v.push(src);
    v
}

/// Everything the plan will touch exists, is what it should be, and nothing
/// else is using it. Runs before any change.
pub fn safety_checks(plan: &Plan, r: &Runner) -> Result<()> {
    // Which devices, and what for.
    let mut wipe: Vec<(String, &str)> = vec![(plan.system.device.clone(), "system drive")];
    for d in &plan.data {
        if let DataMode::Format { device, .. } = &d.mode {
            wipe.push((device.clone(), "data drive"));
        }
    }
    let installer = installer_devices(r);
    let mut seen: Vec<String> = Vec::new();
    for (dev, what) in &wipe {
        let real = resolve(r, dev).with_context(|| format!("the {what} {dev} isn't there anymore: go back and pick it again"))?;
        if seen.contains(&real) {
            bail!("{dev} is picked twice: a drive can only be used once");
        }
        if installer.contains(&real) {
            bail!("{dev} is the installer usb: pick another drive");
        }
        let t = tree(r, &real)?;
        if t.kind != "disk" {
            bail!("the {what} {dev} is a {}, not a whole disk: pick the disk", t.kind);
        }
        if let Some(why) = busy(&t) {
            bail!("{why}");
        }
        seen.push(real);
    }

    for d in &plan.data {
        if let DataMode::Mount { uuid, .. } = &d.mode {
            let p = format!("/dev/disk/by-uuid/{uuid}");
            if !r.exists(Path::new(&p), true) {
                bail!("no partition with uuid {uuid} (for {}): is the drive plugged in?", d.mount);
            }
        }
    }

    match &plan.home {
        HomePlan::Keep => {
            let Some(prev) = &plan.previous else { bail!("keeping /home needs the previous install unlocked first") };
            // DryRun (the demo) has no real paths to compare.
            if !r.dry() && resolve(r, &prev.previous.disk)? != seen[0] {
                bail!("keeping /home only works when installing onto the previous install's drive ({})", prev.previous.disk);
            }
            if prev.previous.encrypted != plan.system.encrypt {
                bail!("keeping /home keeps the disk's encryption as it was: go back and set encryption to match");
            }
        }
        HomePlan::Backup { partition } => {
            if plan.previous.is_none() {
                bail!("backing up /home needs the previous install unlocked first");
            }
            let real = resolve(r, partition)?;
            let t = tree(r, &real)?;
            if !r.dry() && t.kind != "part" {
                bail!("the backup target {partition} is a {}, not a partition", t.kind);
            }
            if let Ok(parent) = r.query(Cmd::new("lsblk").args(["-n", "-d", "-p", "-o", "PKNAME", &real]).fake("/dev/sdy")) {
                let parent = parent.trim().to_string();
                if seen.contains(&parent) {
                    bail!("the backup target {partition} is on a drive that is about to be erased: pick another partition");
                }
                if installer.contains(&parent) || installer.contains(&real) {
                    bail!("the backup target {partition} is the installer usb: pick another partition");
                }
            }
            if let Some(why) = busy(&t) {
                bail!("{why}");
            }
        }
        HomePlan::Fresh => {}
    }

    // Plan-level checks that need no disks.
    super::settings::validate(&plan.settings)?;
    let encrypted = plan.system.encrypt;
    if encrypted && plan.disk_password.as_ref().is_none_or(|p| p.0.is_empty()) {
        bail!("encryption is on but there is no disk password: go back to passwords");
    }
    if plan.system.tpm && !encrypted {
        bail!("tpm unlock needs encryption on");
    }
    if plan.user_password.is_none() {
        let carried = plan.carry.contains(&super::super::plan::Carry::Password)
            && plan.previous.as_ref().is_some_and(|p| !p.users.is_empty());
        if !carried {
            bail!("no login password: set one, or carry over the old one");
        }
    }
    let name = regex::Regex::new(r"^[a-z0-9-]+$").unwrap();
    for d in &plan.data {
        if !name.is_match(&d.name) {
            bail!("data drive name '{}': use a-z, 0-9 and -", d.name);
        }
        if !d.mount.starts_with('/') || d.mount.contains("..") || d.mount.contains(char::is_whitespace) || d.mount.contains('\'') {
            bail!("data drive mount '{}': use an absolute path without spaces", d.mount);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSBLK: &str = r#"{"blockdevices":[{"name":"/dev/nvme0n1","type":"disk","fstype":null,"partlabel":null,"mountpoints":[null],
      "children":[
        {"name":"/dev/nvme0n1p1","type":"part","fstype":"vfat","partlabel":"disk-main-ESP","mountpoints":[null]},
        {"name":"/dev/nvme0n1p2","type":"part","fstype":"crypto_LUKS","partlabel":"disk-main-luks","mountpoints":[null],
         "children":[{"name":"/dev/mapper/vx-old","type":"crypt","fstype":"btrfs","partlabel":null,
           "mountpoints":["/run/vx-install/old/home","/run/vx-install/old/persistent"]}]}
      ]}]}"#;

    #[test]
    fn parses_and_finds_parts() {
        let t = parse_lsblk(LSBLK).unwrap().remove(0);
        assert_eq!(find_part(&t, "disk-main-ESP", "vfat").as_deref(), Some("/dev/nvme0n1p1"));
        assert_eq!(find_part(&t, "disk-main-luks", "crypto_LUKS").as_deref(), Some("/dev/nvme0n1p2"));
        assert_eq!(crypt_children(&t), ["/dev/mapper/vx-old"]);
        // Our own read-only mounts don't make it busy.
        assert_eq!(busy(&t), None);
    }

    #[test]
    fn busy_disks() {
        let t = parse_lsblk(&LSBLK.replace("/run/vx-install/old/home", "/home")).unwrap().remove(0);
        assert!(busy(&t).unwrap().contains("umount /home"));
        let t = parse_lsblk(&LSBLK.replace("/run/vx-install/old/home", "[SWAP]")).unwrap().remove(0);
        assert!(busy(&t).unwrap().contains("swapoff"));
        let t = parse_lsblk(&LSBLK.replace("/run/vx-install/old/home", "/iso")).unwrap().remove(0);
        assert!(busy(&t).unwrap().contains("installer"));
    }
}
