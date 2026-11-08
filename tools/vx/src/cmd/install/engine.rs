//! Does the install described by a Plan.
//!
//! Every external command goes through one runner ([`runner::Runner`]) that,
//! in dry-run/demo mode, only logs "$ cmd args" instead of running it.
//!
//! - `runner`   runs commands and writes files, honouring [`Mode`]
//! - `disk`     lsblk, mounts, the safety checks
//! - `previous` unlocking a previous install read-only, what it can carry
//! - `settings` meta.nix / host.json / disks.json edits
//! - `github`   logging in, the user's fork, their keys
//! - `netbird`  logging in to NetBird on the installer
//! - `wifi`     scanning for and connecting to Wi-Fi on the installer
//! - `steps`    the install steps
//! - `finish`   the summary's next steps

mod disk;
mod finish;
pub mod github;
pub mod netbird;
mod previous;
mod runner;
pub mod settings;
mod steps;
pub mod wifi;

use super::plan::{Event, InstallData, Plan, Previous, Secret, StepState, Unlocked, WORK, WORK_REPO};
use anyhow::{Context, Result, bail};
use runner::{Cmd, Runner};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};

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

/// Run `f` with a Runner whose log lines are collected and returned with its result.
fn logged<T>(mode: Mode, f: impl FnOnce(&Runner) -> Result<T>) -> (Result<T>, Vec<String>) {
    let (tx, rx) = mpsc::channel();
    let r = Runner::new(mode, Some(tx));
    let res = f(&r);
    drop(r);
    let logs = rx.try_iter().filter_map(|e| if let Event::Log(l) = e { Some(l) } else { None }).collect();
    (res, logs)
}

/// Make WORK_REPO: a fresh `git clone` of `origin` (the upstream repo, or the
/// user's fork; through gh's login) when `update`, otherwise a writable copy
/// of data.flake. A fork also gets `upstream` = data.repo_url. Returns the
/// path, and the commands it ran.
pub fn prepare_repo(data: &InstallData, update: bool, origin: &str, mode: Mode) -> (Result<PathBuf>, Vec<String>) {
    logged(mode, |r| prepare_repo_with(r, data, update, origin))
}

