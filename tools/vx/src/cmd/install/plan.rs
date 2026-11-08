//! The contract between the installer's screens (`ui`, `probe`) and the part
//! that does the work (`engine`). The UI fills in a [`Plan`]; the engine
//! carries it out and reports progress as [`Event`]s.
//!
//! Paths shared with the Nix side (modules/hosts/arc/_disko.nix,
//! modules/system/secure-boot.nix) are the consts at the bottom. Change them
//! in both places or not at all.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

// ---------------------------------------------------------------- disks

/// One whole disk, as `lsblk` sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskInfo {
    /// Kernel path, e.g. /dev/nvme0n1.
    pub path: String,
    /// Stable /dev/disk/by-id/... path (what disks.json stores). None for odd devices.
    pub by_id: Option<String>,
    pub model: String,
    pub serial: String,
    /// Bytes.
    pub size: u64,
    /// nvme, sata, usb, ...
    pub transport: String,
    pub removable: bool,
    pub kind: DiskKind,
    pub partitions: Vec<PartInfo>,
}

/// What a disk currently holds (shown next to it, and drives the warnings).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiskKind {
    /// No partitions or filesystems.
    Empty,
    /// The USB stick the installer runs from. Never selectable.
    Installer,
    /// NTFS / Microsoft partitions. `system`: has a Windows boot (EFI + MSR), i.e. a C: drive.
    Windows { bitlocker: bool, system: bool },
    /// A NixOS install made by this flake (ESP + LUKS/btrfs labelled "nixos").
    NixOs,
    /// Some other Linux.
    Linux,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartInfo {
    pub path: String,
    pub uuid: Option<String>,
    pub label: Option<String>,
    pub partlabel: Option<String>,
    /// lsblk FSTYPE: ext4, btrfs, ntfs, vfat, crypto_LUKS, BitLocker, ...
    pub fstype: Option<String>,
    pub size: u64,
    pub mountpoint: Option<String>,
}

// ---------------------------------------------------------------- previous install

/// A NixOS install made by this flake, found on a disk (before unlocking).
#[derive(Clone, Debug, PartialEq)]
pub struct Previous {
    /// The whole disk, e.g. /dev/nvme1n1.
    pub disk: String,
    /// The partition holding the btrfs (the LUKS container when encrypted).
    pub root_part: String,
    pub encrypted: bool,
}

/// A previous install, unlocked and mounted read-only under [`OLD_MOUNT`].
#[derive(Clone, Debug)]
pub struct Unlocked {
    pub previous: Previous,
    /// The @persistent subvolume, mounted read-only.
    pub persistent: PathBuf,
    /// The @home subvolume, mounted read-only.
    pub home: PathBuf,
    /// What can be carried over (only items actually present).
    pub available: BTreeSet<Carry>,
    /// Bytes used by /home (for the backup size check).
    pub home_bytes: u64,
    /// Users with a password file in @persistent/passwords.
    pub users: Vec<String>,
    /// The old system's swap file size in GiB (0 = none), so "keep /home" can keep the layout.
    pub swap_gb: u32,
}

/// Things that can be carried from the previous install into the new one.
/// Paths are relative to the @persistent subvolume.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Carry {
    /// SSH host key: also the key that decrypts secrets, so secrets work on first boot.
    HostKey,
    /// NetBird peer identity: stays logged in, keeps its address.
    NetBird,
    /// Your Secure Boot keys: no BIOS key reset needed if they're still enrolled.
    SecureBoot,
    /// Login password hash(es).
    Password,
    /// Bluetooth pairings and saved Wi-Fi.
    Pairings,
    /// Everything else in /persistent (machine id, fan curves, VMs' state, ...).
    OtherState,
}

impl Carry {
    pub const ALL: [Carry; 6] =
        [Carry::HostKey, Carry::NetBird, Carry::SecureBoot, Carry::Password, Carry::Pairings, Carry::OtherState];

