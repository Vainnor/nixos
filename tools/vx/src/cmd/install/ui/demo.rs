//! `--demo`: a fake machine resembling the real PC, fake install data, and an
//! install that only pretends (progress events on a timer). Nothing is read
//! from or written to any disk.

use super::super::plan::{
    Carry, DataMode, DiskInfo, DiskKind, Event, HomePlan, InstallData, Machine, Outcome, PartInfo, Plan, Previous, Secret,
    Settings, StepState, Unlocked, WallpaperInfo, OLD_MOUNT, WORK_REPO,
};
use super::super::probe::{self, BackupTarget};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread::sleep;
use std::time::Duration;

/// Shorter waits in tests.
fn pause(ms: u64) {
    if !cfg!(test) {
        sleep(Duration::from_millis(ms));
    }
}

pub fn machine() -> Machine {
    Machine { root: true, uefi: true, online: true, secure_boot: false, setup_mode: true, tpm: true, ram_gb: 64 }
}

fn part(path: &str, fstype: Option<&str>, size: u64, label: Option<&str>, partlabel: Option<&str>, uuid: Option<&str>) -> PartInfo {
    PartInfo {
        path: path.into(),
        uuid: uuid.map(Into::into),
        label: label.map(Into::into),
        partlabel: partlabel.map(Into::into),
        fstype: fstype.map(Into::into),
        size,
        mountpoint: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn disk(path: &str, id: &str, model: &str, size: u64, tran: &str, rm: bool, kind: DiskKind, parts: Vec<PartInfo>) -> DiskInfo {
    DiskInfo {
        path: path.into(),
        by_id: Some(format!("/dev/disk/by-id/{id}")),
        model: model.into(),
        serial: id.rsplit('_').next().unwrap_or_default().into(),
        size,
        transport: tran.into(),
        removable: rm,
        kind,
        partitions: parts,
    }
}

const TB1: u64 = 1_000_204_886_016;
const TB2: u64 = 2_000_398_934_016;

/// The PC as the installer sees it, plus a USB SSD to back /home up to.
pub fn disks() -> Vec<DiskInfo> {
    let mut usb = disk(
        "/dev/sdb",
        "usb-SanDisk_Ultra_4C530001231115117283-0:0",
        "SanDisk Ultra",
        30_752_636_928,
        "usb",
        true,
        DiskKind::Installer,
        vec![
            part("/dev/sdb1", Some("iso9660"), 1_221_591_040, Some("nixos-minimal-25.05-x86_64"), None, None),
            part("/dev/sdb2", Some("vfat"), 3_145_728, Some("EFIBOOT"), None, Some("1234-5678")),
        ],
    );
    usb.partitions[0].mountpoint = Some("/iso".into());
    vec![
        disk(
            "/dev/nvme0n1",
            "nvme-CT1000P310SSD8_24304A1B2C3D",
            "CT1000P310SSD8",
            TB1,
            "nvme",
            false,
            DiskKind::NixOs,
            vec![
                part("/dev/nvme0n1p1", Some("vfat"), 1 << 30, None, Some("disk-main-ESP"), Some("9A1B-2C3D")),
                part(
                    "/dev/nvme0n1p2",
                    Some("crypto_LUKS"),
                    999_129_399_296,
                    None,
                    Some("disk-main-luks"),
                    Some("0f6a3c1e-7d42-4a5e-9b1f-3c2d1e0a9b8c"),
                ),
            ],
        ),
        disk(
            "/dev/nvme1n1",
            "nvme-Sabrent_Rocket_4.0_2TB_A1B2C3D4E5F6G7H8",
            "Sabrent Rocket 4.0 2TB",
            TB2,
            "nvme",
            false,
            DiskKind::Windows { bitlocker: false, system: true },
            vec![
                part("/dev/nvme1n1p1", Some("vfat"), 104_857_600, None, Some("EFI system partition"), Some("4C2E-1F0A")),
                part("/dev/nvme1n1p2", None, 16_777_216, None, Some("Microsoft reserved partition"), None),
                part("/dev/nvme1n1p3", Some("ntfs"), 1_999_634_579_456, None, Some("Basic data partition"), Some("7A3E5B2C3E5AE7F1")),
                part("/dev/nvme1n1p4", Some("ntfs"), 643_825_664, None, None, Some("2A4C6E8F0A1B3C5D")),
            ],
        ),
        disk(
            "/dev/nvme2n1",
            "nvme-Sabrent_48790123456789012345",
            "Sabrent",
            TB1,
            "nvme",
            false,
            DiskKind::Windows { bitlocker: false, system: false },
            vec![
                part("/dev/nvme2n1p1", None, 16_777_216, None, Some("Microsoft reserved partition"), None),
                part("/dev/nvme2n1p2", Some("ntfs"), 1_000_186_314_752, Some("L"), Some("Basic data partition"), Some("B2D4F6A8C0E1F3A5")),
            ],
        ),
        disk(
            "/dev/sda",
            "ata-ST2000DM008-2UB102_ZFL4ABCD",
            "ST2000DM008-2UB102",
            TB2,
            "sata",
            false,
            DiskKind::Windows { bitlocker: false, system: false },
            vec![
                part("/dev/sda1", None, 16_777_216, None, Some("Microsoft reserved partition"), None),
                part("/dev/sda2", Some("ntfs"), 2_000_381_018_112, Some("Data"), Some("Basic data partition"), Some("5E8C2F1A8C2EEE4B")),
            ],
        ),
        usb,
        disk(
            "/dev/sdc",
            "usb-Samsung_PSSD_T7_S6WBNS0T123456A-0:0",
            "Samsung PSSD T7",
            TB1,
            "usb",
            false,
            DiskKind::Other,
            vec![part("/dev/sdc1", Some("exfat"), TB1 - 1_048_576, Some("T7"), None, Some("6F2A-91C4"))],
        ),
    ]
}

fn wallpaper(name: &str, hex: &str, border: &str) -> WallpaperInfo {
    let keys = [
        "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08", "base09", "base0A", "base0B",
        "base0C", "base0D", "base0E", "base0F",
    ];
    let mut colors: BTreeMap<String, String> =
        keys.iter().zip(hex.split_whitespace()).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    colors.insert("border".into(), border.into());
    WallpaperInfo { name: name.into(), colors }
}

/// install-data.json as the flake builds it (the real wallpapers' palettes).
pub fn data() -> InstallData {
    InstallData {
        host: "arc".into(),
        repo_url: "https://github.com/vainnor/nixos".into(),
        flake: "/nix/store/0000000000000000000000000000demo-source".into(),
        defaults: Settings {
            hostname: "arc".into(),
            username: "vainnor".into(),
            time_zone: "America/Chicago".into(),
            wallpaper: "eye".into(),
            theme: "match".into(),
        },
        wallpapers: vec![
            wallpaper(
                "eye",
                "0A0A0C 17171C 202024 6E6E77 9B9BA1 D8D8DA E8E8E8 F5F5F5 D47D7D D19A75 D1B675 91C577 70C2C2 5B9BEA B28CD9 D9605A",
                "38383F",
            ),
            wallpaper(
                "dolomites",
                "121410 1B1E18 252822 73786D 9EA29A D9DAD8 E8E9E8 F5F5F5 DE7373 DC986A DCBA6A 8ECE6E 66CCCC EFA672 B283E2 8FA9B8",
                "3B3F37",
            ),
            wallpaper(
                "lagoon",
                "0B1516 112021 1A2C2E 618185 92A8AA D5DBDC E6E9EA F4F5F5 DE7373 DC986A DCBA6A 8ECE6E 66CCCC 35C2B8 B283E2 8CCB94",
                "2D4345",
            ),
            wallpaper(
                "racing-green",
                "0B0F0E 131917 1C2422 6B7B77 99A3A1 D7DAD9 E7E9E8 F5F5F5 DE7373 DC986A DCBA6A 8ECE6E 66CCCC 5DBE92 B283E2 D2A86E",
                "313B38",
            ),
            wallpaper(
                "whitney",
                "0D1419 141D24 1C2830 627583 93A0A9 D6D9DC E7E8E9 F4F5F5 DE7373 DC986A DCBA6A 8ECE6E 66CCCC F0A075 B283E2 5FA3C6",
                "303E47",
            ),
        ],
        themes: vec!["vainnor-slate".into(), "caelus".into()],
    }
}

pub fn prepare_repo(update: bool) -> Result<PathBuf> {
    pause(if update { 1400 } else { 500 });
    Ok(PathBuf::from(WORK_REPO))
}

/// Any passphrase works except an empty one and "wrong" (to try the retry).
pub fn unlock(prev: &Previous, password: Option<&Secret>) -> Result<Unlocked> {
    pause(1200);
    if prev.encrypted && password.is_none_or(|p| p.0.is_empty() || p.0 == "wrong") {
        bail!("no key available with this passphrase: check it and try again");
    }
    Ok(Unlocked {
        previous: prev.clone(),
        persistent: PathBuf::from(format!("{OLD_MOUNT}/persistent")),
        home: PathBuf::from(format!("{OLD_MOUNT}/home")),
        available: Carry::ALL.into_iter().collect(),
        home_bytes: 312_400_000_000,
        users: vec!["vainnor".into()],
        swap_gb: 16,
    })
}

pub fn backup_targets(disks: &[DiskInfo], erased: &[String]) -> Vec<BackupTarget> {
    pause(600);
    probe::backup_candidates(disks, erased).into_iter().map(|t| BackupTarget { free: Some(t.size * 7 / 10), ..t }).collect()
}

fn short(dev: &str) -> &str {
    dev.rsplit('/').next().unwrap_or(dev)
}

pub fn steps(plan: &Plan) -> Vec<String> {
    let mut s = vec![];
    if let HomePlan::Backup { partition } = &plan.home {
        s.push(format!("back up /home to {}", short(partition)));
    }
    s.push("write the disk choices into the repo".into());
    if plan.home == HomePlan::Keep {
        s.push("clear the old system (keeping /home)".into());
    } else {
        s.push(format!("partition {}", short(&plan.system.device)));
    }
    for d in &plan.data {
        if let DataMode::Format { .. } = d.mode {
            s.push(format!("format data drive {}", d.name));
        }
    }
    if !plan.carry.is_empty() {
        s.push("carry over state from the old install".into());
    }
    s.push("set the login password".into());
    if !plan.carry.contains(&Carry::HostKey) {
        s.push("create the ssh host key".into());
    }
    s.push("install nixos".into());
    s.push("put the repo in ~/nixos".into());
    if plan.system.tpm {
        s.push("prepare tpm unlock".into());
    }
    s.push("finish".into());
    s
}

/// Pretend to install: a few log lines per step. VX_DEMO_FAIL=1 fails the install step.
pub fn run(plan: &Plan, tx: Sender<Event>) -> Result<()> {
    let fail = std::env::var("VX_DEMO_FAIL").is_ok_and(|v| v == "1");
    let send = |e: Event| tx.send(e).ok();
    for (i, name) in steps(plan).iter().enumerate() {
        send(Event::Step { index: i, state: StepState::Running });
        let lines: Vec<String> = if name.starts_with("partition") {
            vec![
                format!("$ wipefs -a {}", plan.system.device),
                "$ disko --mode destroy,format,mount /tmp/vx-install/nixos/modules/hosts/arc/_disko.nix".into(),
                "creating partition table".into(),
                "formatting ESP (vfat)".into(),
                if plan.system.encrypt { "creating luks container cryptroot".into() } else { "no encryption".into() },
                "creating btrfs subvolumes @nix @persistent @home @log @snapshots".into(),
            ]
        } else if name == "install nixos" {
            let mut v = vec![format!("$ nixos-install --flake {}#{} --no-root-passwd --no-channel-copy", plan.flake.display(), plan.host)];
            for p in ["linux-6.12.4", "mesa-25.0.1", "hyprland-0.47.2", "steam-1.0.0.82", "librewolf-134.0", "home-manager-generation"] {
                v.push(format!("copying path '/nix/store/{}-{p}' from 'https://cache.nixos.org'...", "x".repeat(32)));
            }
            v.push("installing the boot loader...".into());
            v
        } else {
            vec![format!("$ true  # {name}")]
        };
        for l in lines {
            pause(180);
            send(Event::Log(l));
        }
        pause(350);
        if fail && name == "install nixos" {
            let msg = "nixos-install exited with status 1".to_string();
            send(Event::Log(format!("error: {msg}")));
            send(Event::Step { index: i, state: StepState::Failed(msg.clone()) });
            bail!("{msg}");
        }
        let state = if name == "prepare tpm unlock" && !plan.system.encrypt { StepState::Skipped } else { StepState::Done };
        send(Event::Step { index: i, state });
    }
    let carried = plan.carry.contains(&Carry::HostKey);
    let mut next = vec!["remove the usb stick and reboot; if windows starts, pick the nixos drive with f11.".to_string()];
    if !plan.carry.contains(&Carry::SecureBoot) {
        next.push("secure boot: in the bios, reset to setup mode (delete keys), then boot nixos once: it enrolls its keys. then turn secure boot on.".into());
    }
    if !carried {
        next.push("secrets: on the mac run  vx secrets add-host <age key above>, push, then  vx switch  here.".into());
    }
    if !plan.carry.contains(&Carry::NetBird) {
        next.push("netbird: sudo netbird up".into());
    }
    next.push("github: gh auth login (so git push works)".into());
    send(Event::Done(Outcome {
        age_key: "age1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqs3290gq".into(),
        host_key_carried: carried,
        next_steps: next,
    }));
    Ok(())
}

/// For tests: the carry set a fresh demo unlock offers.
#[allow(dead_code)]
pub fn all_carry() -> BTreeSet<Carry> {
    Carry::ALL.into_iter().collect()
}
