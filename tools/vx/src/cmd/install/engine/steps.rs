//! The install, step by step. Each step is one function; `run` in engine.rs
//! calls them in order and reports progress.
//!
//! The home backup restore (restic.rs) runs once the disks are mounted and
//! before restore, which takes the usb off NetBird; the backup is only
//! reachable over it.

use super::super::plan::{
    Carry, DATA_KEYS, DISK_KEY, DataMode, GH_CONFIG, GitHubMode, HOST_KEY_WORK, HomePlan, HostPlan, INSTALLED_KEYS, NETBIRD_STATE,
    OP_CONFIG, Outcome, Plan, RESTIC_CA_REL, RESTIC_WORK, SBCTL_DIR, SECRETS_OK, TPM_ENROLL_KEY, Unlocked, WORK, disks_json_path,
    hardware_path, host_dir, host_json_path, restore_offered,
};
use super::disk::{self, find_part};
use super::finish::{self, NextInput, PushState};
use super::runner::{Cmd, Runner, shell_quote};
use super::{github, netbird, previous, restic, settings, wifi};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub const NIX_FEATURES: &str = "nix-command flakes";
/// Mapper name of the system LUKS (matches _disko.nix and the initrd).
pub const CRYPTROOT: &str = "cryptroot";
/// Where a backup target partition is mounted.
pub const BACKUP_MOUNT: &str = "/run/vx-install/backup";
/// Marker written by prepare_repo when WORK_REPO is a snapshot, not a clone.
pub const SNAPSHOT_MARK: &str = "/tmp/vx-install/repo-is-snapshot";
pub const META_REL: &str = "modules/flake/meta.nix";
pub const SOPS_REL: &str = ".sops.yaml";
const HOST_KEY_NAME: &str = "ssh_host_ed25519_key";
/// Firmware variables that may be immutable before enrolling (efivarfs).
const SB_VARS: [&str; 4] = ["PK-", "KEK-", "db-", "dbx-"];
const EFIVARS: &str = "/sys/firmware/efi/efivars";

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
    Hardware,
    HomeRestore,
    Restore,
    SecureBoot,
    Commit,
    Install,
    Push,
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
            Step::Hardware => "detect the hardware",
            Step::HomeRestore => "restore /home from the homelab",
            Step::Restore if carries(plan) => "restore carried state",
            Step::Restore => "set up passwords and keys",
            Step::SecureBoot => "enroll secure boot keys",
            Step::Commit => "commit the settings",
            Step::Install => "install the system",
            Step::Push => "push to github",
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
    v.extend([Step::Settings, Step::Disks]);
    // A new host's hardware config needs the disks mounted at /mnt; it's part
    // of the commit, which has to exist before nixos-install builds the flake.
    if plan.host.is_new() {
        v.push(Step::Hardware);
    }
    // Into the new /home while the usb is still on NetBird (restore takes it off).
    if plan.restore.is_some() && restore_offered(&plan.home) {
        v.push(Step::HomeRestore);
    }
    v.push(Step::Restore);
    // Firmware in Setup Mode: enroll the keys now, so nixos-install signs with them.
    if plan.setup_mode {
        v.push(Step::SecureBoot);
    }
    v.extend([Step::Commit, Step::Install]);
    // After a good install, before the repo is copied home (so the copy knows it's pushed).
    if plan.github.pushes() {
        v.push(Step::Push);
    }
    v.extend([Step::Repo, Step::Finish]);
    v
}

/// What the steps learn and pass along.
pub struct Run<'a> {
    pub plan: &'a Plan,
    pub r: Runner,
    /// The username in meta.nix before the installer changed it.
    pub old_default_user: Option<String>,
    /// The email in meta.nix before the installer changed it.
    pub meta_email: Option<String>,
    pub age: String,
    pub host_key_carried: bool,
    pub push: PushState,
    /// gh's login was copied into the new home.
    pub gh_copied: bool,
    /// Secure Boot keys enrolled in the firmware by this run.
    pub sb_enrolled: bool,
    /// meta.secretsReady (after the fork's edits).
    pub secrets_ready: bool,
    /// .sops.yaml got this host's key (part of the install commit).
    pub sops_edited: bool,
    /// The NetBird login made on the installer ("ip (fqdn)").
    pub netbird: Option<String>,
    /// The home restore ran (/home/<user> may hold files: ~/nixos is moved aside).
    pub home_restored: bool,
    /// uid and gid of the restored /home/<user>, as the backed-up system had them.
    pub restored_ids: Option<(u32, u32)>,
    /// The restore didn't finish: why.
    pub restore_failed: Option<String>,
    pub outcome: Option<Outcome>,
}