    pub fn label(self) -> &'static str {
        match self {
            Carry::HostKey => "ssh host key (secrets work on first boot)",
            Carry::NetBird => "netbird login",
            Carry::SecureBoot => "secure boot keys",
            Carry::Password => "login password",
            Carry::Pairings => "bluetooth pairings and wi-fi",
            Carry::OtherState => "everything else in /persistent",
        }
    }

    /// Paths inside @persistent this item covers. OtherState is "everything not
    /// covered by the others" (and never `install/`), so it has none.
    pub fn paths(self) -> &'static [&'static str] {
        match self {
            Carry::HostKey => &["etc/ssh"],
            Carry::NetBird => &["var/lib/netbird"],
            Carry::SecureBoot => &["var/lib/sbctl"],
            Carry::Password => &["passwords"],
            Carry::Pairings => &["var/lib/bluetooth", "etc/NetworkManager/system-connections"],
            Carry::OtherState => &[],
        }
    }
}

/// What happens to /home.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HomePlan {
    /// A new, empty /home.
    Fresh,
    /// Reinstall onto the same disk without reformatting it: only the system
    /// and /persistent are replaced; @home (and the Nix store) stay. Only
    /// possible when the system disk is the previous install's disk; the
    /// encryption and swap settings then stay as they were.
    Keep,
    /// Copy the old /home to a folder on this partition (one that is not being
    /// erased) before anything is wiped. The new /home starts empty.
    Backup { partition: String },
}

// ---------------------------------------------------------------- the plan

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemDisk {
    /// /dev/disk/by-id/... of the whole disk.
    pub device: String,
    pub encrypt: bool,
    /// Unlock with the TPM once Secure Boot is on (needs `encrypt`).
    pub tpm: bool,
    /// Swap file size in GiB, 0 = none (zram is always on).
    pub swap_gb: u32,
}

/// An extra drive mounted in the new system.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataDisk {
    /// Short name: [a-z0-9-]+, used for the label, LUKS name and key file.
    pub name: String,
    /// Where it's mounted, e.g. /home/vainnor/games or /mnt/d.
    pub mount: String,
    #[serde(flatten)]
    pub mode: DataMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum DataMode {
    /// Erase it and make one btrfs volume (optionally encrypted; unlocked at
    /// boot with a key file on /persistent).
    Format { device: String, encrypt: bool },
    /// Keep what's on it and just mount it (e.g. a Windows data drive).
    Mount {
        uuid: String,
        /// Filesystem type for the mount: ntfs3, ext4, btrfs, exfat, vfat, xfs.
        #[serde(rename = "fsType")]
        fs_type: String,
        #[serde(rename = "readOnly")]
        read_only: bool,
    },
}

/// Values written into the repo copy before installing (meta.nix, the host file).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub hostname: String,
    pub username: String,
    #[serde(rename = "timeZone")]
    pub time_zone: String,
    pub wallpaper: String,
    pub theme: String,
}

/// A password typed in the installer. Never printed by Debug.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Clone, Debug)]
pub struct Plan {
    /// The repo copy the installer edits and installs from ([`WORK_REPO`]).
    pub flake: PathBuf,
    /// nixosConfigurations attribute (always "arc" for now; `hostname` may differ).
    pub host: String,
    pub system: SystemDisk,
    pub data: Vec<DataDisk>,
    pub settings: Settings,
    /// Disk passphrase when encrypting. With HomePlan::Keep: the old passphrase.
    pub disk_password: Option<Secret>,
    /// Login password. None when the old one is carried over.
    pub user_password: Option<Secret>,
    pub previous: Option<Unlocked>,
    pub carry: BTreeSet<Carry>,
    pub home: HomePlan,
}

/// Facts about this machine, gathered once at start (and shown on the welcome screen).
#[derive(Clone, Debug, Default)]
pub struct Machine {
    pub root: bool,
    pub uefi: bool,
    pub online: bool,
    /// Secure Boot currently on.
    pub secure_boot: bool,
    /// Firmware in Setup Mode (no keys enrolled: lanzaboote can enroll ours).
    pub setup_mode: bool,
    pub tpm: bool,
    /// Total RAM in GiB (swap size hint).
    pub ram_gb: u32,
}

