//! The install, step by step. Each step is one function; `run` in engine.rs
//! calls them in order and reports progress.

use super::super::plan::{
    Carry, DATA_KEYS, DISK_KEY, DISKS_JSON, DataMode, HomePlan, INSTALLED_KEYS, Outcome, Plan, TPM_ENROLL_KEY,
    Unlocked, WORK,
};
use super::disk::{self, find_part};
use super::finish::{self, NextInput};
use super::runner::{Cmd, Runner, shell_quote};
use super::{previous, settings};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub const NIX_FEATURES: &str = "nix-command flakes";
/// Mapper name of the system LUKS (matches _disko.nix and the initrd).
pub const CRYPTROOT: &str = "cryptroot";
/// Where a backup target partition is mounted.
pub const BACKUP_MOUNT: &str = "/run/vx-install/backup";
/// Marker written by prepare_repo when WORK_REPO is a snapshot, not a clone.
pub const SNAPSHOT_MARK: &str = "/tmp/vx-install/repo-is-snapshot";
pub const COMMIT_MSG: &str = "chore(arc): install settings from vx install";

pub fn carry_dir() -> PathBuf {
    Path::new(WORK).join("carry")
}
pub fn top_dir() -> PathBuf {
    Path::new(WORK).join("top")
}
pub fn data_key(name: &str) -> PathBuf {
    Path::new(DATA_KEYS).join(format!("{name}.key"))
}
/// A path of the installed system, under /mnt.
pub fn mnt(p: &str) -> PathBuf {
    Path::new("/mnt").join(p.trim_start_matches('/'))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Check,
    Carry,
    Backup,
    Settings,
    Disks,
    Restore,
    Install,
    Repo,
    Finish,
}

impl Step {
    pub fn name(self, plan: &Plan) -> &'static str {
        match self {
            Step::Check => "check the drives",
            Step::Carry => "save what to carry over",
            Step::Backup => "back up /home",
            Step::Settings => "write settings into the repo",
            Step::Disks if plan.home == HomePlan::Keep => "replace the system, keep /home",
            Step::Disks => "partition and format",
            Step::Restore if carries(plan) => "restore carried state",
            Step::Restore => "set up passwords and keys",
            Step::Install => "install the system",
            Step::Repo => "put the repo in your home",
            Step::Finish => "finish",
        }
    }
}

fn carries(plan: &Plan) -> bool {
    plan.previous.is_some() && !plan.carry.is_empty()
}

pub fn plan_steps(plan: &Plan) -> Vec<Step> {
    let mut v = vec![Step::Check];
    if plan.previous.is_some() {
        v.push(Step::Carry);
    }
    if matches!(plan.home, HomePlan::Backup { .. }) {
        v.push(Step::Backup);
    }
    v.extend([Step::Settings, Step::Disks, Step::Restore, Step::Install, Step::Repo, Step::Finish]);
    v
}

/// What the steps learn and pass along.
pub struct Run<'a> {
    pub plan: &'a Plan,
    pub r: Runner,
    /// The username in meta.nix before the installer changed it.
    pub old_default_user: Option<String>,
    pub age: String,
    pub host_key_carried: bool,
    pub outcome: Option<Outcome>,
}

impl<'a> Run<'a> {
    pub fn new(plan: &'a Plan, r: Runner) -> Self {
        Run { plan, r, old_default_user: None, age: String::new(), host_key_carried: false, outcome: None }
    }

    pub fn step(&mut self, s: Step) -> Result<()> {
        match s {
            Step::Check => disk::safety_checks(self.plan, &self.r),
            Step::Carry => self.carry(),
            Step::Backup => self.backup(),
            Step::Settings => self.settings(),
            Step::Disks => self.disks(),
            Step::Restore => self.restore(),
            Step::Install => self.install(),
            Step::Repo => self.repo(),
            Step::Finish => self.finish(),
        }
    }

