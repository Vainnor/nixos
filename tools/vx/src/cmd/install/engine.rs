//! Does the install described by a Plan.
//!
//! Every external command goes through one runner ([`runner::Runner`]) that,
//! in dry-run/demo mode, only logs "$ cmd args" instead of running it.
//!
//! - `runner`   runs commands and writes files, honouring [`Mode`]
//! - `disk`     lsblk, mounts, the safety checks
//! - `previous` unlocking a previous install read-only, what it can carry
//! - `settings` meta.nix / host file / disks.json edits
//! - `steps`    the install steps
//! - `finish`   the summary's next steps

mod disk;
mod finish;
mod previous;
mod runner;
mod settings;
mod steps;

use super::plan::{Event, InstallData, Plan, Previous, Secret, StepState, Unlocked, WORK, WORK_REPO};
use anyhow::{Context, Result, bail};
use runner::{Cmd, Runner};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// How commands are executed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Real,
    /// Log commands, run nothing, invent plausible results.
    DryRun,
}

/// Load install-data.json from `path`, or build it from the repo at `repo`
/// (`nix build <repo>#install-data`) when `path` is None.
pub fn load_data(path: Option<&Path>, repo: Option<&Path>) -> Result<InstallData> {
    let file = match (path, repo) {
        (Some(p), _) => p.to_path_buf(),
        (None, Some(repo)) => {
            let r = Runner::new(Mode::Real, None);
            let out = r.query(
                Cmd::new("nix")
                    .args(["--extra-experimental-features", steps::NIX_FEATURES, "build", "--no-link", "--print-out-paths"])
                    .arg(format!("{}#install-data", repo.display())),
            )?;
            let p = PathBuf::from(out.lines().last().unwrap_or("").trim());
            if p.is_dir() { p.join("install-data.json") } else { p }
        }
        (None, None) => bail!("no install data: pass --data <install-data.json>, or run from the repo (--repo)"),
    };
    let text = std::fs::read_to_string(&file).with_context(|| format!("could not read {}", file.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not valid install data", file.display()))
}

/// Make WORK_REPO: a fresh `git clone` of data.repo_url when `update` (and
/// online), otherwise a writable copy of data.flake. Returns the path.
pub fn prepare_repo(data: &InstallData, update: bool, mode: Mode) -> Result<PathBuf> {
    let r = Runner::new(mode, None);
    let repo = Path::new(WORK_REPO);
    r.mkdir(Path::new(WORK), 0o700)?;
    r.remove(repo)?;
    r.remove(Path::new(steps::SNAPSHOT_MARK))?;
    if update {
        let cloned = r.check(
            Cmd::new("git").args(["clone", "-q", &data.repo_url, WORK_REPO]).env("GIT_TERMINAL_PROMPT", "0"),
            true,
        )?;
        if cloned {
            return Ok(repo.to_path_buf());
        }
        r.remove(repo)?;
    }
    // A snapshot of the ISO's copy (a read-only store path) with its own history.
    r.run(Cmd::new("cp").args(["-rT", &data.flake, WORK_REPO]))?;
    r.run(Cmd::new("chmod").args(["-R", "u+w", WORK_REPO]))?;
    let git = || Cmd::new("git").args(["-C", WORK_REPO]);
    r.run(git().args(["init", "-q", "-b", "main"]))?;
    r.run(git().args(["remote", "add", "origin", &data.repo_url]))?;
    r.run(git().args(["add", "-A"]))?;
    r.run(git().args([
        "-c",
        "user.name=vx install",
        "-c",
        "user.email=vx-install@localhost",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "-m",
        &format!("snapshot of {} (history is separate from {})", data.flake, data.repo_url),
    ]))?;
    r.write(Path::new(steps::SNAPSHOT_MARK), b"", 0o644)?;
    Ok(repo.to_path_buf())
}

/// Unlock (when encrypted) and mount a previous install read-only under
/// OLD_MOUNT, and list what can be carried over.
pub fn unlock_previous(prev: &Previous, password: Option<&Secret>, mode: Mode) -> Result<Unlocked> {
    previous::unlock(prev, password, &Runner::new(mode, None))
}

/// Unmount and close what unlock_previous opened (safe to call when nothing is open).
pub fn close_previous(mode: Mode) -> Result<()> {
    previous::close(&Runner::new(mode, None))
}

/// The step names for this plan, in order (shown on the progress screen).
pub fn steps(plan: &Plan) -> Vec<String> {
    steps::plan_steps(plan).into_iter().map(|s| s.name(plan).to_string()).collect()
}

/// Run the plan, sending Events; ends with Event::Done on success. Blocking:
/// the UI calls it on its own thread.
pub fn run(plan: &Plan, tx: Sender<Event>, mode: Mode) -> Result<()> {
    let mut run = steps::Run::new(plan, Runner::new(mode, Some(tx)));
    let list = steps::plan_steps(plan);
    for (index, step) in list.iter().enumerate() {
        run.r.send(Event::Step { index, state: StepState::Running });
        if let Err(e) = run.step(*step) {
            let msg = format!("{e:#}");
            run.r.log(format!("error: {msg}"));
            run.r.send(Event::Step { index, state: StepState::Failed(first_line(&msg)) });
            for later in index + 1..list.len() {
                run.r.send(Event::Step { index: later, state: StepState::Skipped });
            }
            if *step == steps::Step::Check {
                run.r.log("nothing was changed");
            } else {
                steps::after_failure(&run.r);
            }
            return Err(e);
        }
        run.r.send(Event::Step { index, state: StepState::Done });
    }
    let outcome = run.outcome.take().unwrap_or_default();
    run.r.send(Event::Done(outcome));
    Ok(())
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::plan::*;
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::mpsc;

    const DISK_PW: &str = "disk-pass-Zq9";
    const USER_PW: &str = "login-pass-Xk3";

    pub fn plan_fresh() -> Plan {
        Plan {
            flake: PathBuf::from("/nonexistent/vx-install/nixos"),
            host: "arc".into(),
            system: SystemDisk { device: "/dev/disk/by-id/nvme-SYS".into(), encrypt: true, tpm: true, swap_gb: 16 },
            data: vec![
                DataDisk {
                    name: "games".into(),
                    mount: "/home/vainnor/games".into(),
                    mode: DataMode::Format { device: "/dev/disk/by-id/nvme-GAMES".into(), encrypt: true },
                },
                DataDisk {
                    name: "d".into(),
                    mount: "/mnt/d".into(),
                    mode: DataMode::Mount { uuid: "ABCD".into(), fs_type: "ntfs3".into(), read_only: false },
                },
            ],
            settings: Settings {
                hostname: "arc".into(),
                username: "vainnor".into(),
                time_zone: "America/Chicago".into(),
                wallpaper: "eye".into(),
                theme: "match".into(),
            },
            disk_password: Some(Secret(DISK_PW.into())),
            user_password: Some(Secret(USER_PW.into())),
            previous: None,
            carry: BTreeSet::new(),
            home: HomePlan::Fresh,
        }
    }

    fn unlocked() -> Unlocked {
        Unlocked {
            previous: Previous { disk: "/dev/disk/by-id/nvme-SYS".into(), root_part: "/dev/nvme0n1p2".into(), encrypted: true },
            persistent: previous::persistent(),
            home: previous::home(),
            available: Carry::ALL.into_iter().collect(),
            home_bytes: 120_000_000_000,
            users: vec!["vainnor".into()],
            swap_gb: 0,
        }
    }

    pub fn plan_keep() -> Plan {
        let mut p = plan_fresh();
        p.previous = Some(unlocked());
        p.carry = Carry::ALL.into_iter().collect();
        p.home = HomePlan::Keep;
        p.user_password = None;
        p.system.swap_gb = 0;
        p
    }

    fn names(p: &Plan) -> Vec<String> {
        steps(p)
    }

    #[test]
    fn steps_fresh() {
        let mut p = plan_fresh();
        p.data.clear();
        p.system.tpm = false;
        assert_eq!(
            names(&p),
            [
                "check the drives",
                "write settings into the repo",
                "partition and format",
                "set up passwords and keys",
                "install the system",
                "put the repo in your home",
                "finish"
            ]
        );
    }

    #[test]
    fn steps_keep_and_backup() {
        let p = plan_keep();
        assert_eq!(
            names(&p),
            [
                "check the drives",
                "save what to carry over",
                "write settings into the repo",
                "replace the system, keep /home",
                "restore carried state",
                "install the system",
                "put the repo in your home",
                "finish"
            ]
        );
        let mut b = plan_keep();
        b.home = HomePlan::Backup { partition: "/dev/sdb2".into() };
        let n = names(&b);
        assert_eq!(n[1..4], ["save what to carry over", "back up /home", "write settings into the repo"]);
        assert_eq!(n[4], "partition and format");
        // A previous install with nothing carried still gets closed, but nothing is "restored".
        let mut c = plan_fresh();
        c.previous = Some(unlocked());
        let n = names(&c);
        assert!(n.contains(&"save what to carry over".to_string()));
        assert!(n.contains(&"set up passwords and keys".to_string()));
    }

    /// Run in DryRun, return the log lines and the events.
    fn dry(plan: &Plan) -> (Vec<String>, Vec<Event>, Result<()>) {
        let (tx, rx) = mpsc::channel();
        let res = run(plan, tx, Mode::DryRun);
        let events: Vec<Event> = rx.iter().collect();
        if std::env::var_os("VX_SHOW_LOG").is_some() {
            for e in &events {
                eprintln!("{e:?}");
            }
        }
        let logs = events.iter().filter_map(|e| if let Event::Log(s) = e { Some(s.clone()) } else { None }).collect();
        (logs, events, res)
    }

    fn cmds(logs: &[String]) -> Vec<String> {
        logs.iter().filter_map(|l| l.strip_prefix("$ ").map(str::to_string)).collect()
    }

    /// Indexes of the first command starting with each prefix; asserts they're in order.
    fn assert_order(cmds: &[String], prefixes: &[&str]) {
        let mut last = 0;
        for p in prefixes {
            let i = cmds.iter().skip(last).position(|c| c.starts_with(p)).map(|i| i + last);
            let Some(i) = i else { panic!("no `{p}` after #{last} in:\n{}", cmds.join("\n")) };
            last = i;
        }
    }

    fn no_secrets(logs: &[String]) {
        for l in logs {
            assert!(!l.contains(DISK_PW) && !l.contains(USER_PW), "secret in log: {l}");
        }
    }

    #[test]
    fn dry_run_fresh() {
        let plan = plan_fresh();
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        no_secrets(&logs);
        let c = cmds(&logs);
        assert_order(
            &c,
            &[
                "findmnt -n -o SOURCE /iso",
                "lsblk -J -p -o NAME,TYPE,FSTYPE,PARTLABEL,MOUNTPOINTS /dev/disk/by-id/nvme-SYS",
                "git -C /nonexistent/vx-install/nixos add -- modules/flake/meta.nix modules/hosts/arc/default.nix modules/hosts/arc/disks.json",
                "git -C /nonexistent/vx-install/nixos -c user.name=vainnor",
                "nix --extra-experimental-features 'nix-command flakes' build --no-link --print-out-paths /nonexistent/vx-install/nixos#nixosConfigurations.arc.config.system.build.diskoScript",
                "/nix/store/00000000000000000000000000000000-disko",
                "mkpasswd -m yescrypt --stdin < (stdin)",
                "ssh-keygen -q -t ed25519 -N '' -C root@arc -f /mnt/persistent/etc/ssh/ssh_host_ed25519_key",
                "ssh-to-age < (stdin)",
                "lsblk -J -p -o NAME,TYPE,FSTYPE,PARTLABEL,MOUNTPOINTS /dev/disk/by-id/nvme-SYS",
                "cryptsetup luksAddKey --key-file /tmp/vx-install/disk.key /dev/disk/by-id/nvme-SYS-part2 /mnt/persistent/install/tpm-enroll.key",
                "nixos-install --flake /nonexistent/vx-install/nixos#arc --no-root-passwd --no-channel-copy",
                "cp -a /nonexistent/vx-install/nixos /mnt/home/vainnor/nixos",
                "nixos-enter --root /mnt -c 'chown -R vainnor:users /home/vainnor/nixos && chown vainnor:users /home/vainnor && chmod 700 /home/vainnor && chown vainnor:users /home/vainnor/games'",
                "umount -R /mnt",
            ],
        );
        assert!(!c.iter().any(|c| c.contains("diskoDataScript") || c.contains("btrfs subvolume")));
        assert!(logs.contains(&"write /tmp/vx-install/disk.key (secret, 600)".to_string()));
        assert!(logs.contains(&"write /tmp/vx-install/keys/games.key (64 random bytes, 600)".to_string()));
        assert!(logs.contains(&"copy /tmp/vx-install/keys/games.key -> /mnt/persistent/etc/keys/games.key (400)".to_string()));
        assert!(logs.iter().any(|l| l.starts_with("write /mnt/persistent/passwords/vainnor (secret")));

        // Every step ran and it ended in Done.
        let n = steps(&plan).len();
        let done = events.iter().filter(|e| matches!(e, Event::Step { state: StepState::Done, .. })).count();
        assert_eq!(done, n);
        let Some(Event::Done(o)) = events.last() else { panic!("didn't end in Done: {:?}", events.last()) };
        assert!(o.age_key.starts_with("age1"));
        assert!(!o.host_key_carried);
        assert!(o.next_steps.iter().any(|s| s.contains("vx secrets add-host age1")));
        assert!(o.next_steps.iter().any(|s| s == "sudo netbird up"));
    }

    #[test]
    fn dry_run_keep() {
        let plan = plan_keep();
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        no_secrets(&logs);
        let c = cmds(&logs);
        assert_order(
            &c,
            &[
                "rsync -aHAX --exclude=/install /run/vx-install/old/persistent/ /tmp/vx-install/carry/",
                "umount /run/vx-install/old/top",
                "cryptsetup close vx-old",
                "git -C /nonexistent/vx-install/nixos add",
                "mount -t btrfs -o subvolid=5 /dev/mapper/cryptroot /tmp/vx-install/top",
                "btrfs subvolume delete /tmp/vx-install/top/@persistent",
                "btrfs subvolume create /tmp/vx-install/top/@persistent",
                "umount /tmp/vx-install/top",
                "mount -t tmpfs -o size=25%,mode=755 none /mnt",
                "mount -t btrfs -o subvol=@nix,compress=zstd,noatime /dev/mapper/cryptroot /mnt/nix",
                "mount -t btrfs -o subvol=@persistent,compress=zstd,noatime /dev/mapper/cryptroot /mnt/persistent",
                "mount -t btrfs -o subvol=@home,compress=zstd,noatime /dev/mapper/cryptroot /mnt/home",
                "mount -t btrfs -o subvol=@log,compress=zstd,noatime /dev/mapper/cryptroot /mnt/var/log",
                "mount -t vfat -o umask=0077 /dev/disk/by-id/nvme-SYS-part1 /mnt/boot",
                "nix --extra-experimental-features 'nix-command flakes' build --no-link --print-out-paths /nonexistent/vx-install/nixos#nixosConfigurations.arc.config.system.build.diskoDataScript",
                "/nix/store/00000000000000000000000000000000-disko-data",
                "rsync -aHAX /tmp/vx-install/carry/ /mnt/persistent/",
                "ssh-to-age",
                "rsync -aHAX /mnt/persistent/var/lib/sbctl/ /mnt/var/lib/sbctl/",
                "cryptsetup luksAddKey --key-file /tmp/vx-install/disk.key /dev/nvme0n1p2 /mnt/persistent/install/tpm-enroll.key",
                "nixos-install",
                "cp -a",
                "nixos-enter --root /mnt -c 'chown -R vainnor:users /home/vainnor/nixos && chown vainnor:users /home/vainnor && chown vainnor:users /home/vainnor/games'",
                "umount -R /mnt",
            ],
        );
        // Carried: no new password, no new host key; old ~/nixos moved aside.
        assert!(!c.iter().any(|c| c.starts_with("mkpasswd") || c.starts_with("ssh-keygen") || c.contains("diskoScript ")));
        assert!(logs.iter().any(|l| l.starts_with("move /mnt/home/vainnor/nixos -> /mnt/home/vainnor/nixos.before-reinstall-")));
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert!(o.host_key_carried);
        assert_eq!(o.next_steps[1], "turn secure boot back on in the bios: your keys are still enrolled");
        assert!(!o.next_steps.iter().any(|s| s.contains("netbird") || s.contains("add-host")));
    }

    #[test]
    fn dry_run_backup_without_data() {
        let mut plan = plan_keep();
        plan.home = HomePlan::Backup { partition: "/dev/sdb2".into() };
        plan.data.clear();
        plan.system.tpm = false;
        plan.carry = [Carry::HostKey, Carry::Password].into_iter().collect();
        plan.settings.username = "carson".into();
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        let c = cmds(&logs);
        assert_order(
            &c,
            &[
                "rsync -aHAX --relative /run/vx-install/old/persistent/./etc/ssh /tmp/vx-install/carry/",
                "rsync -aHAX --relative /run/vx-install/old/persistent/./passwords /tmp/vx-install/carry/",
                "mount /dev/sdb2 /run/vx-install/backup",
                "df -B1 --output=avail /run/vx-install/backup",
                "rsync -aHAX --info=progress2 --no-inc-recursive /run/vx-install/old/home/ /run/vx-install/backup/nixos-home-backup-",
                "umount /run/vx-install/backup",
                "cryptsetup close vx-old",
                "nix --extra-experimental-features 'nix-command flakes' build --no-link --print-out-paths /nonexistent/vx-install/nixos#nixosConfigurations.arc.config.system.build.diskoScript",
            ],
        );
        assert!(!c.iter().any(|c| c.contains("luksAddKey") || c.contains("diskoDataScript")));
        // Username changed: the single old password file is renamed.
        assert!(logs.contains(&"move /mnt/persistent/passwords/vainnor -> /mnt/persistent/passwords/carson".to_string()));
        assert!(matches!(events.last(), Some(Event::Done(_))));
    }

    #[test]
    fn checks_fail_before_anything_changes() {
        let mut plan = plan_fresh();
        plan.disk_password = None;
        let (logs, events, res) = dry(&plan);
        assert!(res.unwrap_err().to_string().contains("disk password"));
        let c = cmds(&logs);
        assert!(!c.iter().any(|c| c.starts_with("nix ") || c.starts_with("git ") || c.starts_with("/nix/store")));
        assert!(events.iter().any(|e| matches!(e, Event::Step { index: 0, state: StepState::Failed(_) })));
        assert!(!events.iter().any(|e| matches!(e, Event::Done(_))));

        let mut plan = plan_fresh();
        plan.data[0].mode = DataMode::Format { device: plan.system.device.clone(), encrypt: false };
        assert!(dry(&plan).2.unwrap_err().to_string().contains("picked twice"));

        let mut plan = plan_keep();
        plan.system.encrypt = false;
        plan.system.tpm = false;
        assert!(dry(&plan).2.unwrap_err().to_string().contains("encryption"));
    }

    #[test]
    fn no_secret_in_any_argv() {
        let pw = Secret(USER_PW.into());
        let c = steps::mkpasswd_cmd(&pw);
        assert!(c.args.iter().all(|a| !a.contains(USER_PW)));
        assert_eq!(c.stdin_bytes(), Some(USER_PW.as_bytes()));
        let prev = unlocked().previous;
        let c = previous::open_cmd(&prev, &Secret(DISK_PW.into()));
        assert!(c.args.iter().all(|a| !a.contains(DISK_PW)));
    }

    #[test]
    fn carry_commands() {
        let src = Path::new("/o");
        let dst = Path::new("/c");
        let c = steps::carry_cmds(&[Carry::OtherState, Carry::HostKey], src, dst);
        assert_eq!(c.len(), 1);
        assert_eq!(
            c[0].line(),
            "rsync -aHAX --exclude=/install --exclude=/var/lib/netbird --exclude=/var/lib/sbctl --exclude=/passwords \
             --exclude=/var/lib/bluetooth --exclude=/etc/NetworkManager/system-connections /o/ /c/"
        );
        let c = steps::carry_cmds(&[Carry::Pairings], src, dst);
        assert_eq!(
            c.iter().map(|c| c.line()).collect::<Vec<_>>(),
            [
                "rsync -aHAX --relative /o/./var/lib/bluetooth /c/",
                "rsync -aHAX --relative /o/./etc/NetworkManager/system-connections /c/"
            ]
        );
        assert!(steps::carry_cmds(&[], src, dst).is_empty());
    }

    #[test]
    fn backup_flags_follow_the_filesystem() {
        let l = steps::backup_cmd("ntfs", Path::new("/h"), Path::new("/b/x")).line();
        assert!(l.starts_with("rsync -rltH "), "{l}");
        let l = steps::backup_cmd("ext4", Path::new("/h"), Path::new("/b/x")).line();
        assert!(l.starts_with("rsync -aHAX "), "{l}");
    }

    #[test]
    fn which_old_password() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(steps::old_user(&v(&["vainnor"]), None, "carson").as_deref(), Some("vainnor"));
        assert_eq!(steps::old_user(&v(&["a", "vainnor"]), Some("vainnor"), "carson").as_deref(), Some("vainnor"));
        assert_eq!(steps::old_user(&v(&["a", "carson"]), Some("vainnor"), "carson").as_deref(), Some("carson"));
        assert_eq!(steps::old_user(&v(&["a", "b"]), Some("vainnor"), "carson"), None);
    }

    #[test]
    fn commit_has_no_attribution() {
        let c = steps::commit_cmd("/r", "vainnor", "me@x");
        assert_eq!(c.args.last().unwrap(), "chore(arc): install settings from vx install");
        assert!(c.args.contains(&"user.name=vainnor".to_string()));
        assert!(!c.line().contains("Co-Authored"));
    }

    #[test]
    fn prepare_repo_dry() {
        let data = InstallData {
            host: "arc".into(),
            repo_url: "https://github.com/vainnor/nixos".into(),
            flake: "/nix/store/abc-source".into(),
            defaults: plan_fresh().settings,
            wallpapers: vec![],
            themes: vec![],
        };
        assert_eq!(prepare_repo(&data, true, Mode::DryRun).unwrap(), PathBuf::from(WORK_REPO));
        assert_eq!(prepare_repo(&data, false, Mode::DryRun).unwrap(), PathBuf::from(WORK_REPO));
    }

    #[test]
    fn unlock_dry_is_plausible() {
        let u = unlock_previous(&unlocked().previous, Some(&Secret("x".into())), Mode::DryRun).unwrap();
        assert_eq!(u.users, ["vainnor"]);
        assert_eq!(u.available.len(), Carry::ALL.len());
        assert!(u.home_bytes > 100_000_000_000);
        assert!(unlock_previous(&unlocked().previous, None, Mode::DryRun).is_err());
        close_previous(Mode::DryRun).unwrap();
    }

    #[test]
    fn load_data_from_file() {
        let d = std::env::temp_dir().join(format!("vx-data-{}.json", std::process::id()));
        std::fs::write(
            &d,
            r#"{"host":"arc","repoUrl":"https://github.com/vainnor/nixos","flake":"/nix/store/x-source",
               "defaults":{"hostname":"arc","username":"vainnor","timeZone":"America/Chicago","wallpaper":"eye","theme":"match"},
               "wallpapers":[{"name":"eye","colors":{"base00":"101010"}}],"themes":["caelus"]}"#,
        )
        .unwrap();
        let data = load_data(Some(&d), None).unwrap();
        assert_eq!(data.defaults.username, "vainnor");
        assert_eq!(data.wallpapers[0].colors["base00"], "101010");
        std::fs::remove_file(&d).unwrap();
        assert!(load_data(None, None).is_err());
    }
}