/// Defaults and choices from the flake, from `install-data.json` (built by
/// Nix: packages.<system>.install-data; on the ISO its path is in
/// $VX_INSTALL_DATA).
#[derive(Clone, Debug, Deserialize)]
pub struct InstallData {
    pub host: String,
    #[serde(rename = "repoUrl")]
    pub repo_url: String,
    /// The flake source this data was built from (copied to WORK_REPO when not updating).
    pub flake: String,
    pub defaults: Settings,
    pub wallpapers: Vec<WallpaperInfo>,
    /// Theme names besides "match" (assets/themes).
    pub themes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WallpaperInfo {
    pub name: String,
    /// base00..base0F (hex without #) plus border, accent2.
    pub colors: std::collections::BTreeMap<String, String>,
}

// ---------------------------------------------------------------- progress

/// Sent by the engine while it runs.
#[derive(Clone, Debug)]
pub enum Event {
    /// Step `index` (into [`super::engine::steps`]) changed state.
    Step { index: usize, state: StepState },
    /// One line of output (command lines are prefixed with "$ ").
    Log(String),
    /// Finished; the summary screen shows this.
    Done(Outcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepState {
    Running,
    Done,
    Skipped,
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// age recipient of the host key (for .sops.yaml).
    pub age_key: String,
    /// True when the old host key was carried over (secrets already work).
    pub host_key_carried: bool,
    /// What to do next, one plain sentence each (BIOS Secure Boot steps, secrets, ...).
    pub next_steps: Vec<String>,
}

// ---------------------------------------------------------------- shared with Nix

/// Working directory of the installer (RAM on the ISO).
pub const WORK: &str = "/tmp/vx-install";
/// The repo copy that gets edited and installed from.
pub const WORK_REPO: &str = "/tmp/vx-install/nixos";
/// Disk passphrase for disko (`passwordFile` of the system LUKS), no newline.
pub const DISK_KEY: &str = "/tmp/vx-install/disk.key";
/// Format-time key files of encrypted data drives: <dir>/<name>.key.
pub const DATA_KEYS: &str = "/tmp/vx-install/keys";
/// Where the previous install is mounted read-only.
pub const OLD_MOUNT: &str = "/run/vx-install/old";
/// Data drive key files in the installed system: /persistent/etc/keys/<name>.key.
pub const INSTALLED_KEYS: &str = "/persistent/etc/keys";
/// One-time key the first boot uses to enroll the TPM, then removes.
pub const TPM_ENROLL_KEY: &str = "/persistent/install/tpm-enroll.key";
/// Disk choices for the Nix side, relative to the repo.
pub const DISKS_JSON: &str = "modules/hosts/arc/disks.json";

/// The shape of disks.json (written by the engine, read by _disko.nix).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisksJson {
    pub system: SystemDisk,
    pub data: Vec<DataDisk>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disks_json_shape() {
        let d = DisksJson {
            system: SystemDisk { device: "/dev/disk/by-id/nvme-X".into(), encrypt: true, tpm: true, swap_gb: 16 },
            data: vec![
                DataDisk {
                    name: "games".into(),
                    mount: "/home/vainnor/games".into(),
                    mode: DataMode::Format { device: "/dev/disk/by-id/nvme-Y".into(), encrypt: true },
                },
                DataDisk {
                    name: "d".into(),
                    mount: "/mnt/d".into(),
                    mode: DataMode::Mount { uuid: "ABCD".into(), fs_type: "ntfs3".into(), read_only: false },
                },
            ],
        };
        let j = serde_json::to_value(&d).unwrap();
        assert_eq!(j["system"]["swap_gb"], 16);
        assert_eq!(j["data"][0]["mode"], "format");
        assert_eq!(j["data"][1]["fsType"], "ntfs3");
        assert_eq!(j["data"][1]["readOnly"], false);
        let back: DisksJson = serde_json::from_value(j).unwrap();
        assert_eq!(back, d);
    }
}