    fn prev(&self) -> Result<&'a Unlocked> {
        self.plan.previous.as_ref().context("no previous install in the plan")
    }

    fn user(&self) -> &str {
        &self.plan.settings.username
    }

    fn flake(&self) -> String {
        self.plan.flake.to_string_lossy().into_owned()
    }

    fn git(&self) -> Cmd {
        Cmd::new("git").args(["-C", &self.flake()])
    }

    /// `nix build` an attribute of the repo copy; its store path.
    fn build(&self, attr: &str, fake: &str) -> Result<String> {
        let out = self.r.run(
            Cmd::new("nix")
                .args(["--extra-experimental-features", NIX_FEATURES, "build", "--no-link", "--print-out-paths"])
                .arg(format!("{}#{attr}", self.flake()))
                .fake(fake),
        )?;
        out.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).context("nix build printed no path")
    }

    // ------------------------------------------------------------ 1. carry

    /// Copy the chosen items out of the old @persistent into WORK/carry
    /// (owners, modes, ACLs, xattrs kept). Added to, never wiped first: after
    /// a failed "keep /home" run it may hold the only copy.
    fn carry(&mut self) -> Result<()> {
        let prev = self.prev()?;
        let src = previous::persistent();
        let dst = carry_dir();
        // WORK keeps others out; carry/ itself is 0755 because its mode lands on /persistent.
        self.r.mkdir(Path::new(WORK), 0o700)?;
        self.r.mkdir(&dst, 0o755)?;
        let chosen: Vec<Carry> = self.plan.carry.iter().copied().filter(|c| prev.available.contains(c)).collect();
        for c in carry_cmds(&chosen, &src, &dst) {
            self.r.run(c)?;
        }
        if !matches!(self.plan.home, HomePlan::Backup { .. }) {
            previous::close(&self.r)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ 1b. backup

    fn backup(&mut self) -> Result<()> {
        let HomePlan::Backup { partition } = &self.plan.home else { return Ok(()) };
        let prev = self.prev()?;
        let fstype = self.r.query(Cmd::new("lsblk").args(["-n", "-d", "-o", "FSTYPE", partition]).fake("ext4\n"))?;
        let fstype = fstype.trim();
        let mp = Path::new(BACKUP_MOUNT);
        self.r.mkdir(mp, 0o755)?;
        let mut m = Cmd::new("mount");
        if fstype == "ntfs" {
            m = m.args(["-t", "ntfs3"]);
        }
        self.r.run(m.arg(partition).arg(mp.to_string_lossy()))?;
        let res = (|| -> Result<()> {
            let df = self.r.query(
                Cmd::new("df").args(["-B1", "--output=avail"]).arg(mp.to_string_lossy()).fake("Avail\n500000000000\n"),
            )?;
            let avail: u64 = df.lines().filter_map(|l| l.trim().parse().ok()).next().unwrap_or(0);
            let need = prev.home_bytes + (1 << 30);
            if avail < need {
                bail!(
                    "not enough room on {partition} for the /home backup: {} GB free, {} GB needed. pick another partition or don't back up",
                    avail / 1_000_000_000,
                    need.div_ceil(1_000_000_000)
                );
            }
            let dest = mp.join(format!("nixos-home-backup-{}", finish::today()));
            self.r.run(backup_cmd(fstype, &previous::home(), &dest))?;
            Ok(())
        })();
        let _ = self.r.run(Cmd::new("umount").arg(mp.to_string_lossy()));
        res?;
        previous::close(&self.r)
    }

    // ------------------------------------------------------------ 2. settings

    fn settings(&mut self) -> Result<()> {
        let plan = self.plan;
        let repo = &plan.flake;
        let meta_rel = "modules/flake/meta.nix";
        let host_rel = format!("modules/hosts/{}/default.nix", plan.host);
        let meta_p = repo.join(meta_rel);
        let host_p = repo.join(&host_rel);

        let meta = self.r.read(&meta_p, "")?;
        let host = self.r.read(&host_p, "")?;
        let mut email = "vx-install@localhost".to_string();
        if !meta.is_empty() {
            self.old_default_user = settings::get_string(&meta, "username");
            if let Some(e) = settings::get_string(&meta, "email") {
                email = e;
            }
            self.r.write(&meta_p, settings::edit_meta(&meta, &plan.settings)?.as_bytes(), 0o644)?;
        } else {
            self.r.log(format!("edit {meta_rel}: username, flakePath, timeZone, wallpaper, theme"));
        }
        if !host.is_empty() {
            self.r.write(&host_p, settings::edit_host(&host, &plan.settings.hostname, &host_rel)?.as_bytes(), 0o644)?;
        } else {
            self.r.log(format!("edit {host_rel}: networking.hostName"));
        }
        self.r.write(&repo.join(DISKS_JSON), settings::disks_json(plan).as_bytes(), 0o644)?;

        self.r.run(self.git().args(["add", "--", meta_rel, &host_rel, DISKS_JSON]))?;
        // Nothing staged (a second run with the same choices): no commit needed.
        let clean = self.r.check(self.git().args(["diff", "--cached", "--quiet"]), false)?;
        if !clean {
            self.r.run(commit_cmd(&self.flake(), self.user(), &email))?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ 3. disks

    fn disks(&mut self) -> Result<()> {
        let plan = self.plan;
        self.r.mkdir(Path::new(WORK), 0o700)?;
        disk::rename_clashing_partlabels(&self.r, plan)?;
        if plan.system.encrypt {
            let pw = plan.disk_password.as_ref().context("no disk password")?;
            self.r.write_secret(Path::new(DISK_KEY), pw.0.as_bytes(), 0o600)?;
        }
        let encrypted_data: Vec<&str> = encrypted_data(plan);
        if !encrypted_data.is_empty() {
            self.r.mkdir(Path::new(DATA_KEYS), 0o700)?;
            for n in &encrypted_data {
                self.r.write_random_key(&data_key(n), 64, 0o600)?;
            }
        }

        if plan.home != HomePlan::Keep {
            // Destroys, formats and mounts every disk in disks.json at /mnt (no prompt: diskoScript skips it).
            let script = self.build(
                &format!("nixosConfigurations.{}.config.system.build.diskoScript", plan.host),
                "/nix/store/00000000000000000000000000000000-disko",
            )?;
            return self.r.run(Cmd::new(&script)).map(|_| ());
        }

        // Keep: no disko for the system disk; swap the system and @persistent by hand.
        let prev = self.prev()?;
        if disk::is_mounted(&self.r, Path::new("/mnt")) {
            self.r.run(Cmd::new("umount").args(["-R", "/mnt"]))?;
        }
        let dev = if prev.previous.encrypted {
            let mapper = format!("/dev/mapper/{CRYPTROOT}");
            if !self.r.exists(Path::new(&mapper), false) {
                self.r.run(Cmd::new("cryptsetup").args(["open", "--key-file", DISK_KEY, &prev.previous.root_part, CRYPTROOT]))?;
            }
            mapper
        } else {
            prev.previous.root_part.clone()
        };
        let top = top_dir();
        self.r.mkdir(&top, 0o700)?;
        let top_s = top.to_string_lossy().into_owned();
        self.r.run(Cmd::new("mount").args(["-t", "btrfs", "-o", "subvolid=5", &dev, &top_s]))?;
        let res = (|| -> Result<()> {
            for sv in ["@nix", "@home", "@log"] {
                if !self.r.exists(&top.join(sv), true) {
                    bail!("the previous install has no {sv} subvolume, so /home can't be kept: go back and pick fresh or back up");
                }
            }
            let p = top.join("@persistent");
            if self.r.exists(&p, true) {
                self.r.run(Cmd::new("btrfs").args(["subvolume", "delete"]).arg(p.to_string_lossy()))?;
            }
            self.r.run(Cmd::new("btrfs").args(["subvolume", "create"]).arg(p.to_string_lossy()))?;
            Ok(())
        })();
        self.r.run(Cmd::new("umount").arg(&top_s))?;
        res?;

        let esp_tree = disk::tree(&self.r, &disk::resolve(&self.r, &prev.previous.disk)?)?;
        let esp = find_part(&esp_tree, "disk-main-ESP", "vfat").context("the previous install's drive has no EFI partition")?;
        for c in keep_mount_cmds(&dev, &esp) {
            if let Some(dir) = c.args.last() {
                self.r.mkdir(Path::new(dir), 0o755)?;
            }
            self.r.run(c)?;
        }

        if plan.data.iter().any(|d| matches!(d.mode, DataMode::Format { .. })) {
            let script = self.build(
                &format!("nixosConfigurations.{}.config.system.build.diskoDataScript", plan.host),
                "/nix/store/00000000000000000000000000000000-disko-data",
            )?;
            self.r.run(Cmd::new(&script))?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ 4. restore

    fn restore(&mut self) -> Result<()> {
        let plan = self.plan;
        let pers = mnt("/persistent");
        if carries(plan) && self.r.exists(&carry_dir(), true) {
            self.r.run(
                Cmd::new("rsync").args(["-aHAX", &format!("{}/", carry_dir().display()), &format!("{}/", pers.display())]),
            )?;
        }

        // Login password.
        let pwdir = pers.join("passwords");
        let user = self.user().to_string();
        match &plan.user_password {
            Some(pw) => {
                let hash = self.r.run(mkpasswd_cmd(pw))?;
                let hash = hash.trim();
                if hash.is_empty() {
                    bail!("mkpasswd printed no hash");
                }
                self.r.mkdir(&pwdir, 0o700)?;
                self.r.write_secret(&pwdir.join(&user), format!("{hash}\n").as_bytes(), 0o600)?;
            }
            None => {
                let users = &self.prev()?.users;
                let old = old_user(users, self.old_default_user.as_deref(), &user)
                    .context("can't tell which old password to keep: go back and set a new login password")?;
                if old != user {
                    self.r.rename(&pwdir.join(&old), &pwdir.join(&user))?;
                }
            }
        }

        // SSH host key (also the sops age key).
        let key = pers.join("etc/ssh/ssh_host_ed25519_key");
        let pub_ = pers.join("etc/ssh/ssh_host_ed25519_key.pub");
        self.host_key_carried = plan.carry.contains(&Carry::HostKey) && self.r.exists(&key, true) && self.r.exists(&pub_, true);
        if !self.host_key_carried {
            self.r.mkdir(&pers.join("etc/ssh"), 0o755)?;
            self.r.remove(&key)?;
            self.r.remove(&pub_)?;
            self.r.run(
                Cmd::new("ssh-keygen")
                    .args(["-q", "-t", "ed25519", "-N", "", "-C", &format!("root@{}", plan.settings.hostname), "-f"])
                    .arg(key.to_string_lossy()),
            )?;
        }
        let pubkey = self.r.read(&pub_, "ssh-ed25519 AAAAdryrun root@arc")?;
        self.age = self
            .r
            .run(Cmd::new("ssh-to-age").stdin(pubkey.into_bytes()).fake("age1dryrunxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))?
            .trim()
            .to_string();

        // Data drive keys, for unlocking at boot.
        let enc = encrypted_data(plan);
        if !enc.is_empty() {
            let dir = mnt(INSTALLED_KEYS);
            self.r.mkdir(&dir, 0o700)?;
            for n in enc {
                self.r.copy_file(&data_key(n), &dir.join(format!("{n}.key")), 0o400)?;
            }
        }

        stage_secure_boot_keys(&self.r, plan)?;
        self.tpm_enroll_key()
    }

    /// A one-time key the first boot uses to enroll the TPM (then removes).
    fn tpm_enroll_key(&mut self) -> Result<()> {
        let plan = self.plan;
        if !(plan.system.encrypt && plan.system.tpm) {
            return Ok(());
        }
        let part = if plan.home == HomePlan::Keep {
            self.prev()?.previous.root_part.clone()
        } else {
            let t = disk::tree(&self.r, &disk::resolve(&self.r, &plan.system.device)?)?;
            find_part(&t, "disk-main-luks", "crypto_LUKS").context("can't find the new LUKS partition")?
        };
        if plan.home == HomePlan::Keep {
            let dump = self.r.query(Cmd::new("cryptsetup").args(["luksDump", &part]).fake(""))?;
            if dump.contains("systemd-tpm2") {
                self.r.log("the tpm is already enrolled on this disk: keeping it");
                return Ok(());
            }
        }
        let key = mnt(TPM_ENROLL_KEY);
        self.r.mkdir(key.parent().unwrap(), 0o700)?;
        self.r.write_random_key(&key, 64, 0o400)?;
        self.r.run(Cmd::new("cryptsetup").args(["luksAddKey", "--key-file", DISK_KEY, &part]).arg(key.to_string_lossy()))?;
        Ok(())
    }

    // ------------------------------------------------------------ 5. install

    fn install(&mut self) -> Result<()> {
        self.r.run(install_cmd(&self.flake(), &self.plan.host)).map(|_| ())
    }

    // ------------------------------------------------------------ 6. repo

    fn repo(&mut self) -> Result<()> {
        let user = self.user().to_string();
        let home = mnt(&format!("/home/{user}"));
        let dest = home.join("nixos");
        let keep = self.plan.home == HomePlan::Keep;
        if !self.r.exists(&home, keep) {
            self.r.mkdir(&home, 0o700)?;
        }
        if keep && self.r.exists(&dest, true) {
            let mut aside = home.join(format!("nixos.before-reinstall-{}", finish::today()));
            let mut n = 2;
            while self.r.exists(&aside, false) {
                aside = home.join(format!("nixos.before-reinstall-{}-{n}", finish::today()));
                n += 1;
            }
            self.r.rename(&dest, &aside)?;
        } else if self.r.exists(&dest, false) {
            bail!("{} already exists: move it away and try again", dest.display());
        }
        self.r.run(Cmd::new("cp").args(["-a", &self.flake()]).arg(dest.to_string_lossy()))?;
        self.r.write(&dest.join("HOST_AGE_KEY.txt"), format!("{}\n", self.age).as_bytes(), 0o644)?;
        self.r.run(chown_cmd(self.plan, keep))?;
        Ok(())
    }

    // ------------------------------------------------------------ 7. finish

    fn finish(&mut self) -> Result<()> {
        let plan = self.plan;
        let snapshot = self.r.exists(Path::new(SNAPSHOT_MARK), false);
        cleanup_secrets(&self.r);
        self.r.remove(&carry_dir())?;
        // The system is installed by now: trouble letting go of the drives is
        // worth a note, not a failed install (a reboot lets go of them anyway).
        let _ = self.r.run(Cmd::new("sync"));
        match self.r.run(Cmd::new("umount").args(["-R", "/mnt"])) {
            Ok(_) => close_luks(&self.r, plan),
            Err(e) => self.r.log(format!("could not unmount /mnt ({e:#}): it's installed, reboot anyway")),
        }
        if snapshot {
            self.r.log("note: ~/nixos is the usb's copy of the repo with its own history, not a clone of github");
        }
        let input = NextInput {
            sb_carried: plan.carry.contains(&Carry::SecureBoot) && plan.previous.is_some(),
            setup_mode: !self.r.dry() && finish::setup_mode(),
            tpm: plan.system.encrypt && plan.system.tpm,
            host_key_new: !self.host_key_carried,
            age: self.age.clone(),
            netbird_carried: plan.carry.contains(&Carry::NetBird) && plan.previous.is_some(),
            snapshot,
        };
        self.outcome = Some(Outcome {
            age_key: self.age.clone(),
            host_key_carried: self.host_key_carried,
            next_steps: finish::next_steps(&input),
        });
        Ok(())
    }
}

// ---------------------------------------------------------------- pieces (pure, tested)

/// Names of the data drives that get formatted encrypted.
pub fn encrypted_data(plan: &Plan) -> Vec<&str> {
    plan.data
        .iter()
        .filter(|d| matches!(d.mode, DataMode::Format { encrypt: true, .. }))
        .map(|d| d.name.as_str())
        .collect()
}

/// rsync commands that copy the chosen items. With OtherState, one rsync of
/// everything minus the unchosen items' paths and install/; otherwise one per path.
pub fn carry_cmds(chosen: &[Carry], src: &Path, dst: &Path) -> Vec<Cmd> {
    let dst_s = format!("{}/", dst.display());
    if chosen.contains(&Carry::OtherState) {
        let mut c = Cmd::new("rsync").arg("-aHAX").arg(format!("--exclude=/{}", previous::NEVER));
        for item in Carry::ALL.iter().filter(|i| !chosen.contains(i)) {
            for p in item.paths() {
                c = c.arg(format!("--exclude=/{p}"));
            }
        }
        return vec![c.arg(format!("{}/", src.display())).arg(dst_s)];
    }
    chosen
        .iter()
        .flat_map(|c| c.paths().iter())
        .map(|p| Cmd::new("rsync").args(["-aHAX", "--relative"]).arg(format!("{}/./{p}", src.display())).arg(&dst_s))
        .collect()
}

/// Copy the old /home; Linux filesystems keep owners/ACLs/xattrs, Windows ones can't.
pub fn backup_cmd(fstype: &str, home: &Path, dest: &Path) -> Cmd {
    let flags = match fstype {
        "ntfs" | "ntfs3" | "exfat" | "vfat" => "-rltH",
        _ => "-aHAX",
    };
    Cmd::new("rsync")
        .args([flags, "--info=progress2", "--no-inc-recursive"])
        .arg(format!("{}/", home.display()))
        .arg(format!("{}/", dest.display()))
}

pub fn commit_cmd(repo: &str, user: &str, email: &str) -> Cmd {
    Cmd::new("git").args([
        "-C",
        repo,
        "-c",
        &format!("user.name={user}"),
        "-c",
        &format!("user.email={email}"),
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "-m",
        COMMIT_MSG,
    ])
}

/// The layout of _disko.nix, mounted by hand under /mnt.
pub fn keep_mount_cmds(dev: &str, esp: &str) -> Vec<Cmd> {
    let mut v = vec![Cmd::new("mount").args(["-t", "tmpfs", "-o", "size=25%,mode=755", "none", "/mnt"])];
    for (sv, mp) in [("@nix", "/mnt/nix"), ("@persistent", "/mnt/persistent"), ("@home", "/mnt/home"), ("@log", "/mnt/var/log")] {
        v.push(Cmd::new("mount").args(["-t", "btrfs", "-o", &format!("subvol={sv},compress=zstd,noatime"), dev, mp]));
    }
    v.push(Cmd::new("mount").args(["-t", "vfat", "-o", "umask=0077", esp, "/mnt/boot"]));
    v
}

pub fn mkpasswd_cmd(pw: &super::super::plan::Secret) -> Cmd {
    Cmd::new("mkpasswd")
        .args(["-m", "yescrypt", "--stdin"])
        .stdin(pw.0.clone().into_bytes())
        .quiet()
        .fake("$y$j9T$dryrun$dryrundryrundryrundryrundryrundryrundry")
}

pub fn install_cmd(flake: &str, host: &str) -> Cmd {
    Cmd::new("nixos-install").args(["--flake", &format!("{flake}#{host}"), "--no-root-passwd", "--no-channel-copy"])
}

/// The old password file to keep for `new`: the only one, or the old default user's, or `new`'s own.
pub fn old_user(users: &[String], old_default: Option<&str>, new: &str) -> Option<String> {
    if users.iter().any(|u| u == new) {
        return Some(new.to_string());
    }
    if users.len() == 1 {
        return Some(users[0].clone());
    }
    old_default.filter(|d| users.iter().any(|u| u == d)).map(str::to_string)
}

/// One nixos-enter: the repo, the home folder and new data drives under it belong to the user.
pub fn chown_cmd(plan: &Plan, keep: bool) -> Cmd {
    let u = &plan.settings.username;
    let home = format!("/home/{u}");
    let mut parts = vec![format!("chown -R {u}:users {}", shell_quote(&format!("{home}/nixos"))), format!("chown {u}:users {home}")];
    if !keep {
        parts.push(format!("chmod 700 {home}"));
    }
    for d in &plan.data {
        if matches!(d.mode, DataMode::Format { .. }) && d.mount.starts_with(&format!("{home}/")) {
            parts.push(format!("chown {u}:users {}", shell_quote(&d.mount)));
        }
    }
    Cmd::new("nixos-enter").args(["--root", "/mnt", "-c", &parts.join(" && ")])
}

/// Carried Secure Boot keys also go to /mnt/var/lib/sbctl, where lanzaboote
/// looks for them while nixos-install signs the boot files (the bind mount
/// from /persistent only exists once the system boots).
pub fn stage_secure_boot_keys(r: &Runner, plan: &Plan) -> Result<()> {
    if !(plan.carry.contains(&Carry::SecureBoot) && plan.previous.is_some()) {
        return Ok(());
    }
    let from = mnt("/persistent/var/lib/sbctl");
    if !r.exists(&from, true) {
        return Ok(());
    }
    let to = mnt("/var/lib/sbctl");
    r.mkdir(&to, 0o700)?;
    r.run(Cmd::new("rsync").args(["-aHAX", &format!("{}/", from.display()), &format!("{}/", to.display())]))?;
    Ok(())
}

/// Remove the key files under WORK. Best effort (also runs after a failure).
pub fn cleanup_secrets(r: &Runner) {
    let _ = r.remove(Path::new(DISK_KEY));
    let _ = r.remove(Path::new(DATA_KEYS));
}

/// Close every LUKS mapping on the system and data drives. Best effort.
pub fn close_luks(r: &Runner, plan: &Plan) {
    let mut devs = vec![plan.system.device.clone()];
    for d in &plan.data {
        if let DataMode::Format { device, .. } = &d.mode {
            devs.push(device.clone());
        }
    }
    for d in devs {
        let Ok(real) = disk::resolve(r, &d) else { continue };
        let Ok(t) = disk::tree(r, &real) else { continue };
        for m in disk::crypt_children(&t) {
            if let Err(e) = r.run(Cmd::new("cryptsetup").args(["close", &m])) {
                r.log(format!("could not close {m}: {e:#}"));
            }
        }
    }
}

/// After a failure: secrets go, mounts stay (to look around from the shell).
pub fn after_failure(r: &Runner) {
    cleanup_secrets(r);
    if disk::is_mounted(r, Path::new(BACKUP_MOUNT)) {
        let _ = r.run(Cmd::new("umount").arg(BACKUP_MOUNT));
    }
    if !r.dry() && carry_dir().exists() {
        r.log(format!("what was carried over is still in {} (ram: gone after a reboot)", carry_dir().display()));
    }
    if disk::is_mounted(r, Path::new("/mnt")) {
        r.log("the new system is still mounted at /mnt: look around from the shell, or run umount -R /mnt");
    }
}