impl<'a> Run<'a> {
    pub fn new(plan: &'a Plan, r: Runner) -> Self {
        Run {
            plan,
            r,
            old_default_user: None,
            meta_email: None,
            age: String::new(),
            host_key_carried: false,
            push: PushState::default(),
            gh_copied: false,
            sb_enrolled: false,
            secrets_ready: false,
            sops_edited: false,
            netbird: None,
            home_restored: false,
            restored_ids: None,
            restore_failed: None,
            outcome: None,
        }
    }

    pub fn step(&mut self, s: Step) -> Result<()> {
        match s {
            Step::Check => disk::safety_checks(self.plan, &self.r),
            Step::Carry => self.carry(),
            Step::Backup => self.backup(),
            Step::Settings => self.settings(),
            Step::Disks => self.disks(),
            Step::Hardware => self.hardware(),
            Step::HomeRestore => self.home_restore(),
            Step::Restore => self.restore(),
            Step::SecureBoot => self.secure_boot(),
            Step::Commit => self.commit(),
            Step::Install => self.install(),
            Step::Push => self.push(),
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

    fn host(&self) -> &'a str {
        self.plan.host.name()
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

    /// meta.nix, the host's files and disks.json, staged (git-added) so the
    /// flake sees them; committed later, together with the hardware config.
    fn settings(&mut self) -> Result<()> {
        let plan = self.plan;
        let repo = &plan.flake;
        let host = self.host();
        let meta_p = repo.join(META_REL);
        let meta = self.r.read(&meta_p, "")?;
        let personal = personal(plan, &meta);
        if !meta.is_empty() {
            self.old_default_user = settings::get_string(&meta, "username");
            self.meta_email = settings::get_string(&meta, "email");
            let mut edited = settings::edit_meta(&meta, &plan.settings)?;
            if let Some(p) = &personal {
                edited = settings::personalise_meta(&edited, p)?;
            }
            self.secrets_ready = settings::get_bool(&edited, "secretsReady") == Some(true);
            self.r.write(&meta_p, edited.as_bytes(), 0o644)?;
        } else {
            self.r.log(format!("edit {META_REL}: username, flakePath, timeZone, wallpaper, theme"));
            if personal.is_some() {
                self.r.log(format!(
                    "edit {META_REL} for your fork: fullName, email, repoUrl, githubUser, sshKeys, signingKey, secretsReady, resticReady, restoreItem, binaryCache"
                ));
            }
        }

        let mut paths = vec![META_REL.to_string()];
        match &plan.host {
            HostPlan::New(h) => {
                let p = repo.join(host_json_path(host));
                if !self.r.dry() && p.exists() {
                    bail!("{} is already in the repo: go back and pick another name, or reinstall it", host_dir(host));
                }
                self.r.mkdir(&repo.join(host_dir(host)), 0o755)?;
                self.r.write(&p, settings::host_json(h).as_bytes(), 0o644)?;
                paths.push(host_json_path(host));
            }
            HostPlan::Existing(_) => {
                if !self.r.exists(&repo.join(host_dir(host)), true) {
                    bail!("the repo has no {}: go back and pick the machine again", host_dir(host));
                }
            }
        }
        self.r.write(&repo.join(disks_json_path(host)), settings::disks_json(plan).as_bytes(), 0o644)?;
        paths.push(disks_json_path(host));
        if let Some(p) = self.host_key()? {
            paths.push(p);
        }
        self.r.run(self.git().arg("add").arg("--").args(&paths))?;
        Ok(())
    }

    /// The SSH host key (also the sops age key), known before the settings
    /// commit: the carried one, or a new one made in HOST_KEY_WORK (copied to
    /// /persistent in restore). A new key is added to .sops.yaml so the other
    /// machines re-encrypt the secrets for it (not in a fork: its secrets are
    /// its own). Returns .sops.yaml when it was edited.
    fn host_key(&mut self) -> Result<Option<String>> {
        let plan = self.plan;
        let host = self.host();
        let carried_key = carry_dir().join("etc/ssh").join(HOST_KEY_NAME);
        self.host_key_carried = plan.carry.contains(&Carry::HostKey)
            && plan.previous.as_ref().is_some_and(|p| p.available.contains(&Carry::HostKey))
            && self.r.exists(&carried_key, true)
            && self.r.exists(&pub_of(&carried_key), true);
        let key = if self.host_key_carried {
            carried_key
        } else {
            let dir = Path::new(HOST_KEY_WORK);
            self.r.mkdir(Path::new(WORK), 0o700)?;
            self.r.remove(dir)?;
            self.r.mkdir(dir, 0o700)?;
            let key = dir.join(HOST_KEY_NAME);
            self.r.run(
                Cmd::new("ssh-keygen")
                    .args(["-q", "-t", "ed25519", "-N", "", "-C", &format!("root@{}", plan.settings.hostname), "-f"])
                    .arg(key.to_string_lossy()),
            )?;
            key
        };
        let pubkey = self.r.read(&pub_of(&key), "ssh-ed25519 AAAAdryrun root@arc")?;
        self.age = self
            .r
            .run(Cmd::new("ssh-to-age").stdin(pubkey.into_bytes()).fake("age1dryrunxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))?
            .trim()
            .to_string();
        if self.host_key_carried || plan.github.is_fork() {
            return Ok(None);
        }
        let p = plan.flake.join(SOPS_REL);
        let before = self.r.read(&p, crate::sops::SAMPLE)?;
        let after = crate::sops::set_host(&before, host, &self.age, host)
            .with_context(|| format!("couldn't add {host} to {SOPS_REL}: add it later with vx secrets add-host"))?;
        if after != before {
            self.r.log(format!("edit {SOPS_REL}: {host} becomes a recipient of the secrets"));
            self.r.write(&p, after.as_bytes(), 0o644)?;
        }
        self.sops_edited = true;
        Ok(Some(SOPS_REL.to_string()))
    }

    // ------------------------------------------------------------ 3b. hardware

    /// A new host's _hardware.nix, from this machine (disks are mounted at /mnt
    /// by now; fileSystems stay out: disko owns those).
    fn hardware(&mut self) -> Result<()> {
        let host = self.host();
        let out = self.r.run(
            Cmd::new("nixos-generate-config")
                .args(["--no-filesystems", "--show-hardware-config", "--root", "/mnt"])
                .quiet()
                .fake(FAKE_HARDWARE),
        )?;
        if !out.contains('{') {
            bail!("nixos-generate-config printed no hardware config");
        }
        let rel = hardware_path(host);
        self.r.write(&self.plan.flake.join(&rel), out.as_bytes(), 0o644)?;
        self.r.run(self.git().args(["add", "--", &rel]))?;
        Ok(())
    }

    // ------------------------------------------------------------ 3c. home restore

    /// The latest snapshot's /home/<user> into the new /home (restic.rs).
    /// Tried [`restic::ATTEMPTS`] times; if it still stops, the install goes on
    /// and the summary says how to restore later.
    fn home_restore(&mut self) -> Result<()> {
        let Some(hr) = &self.plan.restore else { return Ok(()) };
        let user = self.user().to_string();
        let ca = restic::ca(&self.r, &self.plan.flake);
        if ca.is_none() {
            self.r.log(format!("{RESTIC_CA_REL} isn't in the repo: the server's certificate can't be checked"));
        }
        self.r.log(format!("restoring /home/{user} from the backup of {} (login: {})", hr.latest, hr.source.label()));
        restic::write_creds(&self.r, &hr.creds)?;
        let r = &self.r;
        let failed = restic::retry(
            restic::ATTEMPTS,
            |_| r.run(restic::restore_cmd(&hr.creds, ca.as_deref(), &user)).map(|_| ()),
            |n, why| {
                r.log(format!("the restore stopped ({why}): again in 15 s ({} of {}); what's there already is skipped", n + 1, restic::ATTEMPTS));
                if !r.dry() {
                    std::thread::sleep(std::time::Duration::from_secs(15));
                }
            },
        );
        restic::remove_creds(&self.r);
        self.home_restored = true;
        let home = mnt(&format!("/home/{user}"));
        if self.r.exists(&home, true) {
            let ids = self.r.query(Cmd::new("stat").args(["-c", "%u %g"]).arg(home.to_string_lossy()).fake("1000 100\n"))?;
            let mut it = ids.split_whitespace().filter_map(|n| n.parse::<u32>().ok());
            self.restored_ids = it.next().zip(it.next());
        }
        if let Some(why) = failed {
            self.r.log(format!("the restore didn't finish ({why}): the install goes on; restore later with vx restore --restic"));
            self.restore_failed = Some(why);
        }
        Ok(())
    }

    // ------------------------------------------------------------ 4b. commit

    /// One commit with everything the installer changed (before nixos-install,
    /// so the installed system's flake revision is the pushed one).
    fn commit(&mut self) -> Result<()> {
        let host = self.host();
        let mut paths = vec![META_REL.to_string(), host_dir(host)];
        if self.sops_edited {
            paths.push(SOPS_REL.to_string());
        }
        self.r.run(self.git().args(["add", "-A", "--"]).args(&paths))?;
        // Nothing staged (a second run with the same choices): no commit needed.
        let clean = self.r.check(self.git().args(["diff", "--cached", "--quiet"]), false)?;
        if clean {
            self.r.log("nothing new to commit");
            return Ok(());
        }
        let (name, email) = author(self.plan, self.meta_email.as_deref());
        self.r.run(commit_cmd(&self.flake(), &name, &email, &commit_message(&self.plan.host)))?;
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
                &format!("nixosConfigurations.{}.config.system.build.diskoScript", self.host()),
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
                &format!("nixosConfigurations.{}.config.system.build.diskoDataScript", self.host()),
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

        // The Wi-Fi this usb is on (connected in the installer, or baked into
        // the stick by `vx iso --wifi`): the new system joins it from its first
        // boot. A carried-over file of the same name wins.
        let wifi_files = if self.r.dry() { vec![] } else { wifi::saved_keyfiles() };
        if !wifi_files.is_empty() {
            let dir = pers.join(wifi::CONNECTIONS.trim_start_matches('/'));
            self.r.mkdir(&dir, 0o700)?;
            for f in wifi_files {
                let Some(name) = f.file_name() else { continue };
                let to = dir.join(name);
                if !self.r.exists(&to, false) {
                    self.r.copy_file(&f, &to, 0o600)?;
                }
            }
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

        // SSH host key (also the sops age key): made in settings, unless carried.
        let ssh = pers.join("etc/ssh");
        if !self.host_key_carried {
            let made = Path::new(HOST_KEY_WORK).join(HOST_KEY_NAME);
            self.r.mkdir(&ssh, 0o755)?;
            self.r.copy_file(&made, &ssh.join(HOST_KEY_NAME), 0o600)?;
            self.r.copy_file(&pub_of(&made), &pub_of(&ssh.join(HOST_KEY_NAME)), 0o644)?;
        } else if self.secrets_ready {
            // The old key decrypts already: the first boot has nothing to wait for.
            let ok = mnt(SECRETS_OK);
            self.r.mkdir(ok.parent().unwrap(), 0o700)?;
            self.r.write(&ok, b"", 0o644)?;
        }

        self.netbird_login()?;

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

    /// The NetBird login made on the installer: `netbird down` (the installer
    /// and the new system never use the key at once), then its whole state
    /// folder into /persistent.
    fn netbird_login(&mut self) -> Result<()> {
        let plan = self.plan;
        if !plan.netbird || (plan.carry.contains(&Carry::NetBird) && plan.previous.is_some()) {
            return Ok(());
        }
        if !netbird::logged_in(&self.r) {
            self.r.log("no netbird login on the installer: log in later with sudo netbird up");
            return Ok(());
        }
        netbird::stop();
        self.netbird = netbird::status(&self.r).ok().map(|s| s.show());
        if let Err(e) = self.r.run(netbird::down_cmd()) {
            self.r.log(format!("netbird down failed ({e:#}): copying the login anyway"));
        }
        let to = mnt(&format!("/persistent{NETBIRD_STATE}"));
        self.r.mkdir(&to, 0o700)?;
        self.r.run(netbird::copy_cmd(&to))?;
        Ok(())
    }

    // ------------------------------------------------------------ 4c. secure boot

    /// The firmware is in Setup Mode: our keys (carried, else new) go in now,
    /// with Microsoft's (Windows and the GPU's option ROM need them), and into
    /// the new system so nixos-install signs the boot files with them.
    fn secure_boot(&mut self) -> Result<()> {
        if !self.r.dry() && !finish::setup_mode() {
            self.r.log("the firmware isn't in setup mode: the new system makes and enrolls its keys later");
            return Ok(());
        }
        let plan = self.plan;
        let iso = Path::new(SBCTL_DIR);
        let carried = mnt(&format!("/persistent{SBCTL_DIR}"));
        let use_carried =
            plan.carry.contains(&Carry::SecureBoot) && plan.previous.is_some() && self.r.exists(&carried.join("keys"), true);
        self.r.remove(iso)?;
        if use_carried {
            self.r.mkdir(iso, 0o700)?;
            self.r.run(Cmd::new("rsync").args(["-aHAX", &format!("{}/", carried.display()), &format!("{SBCTL_DIR}/")]))?;
        } else {
            self.r.run(Cmd::new("sbctl").arg("create-keys"))?;
        }
        let vars = self.immutable_candidates();
        if !vars.is_empty()
            && let Err(e) = self.r.run(Cmd::new("chattr").arg("-i").args(&vars))
        {
            self.r.log(format!("chattr -i failed ({e:#}): trying to enroll anyway"));
        }
        self.r.run(Cmd::new("sbctl").args(["enroll-keys", "--microsoft"]))?;
        for to in [mnt(&format!("/persistent{SBCTL_DIR}")), mnt(SBCTL_DIR)] {
            if to == carried && use_carried {
                continue;
            }
            self.r.mkdir(&to, 0o700)?;
            self.r.run(Cmd::new("rsync").args(["-aHAX", &format!("{SBCTL_DIR}/"), &format!("{}/", to.display())]))?;
        }
        self.sb_enrolled = true;
        Ok(())
    }

    /// PK/KEK/db/dbx in efivarfs (may be immutable; enrolling writes them).
    fn immutable_candidates(&self) -> Vec<String> {
        if self.r.dry() {
            return [
                "PK-8be4df61-93ca-11d2-aa0d-00e098032b8c",
                "KEK-8be4df61-93ca-11d2-aa0d-00e098032b8c",
                "db-d719b2cb-3d3a-4596-a3bc-dad00e67656f",
                "dbx-d719b2cb-3d3a-4596-a3bc-dad00e67656f",
            ]
            .iter()
            .map(|n| format!("{EFIVARS}/{n}"))
            .collect();
        }
        let mut v: Vec<String> = std::fs::read_dir(EFIVARS)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| SB_VARS.iter().any(|p| n.starts_with(p)))
                    .map(|n| format!("{EFIVARS}/{n}"))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
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
        self.r.run(install_cmd(&self.flake(), self.host())).map(|_| ())
    }

    // ------------------------------------------------------------ 5b. push

    /// Push the install commit. Never fails the install: a failed push is a next step.
    fn push(&mut self) -> Result<()> {
        let Some((branch, shown)) = push_target(self.plan) else { return Ok(()) };
        if self.r.exists(Path::new(SNAPSHOT_MARK), false) {
            self.r.log("not pushing: the repo is the usb's copy with its own history");
            self.push = PushState::NotPushed("the repo came from the usb, not github".into());
            return Ok(());
        }
        match self.r.run(push_cmd(&self.flake(), &branch)) {
            Ok(_) => self.push = PushState::Pushed(shown),
            Err(e) => {
                self.r.log(format!("push failed (the install is fine): {e:#}"));
                self.push = PushState::Failed(first_line(&format!("{e:#}")));
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ 6. repo

    fn repo(&mut self) -> Result<()> {
        let user = self.user().to_string();
        let home = mnt(&format!("/home/{user}"));
        let dest = home.join("nixos");
        let keep = self.plan.home == HomePlan::Keep;
        // A restored home may hold an old ~/nixos too: moved aside like a kept one.
        let lived_in = keep || self.home_restored;
        if !self.r.exists(&home, lived_in) {
            self.r.mkdir(&home, 0o700)?;
        }
        if lived_in && self.r.exists(&dest, true) {
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
        // Logged in to GitHub here: the new system is too (gh's hosts.yml only:
        // home-manager may own config.yml).
        let hosts = Path::new(GH_CONFIG).join("hosts.yml");
        if self.plan.github.mode != GitHubMode::Offline && self.r.exists(&hosts, true) {
            let gh_dir = home.join(".config/gh");
            if !self.r.exists(&home.join(".config"), lived_in) {
                self.r.mkdir(&home.join(".config"), 0o755)?;
            }
            if !self.r.exists(&gh_dir, lived_in) {
                self.r.mkdir(&gh_dir, 0o700)?;
            }
            self.r.copy_file(&hosts, &gh_dir.join("hosts.yml"), 0o600)?;
            self.gh_copied = true;
        }
        self.r.run(chown_cmd(self.plan, keep, self.gh_copied, self.restored_ids))?;
        Ok(())
    }

    // ------------------------------------------------------------ 7. finish

    fn finish(&mut self) -> Result<()> {
        let plan = self.plan;
        let snapshot = self.r.exists(Path::new(SNAPSHOT_MARK), false);
        netbird::stop();
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
        let push = match (&plan.github.mode, plan.github.push, &self.push) {
            (GitHubMode::Offline, _, _) => PushState::Offline,
            (_, false, _) => PushState::Off,
            (_, true, p) => p.clone(),
        };
        let sb_carried = plan.carry.contains(&Carry::SecureBoot) && plan.previous.is_some();
        let input = NextInput {
            sb_carried,
            sb_enrolled: self.sb_enrolled,
            tpm: plan.system.encrypt && plan.system.tpm,
            secrets_pending: self.secrets_pending(),
            restore_failed: self.restore_failed.clone(),
            user: plan.settings.username.clone(),
        };
        let netbird_carried = plan.carry.contains(&Carry::NetBird) && plan.previous.is_some();
        self.outcome = Some(Outcome {
            age_key: self.age.clone(),
            host_key_carried: self.host_key_carried,
            next_steps: finish::next_steps(&input),
            pushed: match &push {
                PushState::Pushed(s) => Some(s.clone()),
                _ => None,
            },
            not_pushed: finish::not_pushed(&push, snapshot),
            sb_enrolled: self.sb_enrolled,
            secrets_pending: self.secrets_pending(),
            netbird: if netbird_carried { Some("carried over".into()) } else { self.netbird.clone() },
            home_restored: plan.restore.as_ref().filter(|_| self.home_restored && self.restore_failed.is_none()).map(|h| h.latest.clone()),
            restore_failed: self.restore_failed.clone(),
        });
        Ok(())
    }
}

impl Run<'_> {
    /// Secrets are on, and this new host waits for another machine to re-encrypt them.
    fn secrets_pending(&self) -> bool {
        self.secrets_ready && !self.host_key_carried && !self.plan.github.is_fork()
    }
}

/// `<key>.pub`
fn pub_of(key: &Path) -> PathBuf {
    let mut s = key.as_os_str().to_owned();
    s.push(".pub");
    PathBuf::from(s)
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).to_string()
}

/// What DryRun pretends nixos-generate-config printed.
const FAKE_HARDWARE: &str = "# Do not modify this file!  It was generated by 'nixos-generate-config'
{ config, lib, pkgs, modulesPath, ... }:
{
  imports = [ (modulesPath + \"/installer/scan/not-detected.nix\") ];
  boot.initrd.availableKernelModules = [ \"nvme\" \"xhci_pci\" \"ahci\" \"usbhid\" ];
  boot.kernelModules = [ \"kvm-amd\" ];
  nixpkgs.hostPlatform = lib.mkDefault \"x86_64-linux\";
  hardware.cpu.amd.updateMicrocode = lib.mkDefault config.hardware.enableRedistributableFirmware;
}
";

/// Someone's install from their fork: what goes into meta.nix. None for the owner / offline.
pub fn personal(plan: &Plan, meta: &str) -> Option<settings::Personal> {
    let GitHubMode::Fork { url, .. } = &plan.github.mode else { return None };
    let login = plan.github.login.clone().unwrap_or_default();
    Some(settings::Personal {
        full_name: plan.github.full_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| login.clone()),
        email: plan
            .github
            .email
            .clone()
            .filter(|e| !e.is_empty())
            .or_else(|| (!login.is_empty()).then(|| format!("{login}@users.noreply.github.com")))
            .or_else(|| settings::get_string(meta, "email"))
            .unwrap_or_default(),
        ssh_keys: plan.github.keys.clone(),
        repo_url: url.clone(),
        github_user: login,
    })
}

/// The commit's author: the GitHub name/email when known, else the username and meta.nix's email.
pub fn author(plan: &Plan, meta_email: Option<&str>) -> (String, String) {
    let g = &plan.github;
    let name = g.full_name.clone().filter(|n| !n.is_empty()).or_else(|| g.login.clone()).unwrap_or_else(|| plan.settings.username.clone());
    let email = g
        .email
        .clone()
        .filter(|e| !e.is_empty())
        .or_else(|| meta_email.map(str::to_string))
        .unwrap_or_else(|| "vx-install@localhost".into());
    (name, email)
}

pub fn commit_message(host: &HostPlan) -> String {
    match host {
        HostPlan::New(h) => format!("feat(hosts): add {}", h.name),
        HostPlan::Existing(n) => format!("chore({n}): install settings"),
    }
}

/// (branch, "owner/repo branch") the install commit goes to, None when it doesn't.
pub fn push_target(plan: &Plan) -> Option<(String, String)> {
    if !plan.github.pushes() {
        return None;
    }
    match &plan.github.mode {
        GitHubMode::Owner => Some(("main".into(), format!("{} main", plan.github.upstream))),
        GitHubMode::Fork { url, branch } => {
            let name = super::super::plan::parse_github_url(url).map(|(o, r)| format!("{o}/{r}")).unwrap_or_else(|| url.clone());
            Some((branch.clone(), format!("{name} {branch}")))
        }
        GitHubMode::Offline => None,
    }
}

/// `git push -u origin HEAD:<branch>`, with gh as the credential helper.
pub fn push_cmd(repo: &str, branch: &str) -> Cmd {
    github::git_env(Cmd::new("git").args(["-C", repo]).args(github::git_auth_args()).args([
        "push",
        "-u",
        "origin",
        &format!("HEAD:{branch}"),
    ]))
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

pub fn commit_cmd(repo: &str, user: &str, email: &str, msg: &str) -> Cmd {
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
        msg,
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

/// One nixos-enter: the repo, the home folder, gh's login and new data drives
/// under it belong to the user. A home restored from the backup keeps the old
/// system's numeric owners; `restored` (that uid, gid) is handed over to the
/// user and `users` as this system made them, when they differ.
pub fn chown_cmd(plan: &Plan, keep: bool, gh: bool, restored: Option<(u32, u32)>) -> Cmd {
    let u = &plan.settings.username;
    let home = format!("/home/{u}");
    let mut parts = vec![];
    if let Some((uid, gid)) = restored {
        parts.push(format!("{{ [ \"$(id -u {u})\" = {uid} ] || chown -hR --from={uid} {u} {home}; }}"));
        parts.push(format!("{{ [ \"$(id -g {u})\" = {gid} ] || chown -hR --from=:{gid} :users {home}; }}"));
    }
    parts.extend([format!("chown -R {u}:users {}", shell_quote(&format!("{home}/nixos"))), format!("chown {u}:users {home}")]);
    if !keep {
        parts.push(format!("chmod 700 {home}"));
    }
    if gh {
        parts.push(format!("chown {u}:users {home}/.config {home}/.config/gh {home}/.config/gh/hosts.yml"));
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
/// gh's login stays until the end (a failed run may be retried).
pub fn cleanup_secrets(r: &Runner) {
    let _ = r.remove(Path::new(DISK_KEY));
    let _ = r.remove(Path::new(DATA_KEYS));
    let _ = r.remove(Path::new(RESTIC_WORK));
    let _ = r.remove(Path::new(OP_CONFIG));
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
