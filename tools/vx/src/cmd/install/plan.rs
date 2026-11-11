//! The contract between the installer's screens (`ui`, `probe`) and the part
//! that does the work (`engine`). The UI fills in a [`Plan`]; the engine
//! carries it out and reports progress as [`Event`]s.
//!
//! Paths shared with the Nix side (modules/hosts/<name>/{host.json,disks.json,
//! _hardware.nix}, modules/system/secure-boot.nix) are the consts and
//! functions at the bottom. Change them in both places or not at all.

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

/// Restore /home from the homelab's restic backup, onto a /home that is made
/// fresh (never with [`HomePlan::Keep`]: that /home is already there).
pub fn restore_offered(home: &HomePlan) -> bool {
    *home != HomePlan::Keep
}

/// Where the restic login came from, in the order they're tried
/// (engine::restic::sources).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredSource {
    /// secrets.yaml (restic/*), decrypted with the carried-over host key.
    Sops,
    /// The 1Password item meta.restoreItem points at.
    OnePassword,
    /// Typed on the screen.
    Typed,
}

impl CredSource {
    pub fn label(self) -> &'static str {
        match self {
            CredSource::Sops => "the secrets (carried host key)",
            CredSource::OnePassword => "1password",
            CredSource::Typed => "typed in",
        }
    }
}

/// What restic needs to read the backup. Held in memory only; written to
/// [`RESTIC_WORK`] (RAM, 0600) just while restic runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResticCreds {
    /// e.g. rest:https://<server>:8000/<host>/ (sops restic/repository).
    pub repository: Secret,
    /// The repository's key (sops restic/password).
    pub password: Secret,
    /// Environment for restic, e.g. RESTIC_REST_USERNAME / RESTIC_REST_PASSWORD
    /// (sops restic/env, minus RESTIC_CACERT: the repo's pinned cert is used).
    pub env: Vec<(String, Secret)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeRestore {
    pub creds: ResticCreds,
    pub source: CredSource,
    /// The snapshot the screen found ("2026-11-09 03:41 from arc"), for the log.
    pub latest: String,
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

/// The graphics card a host is set up for (host.json `gpu`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gpu {
    Amd,
    Nvidia,
    Intel,
    #[default]
    None,
}

impl Gpu {
    pub const ALL: [Gpu; 4] = [Gpu::Amd, Gpu::Nvidia, Gpu::Intel, Gpu::None];

    pub fn name(self) -> &'static str {
        match self {
            Gpu::Amd => "amd",
            Gpu::Nvidia => "nvidia",
            Gpu::Intel => "intel",
            Gpu::None => "none",
        }
    }
}

/// Which machine of the flake this install is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostPlan {
    /// Reinstall a host that's already in the repo (keeps its _hardware.nix and host.json).
    Existing(String),
    /// A new folder modules/hosts/<name>/ with host.json, disks.json and _hardware.nix.
    New(NewHost),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewHost {
    /// `[a-z][a-z0-9-]{0,30}`, not `mac`, not an existing host. Also the hostname.
    pub name: String,
    /// flake.modules.nixos.* names; always starts with "base".
    pub aspects: Vec<String>,
    pub gpu: Gpu,
    /// system.stateVersion for the new host (install-data `stateVersion`).
    pub state_version: String,
}

impl HostPlan {
    /// nixosConfigurations attribute, folder name and hostname.
    pub fn name(&self) -> &str {
        match self {
            HostPlan::Existing(n) => n,
            HostPlan::New(h) => &h.name,
        }
    }

    pub fn is_new(&self) -> bool {
        matches!(self, HostPlan::New(_))
    }
}

/// How the install talks to GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitHubMode {
    /// The login owns the upstream repo: push there.
    Owner,
    /// Someone else: their fork, cloned and pushed to; meta.nix is personalised.
    Fork {
        /// https clone URL of the fork.
        url: String,
        /// The fork's default branch (pushed to).
        branch: String,
    },
    /// No GitHub: committed locally only.
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubPlan {
    /// The GitHub login (None when offline).
    pub login: Option<String>,
    /// Upstream `owner/repo` (what Owner mode pushes to).
    pub upstream: String,
    pub mode: GitHubMode,
    /// Push the install commit at the end (Owner may turn it off; always on for Fork).
    pub push: bool,
    /// Public SSH keys of the login (github.com/<login>.keys), for meta.nix in Fork mode.
    pub keys: Vec<String>,
    pub full_name: Option<String>,
    pub email: Option<String>,
}