fn prepare_repo_with(r: &Runner, data: &InstallData, update: bool, origin: &str) -> Result<PathBuf> {
    let repo = Path::new(WORK_REPO);
    r.mkdir(Path::new(WORK), 0o700)?;
    r.remove(repo)?;
    r.remove(Path::new(steps::SNAPSHOT_MARK))?;
    let git = || Cmd::new("git").args(["-C", WORK_REPO]);
    let fork = origin != data.repo_url;
    if update {
        let clone = github::git_env(Cmd::new("git").args(github::git_auth_args()).args(["clone", "-q", origin, WORK_REPO]));
        if r.check(clone, true)? {
            if fork {
                r.run(git().args(["remote", "add", "upstream", &data.repo_url]))?;
            }
            return Ok(repo.to_path_buf());
        }
        r.log(format!("couldn't clone {origin}: using the usb's copy"));
        r.remove(repo)?;
    }
    // A snapshot of the ISO's copy (a read-only store path) with its own history.
    r.run(Cmd::new("cp").args(["-rT", &data.flake, WORK_REPO]))?;
    r.run(Cmd::new("chmod").args(["-R", "u+w", WORK_REPO]))?;
    r.run(git().args(["init", "-q", "-b", "main"]))?;
    r.run(git().args(["remote", "add", "origin", origin]))?;
    if fork {
        r.run(git().args(["remote", "add", "upstream", &data.repo_url]))?;
    }
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

// ---------------------------------------------------------------- github (before the install)

/// Ask GitHub for a one-time login code.
pub fn gh_device_code(mode: Mode) -> (Result<github::DeviceCode>, Vec<String>) {
    logged(mode, github::device_code)
}

/// Wait for the code to be entered, then log gh in. `cancel` stops waiting.
pub fn gh_login(dc: &github::DeviceCode, cancel: &AtomicBool, mode: Mode) -> (Result<()>, Vec<String>) {
    logged(mode, |r| github::wait_and_login(r, dc, cancel))
}

/// The logged-in account: owner of `owner/repo`, or their fork (if any).
pub fn gh_account(owner: &str, repo: &str, mode: Mode) -> (Result<github::Account>, Vec<String>) {
    logged(mode, |r| github::account(r, owner, repo))
}

/// Fork `owner/repo` for `login` and wait until it shows up.
pub fn gh_fork(owner: &str, repo: &str, login: &str, mode: Mode) -> (Result<github::Fork>, Vec<String>) {
    logged(mode, |r| github::fork(r, owner, repo, login))
}

/// NetBird: `netbird up` for `host`; the URL to log in at (or already connected).
pub fn nb_start(host: &str, mode: Mode) -> (Result<netbird::Start>, Vec<String>) {
    logged(mode, |r| netbird::start(r, host))
}

/// Wait for the NetBird login to finish; this peer's address. `cancel` stops waiting.
pub fn nb_wait(cancel: &AtomicBool, mode: Mode) -> (Result<netbird::Status>, Vec<String>) {
    logged(mode, |r| netbird::wait(r, cancel))
}

/// The Wi-Fi networks around, strongest first.
pub fn wifi_scan(mode: Mode) -> (Result<Vec<wifi::Network>>, Vec<String>) {
    logged(mode, wifi::scan)
}

/// Connect to a Wi-Fi network and wait for the internet (up to 20 s): Ok(online).
pub fn wifi_connect(n: &wifi::Network, password: Option<&Secret>, mode: Mode) -> (Result<bool>, Vec<String>) {
    logged(mode, |r| {
        wifi::connect(r, n, password)?;
        Ok(wifi::wait_online(r, 20, super::probe::online))
    })
}

/// Stop a waiting `netbird up` (leaving the screen, quitting).
pub fn nb_stop() {
    netbird::stop();
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

    pub fn owner_github() -> GitHubPlan {
        GitHubPlan {
            login: Some("Vainnor".into()),
            upstream: "vainnor/nixos".into(),
            mode: GitHubMode::Owner,
            push: true,
            keys: vec!["ssh-ed25519 AAAAowner".into()],
            full_name: Some("Vainnor".into()),
            email: Some("vainnor@example.com".into()),
        }
    }

    pub fn fork_github() -> GitHubPlan {
        GitHubPlan {
            login: Some("someone".into()),
            upstream: "vainnor/nixos".into(),
            mode: GitHubMode::Fork { url: "https://github.com/someone/nixos.git".into(), branch: "main".into() },
            push: true,
            keys: vec!["ssh-ed25519 AAAAsomeone".into()],
            full_name: Some("Some One".into()),
            email: Some("someone@example.com".into()),
        }
    }

    pub fn new_host() -> HostPlan {
        HostPlan::New(NewHost {
            name: "forge".into(),
            aspects: vec!["base".into(), "desktop".into(), "dev".into()],
            gpu: Gpu::Nvidia,
            state_version: "26.11".into(),
        })
    }

    pub fn plan_fresh() -> Plan {
        Plan {
            flake: PathBuf::from("/nonexistent/vx-install/nixos"),
            host: HostPlan::Existing("arc".into()),
            github: owner_github(),
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
            setup_mode: false,
            netbird: false,
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
                "commit the settings",
                "install the system",
                "push to github",
                "put the repo in your home",
                "finish"
            ]
        );
        // A new machine detects its hardware after the disks are mounted and
        // before the commit; the commit is before the install, the push after it.
        p.host = new_host();
        let n = names(&p);
        let at = |s: &str| n.iter().position(|x| x == s).unwrap_or_else(|| panic!("no {s} in {n:?}"));
        assert!(at("partition and format") < at("detect the hardware"));
        assert!(at("detect the hardware") < at("commit the settings"));
        assert!(at("commit the settings") < at("install the system"));
        assert!(at("install the system") < at("push to github"));
        assert!(at("push to github") < at("put the repo in your home"));
        // Offline: nothing to push.
        p.github = GitHubPlan::offline("vainnor/nixos");
        assert!(!names(&p).contains(&"push to github".to_string()));
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
                "commit the settings",
                "install the system",
                "push to github",
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
            assert!(!l.contains(DISK_PW) && !l.contains(USER_PW) && !l.contains("gho_"), "secret in log: {l}");
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
                // The host key is made before the settings commit: its age key goes into .sops.yaml.
                "ssh-keygen -q -t ed25519 -N '' -C root@arc -f /tmp/vx-install/hostkey/ssh_host_ed25519_key",
                "ssh-to-age < (stdin)",
                "git -C /nonexistent/vx-install/nixos add -- modules/flake/meta.nix modules/hosts/arc/disks.json .sops.yaml",
                "nix --extra-experimental-features 'nix-command flakes' build --no-link --print-out-paths /nonexistent/vx-install/nixos#nixosConfigurations.arc.config.system.build.diskoScript",
                "/nix/store/00000000000000000000000000000000-disko",
                "mkpasswd -m yescrypt --stdin < (stdin)",
                "lsblk -J -p -o NAME,TYPE,FSTYPE,PARTLABEL,MOUNTPOINTS /dev/disk/by-id/nvme-SYS",
                "cryptsetup luksAddKey --key-file /tmp/vx-install/disk.key /dev/disk/by-id/nvme-SYS-part2 /mnt/persistent/install/tpm-enroll.key",
                "git -C /nonexistent/vx-install/nixos add -A -- modules/flake/meta.nix modules/hosts/arc .sops.yaml",
                "git -C /nonexistent/vx-install/nixos -c user.name=Vainnor -c user.email=vainnor@example.com -c commit.gpgsign=false commit -q -m 'chore(arc): install settings'",
                "nixos-install --flake /nonexistent/vx-install/nixos#arc --no-root-passwd --no-channel-copy",
                "git -C /nonexistent/vx-install/nixos -c credential.https://github.com.helper= -c 'credential.https://github.com.helper=!gh auth git-credential' push -u origin HEAD:main",
                "cp -a /nonexistent/vx-install/nixos /mnt/home/vainnor/nixos",
                "nixos-enter --root /mnt -c 'chown -R vainnor:users /home/vainnor/nixos && chown vainnor:users /home/vainnor && chmod 700 /home/vainnor && chown vainnor:users /home/vainnor/.config /home/vainnor/.config/gh /home/vainnor/.config/gh/hosts.yml && chown vainnor:users /home/vainnor/games'",
                "umount -R /mnt",
            ],
        );
        assert!(logs.contains(&"copy /tmp/vx-install/gh/hosts.yml -> /mnt/home/vainnor/.config/gh/hosts.yml (600)".to_string()));
        assert!(!c.iter().any(|c| c.starts_with("nixos-generate-config")), "an existing host keeps its hardware config");
        assert!(!c.iter().any(|c| c.contains("diskoDataScript") || c.contains("btrfs subvolume")));
        assert!(logs.contains(&"write /tmp/vx-install/disk.key (secret, 600)".to_string()));
        assert!(logs.contains(&"write /tmp/vx-install/keys/games.key (64 random bytes, 600)".to_string()));
        assert!(logs.contains(&"copy /tmp/vx-install/keys/games.key -> /mnt/persistent/etc/keys/games.key (400)".to_string()));
        assert!(logs.iter().any(|l| l.starts_with("write /mnt/persistent/passwords/vainnor (secret")));
        assert!(logs.contains(&"write /nonexistent/vx-install/nixos/.sops.yaml (644)".to_string()));
        assert!(logs.contains(&"copy /tmp/vx-install/hostkey/ssh_host_ed25519_key -> /mnt/persistent/etc/ssh/ssh_host_ed25519_key (600)".to_string()));
        assert!(!c.iter().any(|c| c.starts_with("sbctl") || c.starts_with("netbird")), "not in setup mode, no netbird login");

        // Every step ran and it ended in Done.
        let n = steps(&plan).len();
        let done = events.iter().filter(|e| matches!(e, Event::Step { state: StepState::Done, .. })).count();
        assert_eq!(done, n);
        let Some(Event::Done(o)) = events.last() else { panic!("didn't end in Done: {:?}", events.last()) };
        assert!(o.age_key.starts_with("age1"));
        assert!(!o.host_key_carried);
        let all = o.next_steps.join("\n");
        assert!(!all.contains("add-host") && !all.contains("netbird") && !all.contains("gh auth") && !all.contains("push"), "{all}");
        assert_eq!(o.next_steps.len(), 2, "usb + secure boot; secrets are off in this meta: {all}");
        assert!(!o.secrets_pending && !o.sb_enrolled);
        assert_eq!(o.pushed.as_deref(), Some("vainnor/nixos main"));
        assert_eq!(o.not_pushed, None);
    }

    #[test]
    fn dry_run_new_host_from_a_fork() {
        let mut plan = plan_fresh();
        plan.host = new_host();
        plan.github = fork_github();
        plan.settings.hostname = "forge".into();
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        no_secrets(&logs);
        let c = cmds(&logs);
        assert_order(
            &c,
            &[
                "git -C /nonexistent/vx-install/nixos add -- modules/flake/meta.nix modules/hosts/forge/host.json modules/hosts/forge/disks.json",
                "nix --extra-experimental-features 'nix-command flakes' build --no-link --print-out-paths /nonexistent/vx-install/nixos#nixosConfigurations.forge.config.system.build.diskoScript",
                "/nix/store/00000000000000000000000000000000-disko",
                "nixos-generate-config --no-filesystems --show-hardware-config --root /mnt",
                "git -C /nonexistent/vx-install/nixos add -- modules/hosts/forge/_hardware.nix",
                "git -C /nonexistent/vx-install/nixos add -A -- modules/flake/meta.nix modules/hosts/forge",
                "git -C /nonexistent/vx-install/nixos -c 'user.name=Some One' -c user.email=someone@example.com -c commit.gpgsign=false commit -q -m 'feat(hosts): add forge'",
                "nixos-install --flake /nonexistent/vx-install/nixos#forge --no-root-passwd --no-channel-copy",
                "git -C /nonexistent/vx-install/nixos -c credential.https://github.com.helper= -c 'credential.https://github.com.helper=!gh auth git-credential' push -u origin HEAD:main",
                "cp -a /nonexistent/vx-install/nixos /mnt/home/vainnor/nixos",
            ],
        );
        assert!(logs.contains(&"write /nonexistent/vx-install/nixos/modules/hosts/forge/host.json (644)".to_string()));
        assert!(logs.contains(&"write /nonexistent/vx-install/nixos/modules/hosts/forge/_hardware.nix (644)".to_string()));
        assert!(logs.iter().any(|l| l.contains("edit modules/flake/meta.nix for your fork")));
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert_eq!(o.pushed.as_deref(), Some("someone/nixos main"));
        assert!(!c.iter().any(|c| c.contains(".sops.yaml")), "a fork's secrets are its own: .sops.yaml untouched");
        assert!(!o.secrets_pending);
    }

    #[test]
    fn offline_commits_but_never_pushes() {
        let mut plan = plan_fresh();
        plan.github = GitHubPlan::offline("vainnor/nixos");
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        let c = cmds(&logs);
        assert!(c.iter().any(|c| c.contains(" commit -q -m 'chore(arc): install settings'")));
        assert!(!c.iter().any(|c| c.contains(" push ")));
        assert!(!logs.iter().any(|l| l.contains("gh/hosts.yml")), "no gh login to copy");
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert_eq!(o.pushed, None);
        assert!(o.not_pushed.as_deref().unwrap().contains("vx sync"));
        assert!(!o.next_steps.iter().any(|s| s.contains("gh auth") || s.contains("push")));
    }

    #[test]
    fn commit_identity_and_message() {
        let mut p = plan_fresh();
        assert_eq!(steps::author(&p, Some("meta@x")), ("Vainnor".to_string(), "vainnor@example.com".to_string()));
        p.github = GitHubPlan::offline("vainnor/nixos");
        assert_eq!(steps::author(&p, Some("meta@x")), ("vainnor".to_string(), "meta@x".to_string()));
        assert_eq!(steps::commit_message(&new_host()), "feat(hosts): add forge");
        assert_eq!(steps::commit_message(&HostPlan::Existing("arc".into())), "chore(arc): install settings");
        let mut f = plan_fresh();
        f.github = fork_github();
        let per = steps::personal(&f, "").unwrap();
        assert_eq!((per.repo_url.as_str(), per.github_user.as_str()), ("https://github.com/someone/nixos.git", "someone"));
        f.github.email = None;
        assert_eq!(steps::personal(&f, "").unwrap().email, "someone@users.noreply.github.com");
        assert!(steps::personal(&plan_fresh(), "").is_none(), "the owner's meta isn't personalised");
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
                // The carried host key's age recipient; .sops.yaml stays as it is.
                "ssh-to-age",
                "git -C /nonexistent/vx-install/nixos add -- modules/flake/meta.nix modules/hosts/arc/disks.json",
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
                "rsync -aHAX /mnt/persistent/var/lib/sbctl/ /mnt/var/lib/sbctl/",
                "cryptsetup luksAddKey --key-file /tmp/vx-install/disk.key /dev/nvme0n1p2 /mnt/persistent/install/tpm-enroll.key",
                "git -C /nonexistent/vx-install/nixos -c user.name=Vainnor",
                "nixos-install",
                "git -C /nonexistent/vx-install/nixos -c credential.https://github.com.helper=",
                "cp -a",
                "nixos-enter --root /mnt -c 'chown -R vainnor:users /home/vainnor/nixos && chown vainnor:users /home/vainnor && chown vainnor:users /home/vainnor/.config /home/vainnor/.config/gh /home/vainnor/.config/gh/hosts.yml && chown vainnor:users /home/vainnor/games'",
                "umount -R /mnt",
            ],
        );
        // Carried: no new password, no new host key; old ~/nixos moved aside.
        assert!(!c.iter().any(|c| c.starts_with("mkpasswd") || c.starts_with("ssh-keygen") || c.contains("diskoScript ")));
        assert!(logs.iter().any(|l| l.starts_with("move /mnt/home/vainnor/nixos -> /mnt/home/vainnor/nixos.before-reinstall-")));
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert!(o.host_key_carried);
        assert_eq!(o.next_steps[1], "turn secure boot back on in the bios: your keys are still enrolled; the disk then unlocks itself");
        assert!(!o.next_steps.iter().any(|s| s.contains("netbird") || s.contains("secrets")));
        assert!(!c.iter().any(|c| c.contains(".sops.yaml")), "a carried key is already a recipient");
        assert_eq!(o.netbird.as_deref(), Some("carried over"));
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
    fn secure_boot_enrolled_before_the_install() {
        let mut plan = plan_fresh();
        plan.setup_mode = true;
        let n = names(&plan);
        let at = |s: &str| n.iter().position(|x| x == s).unwrap_or_else(|| panic!("no {s} in {n:?}"));
        assert!(at("set up passwords and keys") < at("enroll secure boot keys"));
        assert!(at("enroll secure boot keys") < at("install the system"));
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        no_secrets(&logs);
        let c = cmds(&logs);
        assert_order(
            &c,
            &[
                "sbctl create-keys",
                "chattr -i /sys/firmware/efi/efivars/PK-8be4df61-93ca-11d2-aa0d-00e098032b8c /sys/firmware/efi/efivars/KEK-",
                "sbctl enroll-keys --microsoft",
                "rsync -aHAX /var/lib/sbctl/ /mnt/persistent/var/lib/sbctl/",
                "rsync -aHAX /var/lib/sbctl/ /mnt/var/lib/sbctl/",
                "nixos-install",
            ],
        );
        assert!(logs.contains(&"mkdir /mnt/persistent/var/lib/sbctl (700)".to_string()));
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert!(o.sb_enrolled);
        assert_eq!(o.next_steps[1], "turn secure boot on in the bios (your keys are enrolled); the disk then unlocks itself");

        // Carried keys: those are enrolled, none made.
        let mut plan = plan_keep();
        plan.setup_mode = true;
        let (logs, _, res) = dry(&plan);
        res.unwrap();
        let c = cmds(&logs);
        assert!(!c.iter().any(|c| c == "sbctl create-keys"));
        assert_order(&c, &["rsync -aHAX /mnt/persistent/var/lib/sbctl/ /var/lib/sbctl/", "sbctl enroll-keys --microsoft", "rsync -aHAX /var/lib/sbctl/ /mnt/var/lib/sbctl/", "nixos-install"]);
        assert!(!c.iter().any(|c| c == "rsync -aHAX /var/lib/sbctl/ /mnt/persistent/var/lib/sbctl/"), "already there");
    }

    #[test]
    fn netbird_login_is_copied_in_restore() {
        let mut plan = plan_fresh();
        plan.netbird = true;
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        let c = cmds(&logs);
        assert_order(
            &c,
            &[
                "/nix/store/00000000000000000000000000000000-disko",
                "netbird status --json",
                "netbird down",
                "rsync -rlt --chmod=D700,F600 /var/lib/netbird/ /mnt/persistent/var/lib/netbird/",
                "git -C /nonexistent/vx-install/nixos -c user.name=Vainnor",
                "nixos-install",
            ],
        );
        assert!(logs.contains(&"mkdir /mnt/persistent/var/lib/netbird (700)".to_string()));
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert_eq!(o.netbird.as_deref(), Some("100.92.14.7 (forge.netbird.cloud)"));
        // Carried over: the old login is kept, nothing copied from the usb.
        let mut plan = plan_keep();
        plan.netbird = true;
        let c = cmds(&dry(&plan).0);
        assert!(!c.iter().any(|c| c.starts_with("netbird")));
    }

    /// The repo's meta.nix with secrets on, in a temp repo (skipped where the repo isn't there).
    fn repo_with_secrets(name: &str) -> Option<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let meta = std::fs::read_to_string(root.join(steps::META_REL)).ok()?;
        let dir = std::env::temp_dir().join(format!("vx-secrets-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("modules/flake")).unwrap();
        std::fs::write(dir.join(steps::META_REL), meta.replace("secretsReady = false;", "secretsReady = true;")).unwrap();
        std::fs::write(dir.join(".sops.yaml"), crate::sops::SAMPLE).unwrap();
        Some(dir)
    }

    #[test]
    fn secrets_on_new_host_key() {
        let Some(dir) = repo_with_secrets("new") else { return };
        let mut plan = plan_fresh();
        plan.flake = dir.clone();
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert!(o.secrets_pending && !o.host_key_carried);
        assert!(o.next_steps[2].starts_with("secrets: your other machines add this one automatically"));
        assert!(!logs.iter().any(|l| l.contains("secrets-ok")), "it waits for the re-encryption");
        // Carried: secrets already work; the wait unit is told so.
        let mut plan = plan_keep();
        plan.flake = dir.clone();
        let (logs, events, res) = dry(&plan);
        res.unwrap();
        assert!(logs.contains(&"write /mnt/persistent/install/secrets-ok (644)".to_string()));
        let Some(Event::Done(o)) = events.last() else { panic!("no Done") };
        assert!(!o.secrets_pending);
        std::fs::remove_dir_all(dir).unwrap();
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
        // The GitHub token: stdin only, and never in the log of a whole login.
        let c = github::login_cmd(&Secret("gho_SECRET".into()));
        assert!(c.args.iter().all(|a| !a.contains("gho_SECRET")) && !c.line().contains("gho_SECRET"));
        let dc = gh_device_code(Mode::DryRun).0.unwrap();
        let (res, logs) = gh_login(&dc, &AtomicBool::new(false), Mode::DryRun);
        res.unwrap();
        assert!(logs.iter().any(|l| l.starts_with("$ gh auth login --hostname github.com")), "{logs:?}");
        assert!(logs.iter().all(|l| !l.contains("gho_") && !l.contains(&dc.device_code.0)), "{logs:?}");
        // The push: no credentials in the URL or argv.
        let c = steps::push_cmd("/r", "main");
        assert!(c.args.iter().all(|a| !a.contains('@') || a.contains("helper")));
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
        let c = steps::commit_cmd("/r", "vainnor", "me@x", "chore(arc): install settings");
        assert_eq!(c.args.last().unwrap(), "chore(arc): install settings");
        assert!(c.args.contains(&"user.name=vainnor".to_string()));
        assert!(!c.line().contains("Co-Authored"));
    }

    fn data() -> InstallData {
        InstallData {
            host: "arc".into(),
            repo_url: "https://github.com/vainnor/nixos".into(),
            flake: "/nix/store/abc-source".into(),
            defaults: plan_fresh().settings,
            wallpapers: vec![],
            themes: vec![],
            owner: "vainnor".into(),
            repo: "nixos".into(),
            hosts: vec![],
            aspects: vec![],
            state_version: "26.11".into(),
        }
    }

    #[test]
    fn prepare_repo_dry() {
        let data = data();
        let (p, logs) = prepare_repo(&data, true, &data.repo_url, Mode::DryRun);
        assert_eq!(p.unwrap(), PathBuf::from(WORK_REPO));
        assert!(logs.iter().any(|l| l.starts_with("$ git -c credential.https://github.com.helper= ") && l.ends_with("clone -q https://github.com/vainnor/nixos /tmp/vx-install/nixos")), "{logs:?}");
        assert!(!logs.iter().any(|l| l.contains("upstream")));
        // A fork: cloned from the fork, upstream added.
        let (p, logs) = prepare_repo(&data, true, "https://github.com/someone/nixos.git", Mode::DryRun);
        p.unwrap();
        assert!(logs.iter().any(|l| l.ends_with("clone -q https://github.com/someone/nixos.git /tmp/vx-install/nixos")));
        assert!(logs.contains(&"$ git -C /tmp/vx-install/nixos remote add upstream https://github.com/vainnor/nixos".to_string()));
        let (p, logs) = prepare_repo(&data, false, &data.repo_url, Mode::DryRun);
        assert_eq!(p.unwrap(), PathBuf::from(WORK_REPO));
        assert!(logs.contains(&"$ git -C /tmp/vx-install/nixos remote add origin https://github.com/vainnor/nixos".to_string()));
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