impl GitHubPlan {
    pub fn offline(upstream: &str) -> Self {
        GitHubPlan {
            login: None,
            upstream: upstream.into(),
            mode: GitHubMode::Offline,
            push: false,
            keys: vec![],
            full_name: None,
            email: None,
        }
    }

    /// Will the engine try to push?
    pub fn pushes(&self) -> bool {
        self.push && self.mode != GitHubMode::Offline
    }

    pub fn is_fork(&self) -> bool {
        matches!(self.mode, GitHubMode::Fork { .. })
    }
}

#[derive(Clone, Debug)]
pub struct Plan {
    /// The repo copy the installer edits and installs from ([`WORK_REPO`]).
    pub flake: PathBuf,
    /// Which machine: an existing host or a new one. `settings.hostname` is its name.
    pub host: HostPlan,
    pub github: GitHubPlan,
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
    /// The firmware was in Setup Mode when the installer started: a "secure
    /// boot" step enrolls the keys (checked again when it runs).
    pub setup_mode: bool,
    /// Logged in to NetBird on the installer: its [`NETBIRD_STATE`] is copied
    /// into the new system.
    pub netbird: bool,
    /// Restore /home/<user> from the homelab backup (only with [`restore_offered`]).
    pub restore: Option<HomeRestore>,
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
    /// Has a Wi-Fi card (the welcome screen offers to connect).
    pub wifi: bool,
    /// Total RAM in GiB (swap size hint).
    pub ram_gb: u32,
    /// The graphics card found by lspci (a new host's default).
    pub gpu: Gpu,
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
    /// Upstream GitHub owner and repo (from repoUrl). Empty in older data: see [`InstallData::upstream`].
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub repo: String,
    /// Hosts in the repo (folders with a host.json).
    #[serde(default)]
    pub hosts: Vec<HostInfo>,
    /// Features a new host can pick (flake.modules.nixos.*; `base` is always on).
    #[serde(default)]
    pub aspects: Vec<AspectInfo>,
    /// system.stateVersion for new hosts.
    #[serde(default, rename = "stateVersion")]
    pub state_version: String,
    /// meta.restoreItem: the 1Password item with the restic login
    /// ("op://<vault>/<item>"); None = don't offer 1Password.
    #[serde(default, rename = "restoreItem")]
    pub restore_item: Option<String>,
}

impl InstallData {
    /// (owner, repo) of upstream: the data's fields, else parsed from repo_url.
    pub fn upstream(&self) -> (String, String) {
        if !self.owner.is_empty() && !self.repo.is_empty() {
            return (self.owner.clone(), self.repo.clone());
        }
        parse_github_url(&self.repo_url).unwrap_or_else(|| (self.owner.clone(), self.repo.clone()))
    }
}

/// `https://github.com/<owner>/<repo>(.git)` (or git@github.com:owner/repo) -> (owner, repo).
pub fn parse_github_url(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("git@github.com:"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (o, r) = rest.split_once('/')?;
    (!o.is_empty() && !r.is_empty() && !r.contains('/')).then(|| (o.to_string(), r.to_string()))
}

#[derive(Clone, Debug, Deserialize)]
pub struct HostInfo {
    pub name: String,
    #[serde(default)]
    pub aspects: Vec<String>,
    #[serde(default)]
    pub gpu: Gpu,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AspectInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default: bool,
    /// Aspects this one needs on too (dev, gaming, streaming and vm: desktop).
    #[serde(default)]
    pub requires: Vec<String>,
}

/// Turn an aspect on or off in a new host's checklist, keeping `requires`:
/// on also turns on what it needs, off also turns off what needs it.
pub fn toggle_aspect(all: &[AspectInfo], on: &mut std::collections::BTreeSet<String>, name: &str) {
    if on.remove(name) {
        // Off: drop everything that (transitively) needs something now off.
        loop {
            let gone: Vec<String> = all
                .iter()
                .filter(|a| on.contains(&a.name) && a.requires.iter().any(|r| r != "base" && !on.contains(r)))
                .map(|a| a.name.clone())
                .collect();
            if gone.is_empty() {
                break;
            }
            for g in gone {
                on.remove(&g);
            }
        }
    } else {
        on.insert(name.to_string());
        with_requires(all, on);
    }
}

/// Add every aspect the chosen ones need (transitively).
pub fn with_requires(all: &[AspectInfo], on: &mut std::collections::BTreeSet<String>) {
    loop {
        let need: Vec<String> = all
            .iter()
            .filter(|a| on.contains(&a.name))
            .flat_map(|a| a.requires.iter())
            .filter(|r| *r != "base" && !on.contains(*r))
            .cloned()
            .collect();
        if need.is_empty() {
            break;
        }
        on.extend(need);
    }
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
    /// Where the install commit was pushed (`owner/repo branch`), None when it wasn't.
    pub pushed: Option<String>,
    /// Why it wasn't pushed (offline, failed, turned off), and what to do about it.
    pub not_pushed: Option<String>,
    /// Secure Boot keys were enrolled in the firmware by the installer.
    pub sb_enrolled: bool,
    /// Secrets are on but this new host isn't a recipient yet: another machine
    /// re-encrypts them, then this one switches by itself.
    pub secrets_pending: bool,
    /// NetBird login carried or made on the installer: "ip (fqdn)" when known.
    pub netbird: Option<String>,
    /// /home was restored from the homelab backup: what (the snapshot).
    pub home_restored: Option<String>,
    /// The restore was chosen but didn't finish: why (a next step says how to redo it).
    pub restore_failed: Option<String>,
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
/// gh's config dir on the installer (hosts.yml holds the login; RAM).
pub const GH_CONFIG: &str = "/tmp/vx-install/gh";
/// restic's repository and password files while it runs (tmpfs, 0700/0600; removed after).
pub const RESTIC_WORK: &str = "/run/vx-install/restic";
/// The 1Password CLI's config (its sign-in) on the installer (tmpfs; removed once read).
pub const OP_CONFIG: &str = "/run/vx-install/op";
/// The homelab rest-server's pinned certificate, relative to the repo (public;
/// installed as /etc/restic/homelab-ca.pem by system/backups.nix).
pub const RESTIC_CA_REL: &str = "assets/restic/homelab-ca.pem";

/// NetBird's state directory, on the installer and in the installed system
/// (services.netbird client "default": dir.state, NB_CONFIG = <it>/config.json).
/// The installer logs in with the ISO's own daemon, runs `netbird down`, then
/// copies this whole folder to /persistent<it>.
pub const NETBIRD_STATE: &str = "/var/lib/netbird";
/// The login file in NETBIRD_STATE.
pub const NETBIRD_CONFIG: &str = "config.json";
/// The new host's SSH host key, made before the settings commit (its age
/// recipient goes into .sops.yaml), then copied to /persistent/etc/ssh.
pub const HOST_KEY_WORK: &str = "/tmp/vx-install/hostkey";
/// sbctl's keys on the installer (/etc/sbctl/sbctl.conf there; lanzaboote's
/// pkiBundle on the installed system), copied to the new system's
/// /persistent/var/lib/sbctl and /var/lib/sbctl.
pub const SBCTL_DIR: &str = "/var/lib/sbctl";
/// "This host has its secrets": the vx-secrets-wait timer runs until it
/// exists. The installer writes it when the old host key was carried over;
/// `vx secrets wait` (and the unit) write it after switching.
pub const SECRETS_OK: &str = "/persistent/install/secrets-ok";

/// A host's folder, relative to the repo.
pub fn host_dir(host: &str) -> String {
    format!("modules/hosts/{host}")
}
/// Disk choices for the Nix side (read by the shared disko module), relative to the repo.
pub fn disks_json_path(host: &str) -> String {
    format!("modules/hosts/{host}/disks.json")
}
/// A host's description (makes it a nixosConfiguration), relative to the repo.
pub fn host_json_path(host: &str) -> String {
    format!("modules/hosts/{host}/host.json")
}
/// nixos-generate-config's hardware part, relative to the repo.
pub fn hardware_path(host: &str) -> String {
    format!("modules/hosts/{host}/_hardware.nix")
}

/// The shape of host.json (written by the engine for new hosts, read by the flake).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostJson {
    pub name: String,
    pub system: String,
    #[serde(rename = "stateVersion")]
    pub state_version: String,
    pub aspects: Vec<String>,
    pub gpu: Gpu,
    /// EDID model of the main screen; null = the first one.
    #[serde(rename = "primaryMonitor")]
    pub primary_monitor: Option<String>,
    /// PipeWire sink name; null = none picked.
    #[serde(rename = "audioOutput")]
    pub audio_output: Option<String>,
    #[serde(rename = "replayMonitor")]
    pub replay_monitor: String,
}

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

    #[test]
    fn host_json_shape() {
        let h = HostJson {
            name: "forge".into(),
            system: "x86_64-linux".into(),
            state_version: "26.11".into(),
            aspects: vec!["base".into(), "desktop".into()],
            gpu: Gpu::Amd,
            primary_monitor: None,
            audio_output: None,
            replay_monitor: "screen".into(),
        };
        let j = serde_json::to_value(&h).unwrap();
        assert_eq!(
            j,
            serde_json::json!({ "name": "forge", "system": "x86_64-linux", "stateVersion": "26.11",
                "aspects": ["base", "desktop"], "gpu": "amd", "primaryMonitor": null, "audioOutput": null,
                "replayMonitor": "screen" })
        );
        assert_eq!(disks_json_path("forge"), "modules/hosts/forge/disks.json");
    }

    #[test]
    fn old_install_data_still_parses() {
        let d: InstallData = serde_json::from_str(
            r#"{"host":"arc","repoUrl":"https://github.com/vainnor/nixos.git","flake":"/nix/store/x",
               "defaults":{"hostname":"arc","username":"vainnor","timeZone":"UTC","wallpaper":"eye","theme":"match"},
               "wallpapers":[],"themes":[]}"#,
        )
        .unwrap();
        assert!(d.hosts.is_empty() && d.aspects.is_empty());
        assert_eq!(d.upstream(), ("vainnor".to_string(), "nixos".to_string()));
        let d: InstallData = serde_json::from_str(
            r#"{"host":"arc","repoUrl":"x","flake":"/nix/store/x","owner":"vainnor","repo":"nixos","stateVersion":"26.11",
               "defaults":{"hostname":"arc","username":"vainnor","timeZone":"UTC","wallpaper":"eye","theme":"match"},
               "wallpapers":[],"themes":[],"hosts":[{"name":"arc","aspects":["base","gaming"],"gpu":"amd"}],
               "aspects":[{"name":"gaming","description":"steam","default":false,"requires":["desktop"]}],
               "githubUser":"Vainnor"}"#,
        )
        .unwrap();
        assert_eq!(d.hosts[0].gpu, Gpu::Amd);
        assert_eq!(d.state_version, "26.11");
        assert!(!d.aspects[0].default);
        assert_eq!(d.aspects[0].requires, ["desktop"]);
    }

    #[test]
    fn aspect_requires() {
        let a = |n: &str, r: &[&str]| AspectInfo {
            name: n.into(),
            description: String::new(),
            default: false,
            requires: r.iter().map(|s| s.to_string()).collect(),
        };
        let all = [a("desktop", &[]), a("dev", &["desktop"]), a("gaming", &["desktop"]), a("privacy", &[]), a("x", &["gaming"])];
        let mut on = std::collections::BTreeSet::new();
        let names = |on: &std::collections::BTreeSet<String>| on.iter().cloned().collect::<Vec<_>>().join(" ");
        toggle_aspect(&all, &mut on, "x");
        assert_eq!(names(&on), "desktop gaming x", "on pulls in what it needs, transitively");
        toggle_aspect(&all, &mut on, "dev");
        toggle_aspect(&all, &mut on, "privacy");
        toggle_aspect(&all, &mut on, "desktop");
        assert_eq!(names(&on), "privacy", "off drops everything that needs it");
        toggle_aspect(&all, &mut on, "desktop");
        toggle_aspect(&all, &mut on, "gaming");
        toggle_aspect(&all, &mut on, "x");
        toggle_aspect(&all, &mut on, "gaming");
        assert_eq!(names(&on), "desktop privacy");
        let mut on: std::collections::BTreeSet<String> = ["dev".to_string()].into();
        with_requires(&all, &mut on);
        assert_eq!(names(&on), "desktop dev");
    }

    #[test]
    fn github_urls() {
        let p = |s: &str| parse_github_url(s);
        assert_eq!(p("https://github.com/vainnor/nixos"), Some(("vainnor".into(), "nixos".into())));
        assert_eq!(p("git@github.com:someone/nixos.git"), Some(("someone".into(), "nixos".into())));
        assert_eq!(p("https://gitlab.com/a/b"), None);
        assert_eq!(p("https://github.com/a"), None);
    }
}
