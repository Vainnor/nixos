//! Rebuilding, updating and checking the system, on the NixOS machines (nh) and the Mac
//! (darwin-rebuild), plus `vx @arc ...` / `vx @mac ...` over SSH.

mod doctor;
mod mac;
mod status;
mod util;

use crate::ctx::{self, ARC_SSH, Ctx, Host, MAC_HOST, MAC_SSH};
use crate::ui;
use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Subcommand)]
pub enum Cmd {
    /// One screen: generation, repo, inputs, failed units, disks, backups, network
    Status,
    /// Deeper checks (secrets, ssh agent, signing, caches, flake) with fixes
    Doctor {
        /// Also run the whole flake check and a real signing test
        #[arg(long)]
        full: bool,
    },
    /// git pull the repo (local commits rebased on top) and list the new commits; works from any folder
    Pull,
    /// Build and switch to the current config (darwin-rebuild on the Mac)
    Switch {
        /// git pull first (rebasing local commits)
        #[arg(long)]
        pull: bool,
    },
    /// Build and activate without adding a boot entry (reverts on reboot)
    Test,
    /// Build and make it the default for the next boot, without switching now
    Boot,
    /// Build only, and show what would change
    Build,
    /// Update every flake input (or some: `vx update nixpkgs`), show what changes, then offer to switch
    Update {
        /// Inputs to update (default: all)
        inputs: Vec<String>,
        /// Switch without asking
        #[arg(short, long)]
        yes: bool,
    },
    /// Update inputs, then switch
    Upgrade {
        /// Inputs to update (default: all)
        inputs: Vec<String>,
    },
    /// Go back to the previous generation
    Rollback,
    /// Remove old generations and unused store paths (keeps the last 10 and 14 days)
    Clean {
        /// Don't ask first
        #[arg(short, long)]
        yes: bool,
    },
    /// What the working tree would change vs the running system
    Diff {
        /// Instead: what changed between the last two system generations
        #[arg(long)]
        gens: bool,
    },
    /// Format, check, commit, push, switch here, then offer the other machine
    Sync {
        /// Commit message (asked for if there are changes and none is given)
        message: Vec<String>,
        /// Don't ask before switching the other machine
        #[arg(short, long)]
        yes: bool,
    },
    /// From the Mac: push, then pull and switch on the PC over NetBird SSH
    Deploy {
        /// Only activate on the PC's next boot
        #[arg(long)]
        boot: bool,
    },
    /// Push local commits to GitHub
    Push,
    /// Evaluate everything without building (both machines' configs)
    Check,
    /// Format all Nix files
    Fmt,
    /// Format, check, then commit everything: `vx commit fix: thing`
    Commit {
        /// The commit message
        #[arg(required = true, num_args = 1..)]
        message: Vec<String>,
    },
    /// Browse what takes space in the running system
    Tree,
    /// Open a REPL with this machine's config loaded (`config.services...`)
    Repl,
    /// Push the running system to the homelab cache now (normally automatic)
    CachePush,
    /// New project with its own toolchain: `vx new python ~/code/thing`
    New {
        /// Template: python node java go php rust c
        lang: String,
        /// Where (created if missing)
        dir: PathBuf,
    },
    /// Change your login password (stored on /persistent; applies at next login)
    Password,
    /// Build the custom installer ISO (x86_64 Linux); --flash writes it to a USB stick
    Iso {
        /// USB stick to write it to, e.g. /dev/sdb (erased; asks first)
        #[arg(long, value_name = "DEVICE", conflicts_with = "ventoy")]
        flash: Option<String>,
        /// Copy it onto a Ventoy stick instead (found by its "Ventoy" label, or the folder given); nothing is erased
        #[arg(long, value_name = "DIR", num_args = 0..=1, default_missing_value = "")]
        ventoy: Option<String>,
        /// Put a Wi-Fi network on the stick so it connects by itself: this PC's current one, or the one named
        #[arg(long, value_name = "NETWORK", num_args = 0..=1, default_missing_value = "")]
        wifi: Option<String>,
    },
    /// Boot this machine's config in a VM window (arc's off a NixOS host; login vainnor / preview)
    Preview {
        /// Graphics: soft (no GL, for WSL), gl, or auto
        #[arg(long, value_enum, default_value_t = Gpu::Auto)]
        gpu: Gpu,
        /// Throw away the preview VM's disk instead (fresh first boot next time)
        #[arg(long)]
        reset: bool,
    },
    /// The Mac: build, adopt apps into Homebrew, undo nix-darwin, upgrade brews
    #[command(subcommand)]
    Mac(MacCmd),
    /// List every command
    List,
}

#[derive(Subcommand)]
pub enum MacCmd {
    /// Build and switch the Mac (same as `vx switch` there)
    Switch,
    /// Build the Mac config only, and show what would change
    Build,
    /// First time only: hand apps you installed by hand over to Homebrew
    Adopt,
    /// Remove nix-darwin and put the Mac back as it was (settings, dotfiles, /etc)
    Undo {
        /// Only show what it would do
        #[arg(long)]
        dry: bool,
    },
    /// Update Homebrew and upgrade every brew and cask (vx switch doesn't)
    Upgrade,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Gpu {
    Auto,
    Soft,
    Gl,
}

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Status => status::run(ctx),
        Cmd::Doctor { full } => doctor::run(ctx, full),
        Cmd::Pull => pull(ctx),
        Cmd::Switch { pull } => {
            if pull {
                ctx.run(ctx.cmd("git").args(PULL))?;
            }
            switch(ctx)
        }
        Cmd::Test => nh_os(ctx, "test"),
        Cmd::Boot => nh_os(ctx, "boot"),
        Cmd::Build => match ctx.host {
            Host::NixOs => nh_os(ctx, "build"),
            Host::Mac => build_and_diff(ctx),
        },
        Cmd::Update { inputs, yes } => update(ctx, &inputs, yes),
        Cmd::Upgrade { inputs } => update(ctx, &inputs, true),
        Cmd::Rollback => match ctx.host {
            Host::NixOs => ctx.run(ctx.sudo("nixos-rebuild").args(["switch", "--rollback"])),
            Host::Mac => ctx.run(ctx.sudo(&darwin_rebuild(ctx)?).args(["switch", "--rollback"])),
        },
        Cmd::Clean { yes } => {
            if !yes && !ui::confirm("remove generations beyond the last 10 and older than 14 days, and unused store paths?") {
                return Ok(());
            }
            ctx.run(ctx.cmd("nh").args(["clean", "all", "--keep", "10", "--keep-since", "14d"]))
        }
        Cmd::Diff { gens } => {
            if gens {
                diff_generations(ctx)
            } else {
                build_and_diff(ctx)
            }
        }
        Cmd::Sync { message, yes } => sync(ctx, &message.join(" "), yes),
        Cmd::Deploy { boot } => deploy(ctx, boot),
        Cmd::Push => push(ctx),
        Cmd::Check => check(ctx),
        Cmd::Fmt => fmt(ctx),
        Cmd::Commit { message } => {
            fmt(ctx)?;
            check(ctx)?;
            commit(ctx, &message.join(" "))
        }
        Cmd::Tree => {
            let mut c = if ctx::has("nix-tree") {
                ctx.cmd("nix-tree")
            } else {
                let mut c = ctx.nix();
                c.args(["run", "nixpkgs#nix-tree", "--"]);
                c
            };
            ctx.run(c.arg("/run/current-system"))
        }
        Cmd::Repl => {
            let attr = match ctx.host {
                Host::NixOs => format!("nixosConfigurations.{}", ctx.nixos_host()),
                Host::Mac => format!("darwinConfigurations.{MAC_HOST}"),
            };
            ctx.run(ctx.nix().arg("repl").arg(flake(ctx, &attr)?))
        }
        Cmd::CachePush => cache_push(ctx),
        Cmd::New { lang, dir } => new_project(ctx, &lang, &dir),
        Cmd::Password => password(ctx),
        Cmd::Iso { flash, ventoy, wifi } => iso(ctx, flash.as_deref(), ventoy.as_deref(), wifi.as_deref()),
        Cmd::Preview { gpu, reset } => preview(ctx, gpu, reset),
        Cmd::Mac(m) => {
            ctx.need(Host::Mac)?;
            match m {
                MacCmd::Switch => switch(ctx),
                MacCmd::Build => build_and_diff(ctx),
                MacCmd::Adopt => mac::adopt(ctx),
                MacCmd::Undo { dry } => mac::undo(ctx, dry),
                MacCmd::Upgrade => mac::upgrade(ctx),
            }
        }
        Cmd::List => {
            use clap::CommandFactory;
            let _ = crate::Cli::command().print_help(); // a closed pipe (vx list | head) is fine
            Ok(())
        }
    }
}

/// `<repo>#<attr>`, checking the repo exists.
fn flake(ctx: &Ctx, attr: &str) -> Result<String> {
    Ok(format!("{}#{attr}", ctx.repo()?.display()))
}

/// This machine's system derivation in the flake.
fn system_attr(ctx: &Ctx) -> String {
    match ctx.host {
        Host::NixOs => format!("nixosConfigurations.{}.config.system.build.toplevel", ctx.nixos_host()),
        Host::Mac => format!("darwinConfigurations.{MAC_HOST}.system"),
    }
}

/// `nh os <action> <repo>` on a NixOS machine.
fn nh_os(ctx: &Ctx, action: &str) -> Result<()> {
    if ctx.host == Host::Mac {
        bail!("`vx {action}` is for the NixOS machines; on the Mac use vx switch or vx build");
    }
    let repo = ctx.repo()?.to_path_buf();
    // -H: the hostname may differ from the configuration's name (each host is its own nixosConfiguration).
    ctx.run(ctx.cmd("nh").args(["os", action]).arg(repo).args(["-H", ctx.nixos_host()]))
}

/// darwin-rebuild, preferring the running system's own copy (sudo's PATH may lack it).
fn darwin_rebuild(ctx: &Ctx) -> Result<String> {
    let own = "/run/current-system/sw/bin/darwin-rebuild";
    if Path::new(own).exists() {
        return Ok(own.into());
    }
    if ctx::has("darwin-rebuild") {
        return Ok("darwin-rebuild".into());
    }
    bail!(
        "nix-darwin isn't installed yet. first time: sudo nix run nix-darwin -- switch --flake {}#{MAC_HOST}",
        ctx.repo.display()
    )
}

/// Build and switch this machine.
fn switch(ctx: &Ctx) -> Result<()> {
    match ctx.host {
        Host::NixOs => nh_os(ctx, "switch"),
        Host::Mac => {
            let f = flake(ctx, MAC_HOST)?;
            let dr = darwin_rebuild(ctx)?;
            ctx.run(ctx.sudo(&dr).args(["switch", "--flake", &f]))
        }
    }
}

/// Build this machine's config from the working tree; the store path.
fn build_system(ctx: &Ctx) -> Result<String> {
    let f = flake(ctx, &system_attr(ctx))?;
    let out = ctx.output(ctx.nix().args(["build", &f, "--no-link", "--print-out-paths"]))?;
    out.lines().last().map(str::to_string).context("nix build printed no path")
}

/// The running system, if there is one (none on a Mac before nix-darwin).
fn current_system() -> Option<PathBuf> {
    std::fs::canonicalize("/run/current-system").ok()
}

/// Package-level diff between two systems: nvd, else dix (what nh uses), else nix's own.
fn show_diff(ctx: &Ctx, old: &Path, new: &Path) -> Result<()> {
    let mut c = if ctx::has("nvd") {
        let mut c = ctx.cmd("nvd");
        c.arg("diff");
        c
    } else if ctx::has("dix") {
        ctx.cmd("dix")
    } else {
        let mut c = ctx.nix();
        c.args(["store", "diff-closures"]);
        c
    };
    ctx.run(c.arg(old).arg(new))
}

/// Build the working tree and show what it changes vs the running system.
/// Returns the new path and whether it differs.
fn build_and_diff_path(ctx: &Ctx) -> Result<(String, bool)> {
    let new = build_system(ctx)?;
    ui::info(&format!("built {new}"));
    let Some(cur) = current_system() else {
        ui::info("nothing running to compare with (first switch)");
        return Ok((new, true));
    };
    if cur == Path::new(&new) {
        ui::ok("same as the running system: nothing would change");
        return Ok((new, false));
    }
    show_diff(ctx, &cur, Path::new(&new))?;
    Ok((new, true))
}

fn build_and_diff(ctx: &Ctx) -> Result<()> {
    build_and_diff_path(ctx).map(|_| ())
}

/// The last two `system-N-link` generations, oldest first.
pub fn last_two_generations(names: &[String]) -> Option<(String, String)> {
    let re = regex::Regex::new(r"^system-(\d+)-link$").ok()?;
    let mut gens: Vec<(u64, &String)> =
        names.iter().filter_map(|n| re.captures(n).and_then(|c| c[1].parse().ok()).map(|g| (g, n))).collect();
    gens.sort();
    let n = gens.len();
    (n >= 2).then(|| (gens[n - 2].1.clone(), gens[n - 1].1.clone()))
}

fn diff_generations(ctx: &Ctx) -> Result<()> {
    let dir = Path::new("/nix/var/nix/profiles");
    let names: Vec<String> = std::fs::read_dir(dir)
        .context("no /nix/var/nix/profiles")?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    let (a, b) = last_two_generations(&names).context("fewer than two system generations")?;
    ui::info(&format!("{a} -> {b}"));
    show_diff(ctx, &dir.join(a), &dir.join(b))
}

/// What moved in flake.lock: (name, `12 d old -> 0 s old (abc1234 -> def5678)`).
fn lock_changes(before: &str, after: &str) -> Vec<(String, String)> {
    let old = util::lock_inputs(before);
    let now = util::now();
    let short = |r: &str| r.chars().take(7).collect::<String>();
    util::lock_inputs(after)
        .into_iter()
        .filter_map(|n| {
            let o = old.iter().find(|o| o.name == n.name);
            if o.map(|o| o.rev == n.rev).unwrap_or(false) {
                return None;
            }
            let age = |t: u64| util::ago(now.saturating_sub(t));
            Some(match o {
                Some(o) => (n.name.clone(), format!("{} old -> {} old ({} -> {})", age(o.last_modified), age(n.last_modified), short(&o.rev), short(&n.rev))),
                None => (n.name.clone(), "new".into()),
            })
        })
        .collect()
}

fn update(ctx: &Ctx, inputs: &[String], yes: bool) -> Result<()> {
    let repo = ctx.repo()?.to_path_buf();
    let lock = repo.join("flake.lock");
    let before = std::fs::read_to_string(&lock).unwrap_or_default();
    ctx.run(ctx.nix().args(["flake", "update"]).args(inputs).arg("--flake").arg(&repo))?;
    let after = std::fs::read_to_string(&lock).unwrap_or_default();
    if before == after {
        ui::ok("flake.lock is already up to date");
        return Ok(());
    }
    ui::heading("inputs");
    let changes = lock_changes(&before, &after);
    let rows: Vec<(&str, String)> = changes.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    ui::kv(&rows);
    ui::heading("system");
    let (_, changed) = build_and_diff_path(ctx)?;
    if !changed {
        ui::info("flake.lock moved but the system is the same; commit it with vx commit");
        return Ok(());
    }
    if yes || ui::confirm("switch to it now?") {
        switch(ctx)
    } else {
        ui::info("not switched. later: vx switch (or undo: git checkout flake.lock)");
        Ok(())
    }
}

fn fmt(ctx: &Ctx) -> Result<()> {
    ctx.repo()?;
    ctx.run(ctx.nix().arg("fmt"))
}

fn check(ctx: &Ctx) -> Result<()> {
    ctx.run(ctx.nix().args(["flake", "check", "--no-build", "--all-systems"]))?;
    let f = flake(ctx, &format!("darwinConfigurations.{MAC_HOST}.system.drvPath"))?;
    ctx.output(ctx.nix().args(["eval", "--raw", &f]))?;
    ui::ok("everything evaluates");
    Ok(())
}

fn commit(ctx: &Ctx, message: &str) -> Result<()> {
    ctx.run(ctx.cmd("git").args(["add", "-A"]))?;
    ctx.run(ctx.cmd("git").args(["commit", "-m", message]))
}

fn push(ctx: &Ctx) -> Result<()> {
    ctx.repo()?;
    ctx.run(ctx.cmd("git").arg("push"))
}

fn is_dirty(ctx: &Ctx) -> Result<bool> {
    Ok(!ctx.output(ctx.cmd("git").args(["status", "--porcelain"]))?.trim().is_empty())
}

/// Local commits (a wallpaper picked on this machine, say) go on top of what
/// was pulled instead of stopping the pull; uncommitted edits are kept too.
const PULL: [&str; 3] = ["pull", "--rebase", "--autostash"];

fn pull(ctx: &Ctx) -> Result<()> {
    let repo = ctx.repo()?.to_path_buf();
    let before = ctx.output(ctx.cmd("git").args(["rev-parse", "HEAD"]))?;
    ctx.run(ctx.cmd("git").args(PULL).arg("--quiet"))?;
    let after = ctx.output(ctx.cmd("git").args(["rev-parse", "HEAD"]))?;
    if before == after {
        ui::ok(&format!("{} is up to date", repo.display()));
        return Ok(());
    }
    ctx.run(ctx.cmd("git").args(["log", "--oneline", "--no-decorate", &format!("{before}..{after}")]))?;
    ui::ok("pulled. vx switch applies it");
    Ok(())
}

fn sync(ctx: &Ctx, message: &str, yes: bool) -> Result<()> {
    ctx.repo()?;
    ui::heading("format");
    fmt(ctx)?;
    ui::heading("check");
    check(ctx)?;
    if is_dirty(ctx)? {
        ui::heading("commit");
        ctx.run(ctx.cmd("git").args(["status", "--short"]))?;
        let msg = if message.trim().is_empty() { util::prompt("commit message: ")? } else { message.to_string() };
        if msg.trim().is_empty() {
            bail!("no commit message; nothing committed");
        }
        commit(ctx, msg.trim())?;
    }
    ui::heading("push");
    push(ctx)?;
    // The Mac holds the editing key: re-encrypt for machines added to .sops.yaml.
    if ctx.host == Host::Mac && super::care::secrets::mac_age_key().is_some() {
        ui::heading("secrets");
        if let Err(e) = super::care::secrets::rekey(ctx, true) {
            ui::warn(&format!("couldn't re-encrypt the secrets: {e:#}"));
        }
    }
    ui::heading("switch");
    switch(ctx)?;
    let other = match ctx.host {
        Host::NixOs => "mac",
        Host::Mac => "arc",
    };
    if yes || ui::confirm(&format!("pull and switch {other} too?")) {
        remote(other, &["switch".into(), "--pull".into()])?;
    }
    Ok(())
}

fn deploy(ctx: &Ctx, boot: bool) -> Result<()> {
    if ctx.is_nixos() {
        bail!("you're on a NixOS machine: vx switch (vx sync also pushes)");
    }
    push(ctx)?;
    let action = if boot { "boot" } else { "switch" };
    // Same command as the recipe; it parses in fish (arc's login shell) and sh.
    let script = format!("cd ~/nixos && git pull --rebase --autostash && nh os {action} .");
    ctx.run(Command::new("ssh").arg("-t").arg(ssh_target("arc")?).arg(script))
}

fn cache_push(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let meta = util::meta_src(ctx.repo()?);
    let Some((_, cache)) = util::binary_cache(&meta) else {
        bail!("the homelab cache is off (meta.binaryCache = null in modules/flake/meta.nix)");
    };
    ctx.run(ctx.sudo("env").args(["HOME=/var/lib/attic-push", "attic", "push", &format!("homelab:{cache}"), "/run/current-system"]))
}

fn new_project(ctx: &Ctx, lang: &str, dir: &Path) -> Result<()> {
    let repo = ctx.repo()?;
    let have: Vec<String> = std::fs::read_dir(repo.join("templates"))
        .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    if !have.iter().any(|h| h == lang) {
        let mut have = have;
        have.sort();
        bail!("no template '{lang}'. there are: {}", have.join(" "));
    }
    // Relative to where you ran vx, not the repo.
    let dir = std::env::current_dir()?.join(dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    let here = |p: &str| {
        let mut c = Command::new(p);
        c.current_dir(&dir);
        c
    };
    let mut init = here("nix");
    init.args(["--extra-experimental-features", "nix-command flakes", "flake", "init", "-t", &flake(ctx, lang)?]);
    ctx.run(&mut init)?;
    if !dir.join(".git").exists() {
        ctx.run(here("git").args(["init", "-q"]))?;
    }
    ctx.run(here("git").args(["add", "-A"]))?; // flakes only see files git knows about
    if ctx::has("direnv") {
        ctx.run(here("direnv").arg("allow"))?;
    } else {
        ui::info("direnv isn't installed here; run: nix develop");
    }
    ui::ok(&format!("ready in {}. the toolchain loads when you cd in", dir.display()));
    Ok(())
}

fn password(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let a = util::prompt_hidden("New password: ")?;
    let b = util::prompt_hidden("Repeat:       ")?;
    if a.is_empty() || a != b {
        bail!("empty or didn't match. nothing changed");
    }
    let hash = util::pipe(Command::new("mkpasswd").args(["-m", "yescrypt", "--stdin"]), a.as_bytes())?;
    let user = std::env::var("USER").context("USER is not set")?;
    let mut tee = ctx.sudo("tee");
    tee.arg(format!("/persistent/passwords/{user}"));
    util::pipe(&mut tee, &hash)?;
    ui::ok("password updated (applies at next login)");
    Ok(())
}

fn iso(ctx: &Ctx, flash: Option<&str>, ventoy: Option<&str>, wifi: Option<&str>) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let f = flake(ctx, "nixosConfigurations.installer.config.system.build.isoImage")?;
    let mut build = ctx.nix();
    build.args(["build", &f]);
    // The Wi-Fi keyfile goes in through $VX_ISO_WIFI (modules/hosts/installer.nix), which needs --impure.
    let dir = match wifi {
        Some(name) => Some(iso_wifi(ctx, name)?),
        None => None,
    };
    if let Some(d) = &dir {
        build.arg("--impure").env("VX_ISO_WIFI", d);
    }
    let built = ctx.run(&mut build);
    if let Some(d) = &dir {
        let _ = std::fs::remove_dir_all(d);
    }
    built?;
    let mut image = None;
    for e in std::fs::read_dir(ctx.repo.join("result/iso"))?.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "iso") {
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            ui::ok(&format!("{} ({:.1} GiB)", p.display(), size as f64 / (1u64 << 30) as f64));
            image = Some(p);
        }
    }
    if let Some(dir) = ventoy {
        let image = image.context("the build made no .iso")?;
        return copy_to_ventoy(ctx, &image, dir);
    }
    let Some(dev) = flash else {
        ui::dim("put it on a usb stick: vx iso --flash /dev/sdX (lsblk shows which), or vx iso --ventoy");
        return Ok(());
    };
    let image = image.context("the build made no .iso")?;
    flash_usb(ctx, &image, dev)
}

/// Copy the ISO onto a Ventoy stick's data partition (label "Ventoy"),
/// replacing the previous copy of the same name. Mounts it (udisks) if it
/// isn't mounted, and unmounts it again afterwards. `dir`: a mounted folder to
/// use instead of looking for the label.
fn copy_to_ventoy(ctx: &Ctx, image: &Path, dir: &str) -> Result<()> {
    let mut mounted_here = None;
    let root = if !dir.is_empty() {
        let d = PathBuf::from(dir);
        if !d.is_dir() {
            bail!("{dir} is not a folder");
        }
        d
    } else {
        let parts = ctx.output(Command::new("lsblk").args(["-nrpo", "NAME,LABEL,MOUNTPOINT"]))?;
        let (dev, mp) = ventoy_partition(&parts).context(
            "no ventoy stick found (no partition labelled Ventoy): plug it in, or pass its folder: vx iso --ventoy /run/media/...",
        )?;
        match mp {
            Some(mp) => PathBuf::from(mp),
            None => {
                let out = ctx
                    .output(Command::new("udisksctl").args(["mount", "--no-user-interaction", "-b", &dev]))
                    .with_context(|| format!("couldn't mount {dev}; mount it yourself and pass the folder: vx iso --ventoy <folder>"))?;
                mounted_here = Some(dev.clone());
                // "Mounted /dev/sdb1 at /run/media/vainnor/Ventoy"
                PathBuf::from(out.rsplit(" at ").next().unwrap_or("").trim().trim_end_matches('.'))
            }
        }
    };
    let name = image.file_name().context("the image has no name")?;
    let dest = root.join(name);
    ui::info(&format!("copying to {} ...", dest.display()));
    // A temporary name first: a half-copied image never shows up in Ventoy's menu.
    let tmp = root.join(format!(".{}.part", name.to_string_lossy()));
    let copied = std::fs::copy(image, &tmp).and_then(|_| std::fs::rename(&tmp, &dest));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&tmp);
        if e.kind() != std::io::ErrorKind::PermissionDenied {
            return Err(e).with_context(|| format!("couldn't copy to {}", dest.display()));
        }
        ctx.run(ctx.sudo("cp").arg(image).arg(&dest))?;
    }
    ctx.run(&mut Command::new("sync"))?;
    if let Some(dev) = mounted_here {
        let _ = ctx.run(Command::new("udisksctl").args(["unmount", "--no-user-interaction", "-b", &dev]));
    }
    ui::ok(&format!("{} is on the ventoy stick: pick it in ventoy's menu (uefi, secure boot off)", name.to_string_lossy()));
    Ok(())
}

/// `lsblk -nrpo NAME,LABEL,MOUNTPOINT` -> the Ventoy data partition (device, mountpoint).
/// lsblk -r escapes spaces as \x20.
fn ventoy_partition(lsblk: &str) -> Option<(String, Option<String>)> {
    lsblk.lines().find_map(|l| {
        let f: Vec<&str> = l.split(' ').collect();
        let (name, label) = (*f.first()?, *f.get(1)?);
        let mp = f.get(2).map(|m| m.replace("\\x20", " ")).filter(|m| !m.is_empty());
        (label == "Ventoy").then(|| (name.to_string(), mp))
    })
}

/// A folder holding one NetworkManager keyfile for `name` (empty: the Wi-Fi this
/// PC is on), for the ISO. A saved network's password comes from
/// NetworkManager (sudo); any other network is asked for.
fn iso_wifi(ctx: &Ctx, name: &str) -> Result<PathBuf> {
    use crate::cmd::install::engine::wifi::{self, Security};
    let saved: Vec<(String, String)> = ctx
        .probe(Command::new("nmcli").args(["-t", "-f", "NAME,TYPE,ACTIVE", "connection", "show"]))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| match wifi::split_terse(l).as_slice() {
            [n, ty, active] if ty == "802-11-wireless" => Some((n.clone(), active.clone())),
            _ => None,
        })
        .collect();
    let name = if name.is_empty() {
        match saved.iter().find(|(_, a)| a == "yes") {
            Some((n, _)) => n.clone(),
            None => bail!("this pc isn't on wi-fi: name the network, vx iso --wifi \"<network>\""),
        }
    } else {
        name.to_string()
    };
    let (ssid, security, psk) = if saved.iter().any(|(n, _)| *n == name) {
        let out = ctx.output(ctx.sudo("nmcli").args([
            "-s",
            "-t",
            "-g",
            "802-11-wireless.ssid,802-11-wireless-security.key-mgmt,802-11-wireless-security.psk",
            "connection",
            "show",
            "id",
            &name,
        ]))?;
        let f: Vec<String> = out.lines().map(|l| l.replace("\\:", ":").replace("\\\\", "\\")).collect();
        let [ssid, mgmt, psk] = f.as_slice() else { bail!("couldn't read the saved network {name}") };
        let sec = Security::from_key_mgmt(mgmt);
        if sec == Security::Unsupported {
            bail!("{name} uses a login (key-mgmt {mgmt}) the installer can't carry; connect on the stick with nmtui");
        }
        (ssid.clone(), sec, (!psk.is_empty()).then(|| psk.clone()))
    } else {
        ui::info(&format!("{name} isn't saved on this pc"));
        let pw = util::prompt_hidden(&format!("password for {name} (empty for an open network): "))?;
        if !pw.is_empty() && !(8..=63).contains(&pw.len()) {
            bail!("a wi-fi password is 8 to 63 characters");
        }
        let sec = if pw.is_empty() { Security::Open } else { Security::Psk };
        (name.clone(), sec, (!pw.is_empty()).then_some(pw))
    };
    // Saved without its password (kept in a desktop keyring instead): ask.
    let psk = match psk {
        None if security.needs_password() => {
            let pw = util::prompt_hidden(&format!("password for {ssid}: "))?;
            if !(8..=63).contains(&pw.len()) {
                bail!("a wi-fi password is 8 to 63 characters");
            }
            Some(pw)
        }
        p => p,
    };
    let dir = std::env::temp_dir().join(format!("vx-iso-wifi-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    let file = dir.join(format!("{}.nmconnection", wifi::file_stem(&ssid)));
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&file)?;
        f.write_all(wifi::keyfile(&ssid, security, psk.as_deref(), &wifi::new_uuid()).as_bytes())?;
    }
    ui::ok(&format!("the stick connects to {ssid} by itself"));
    ui::warn("its wi-fi password is on the stick and in this pc's nix store: keep the stick to yourself");
    Ok(dir)
}

/// Write the ISO to a USB stick, refusing anything that isn't a removable/USB whole disk.
fn flash_usb(ctx: &Ctx, image: &std::path::Path, dev: &str) -> Result<()> {
    let info = ctx.output(Command::new("lsblk").args(["-dnbo", "TYPE,TRAN,RM,SIZE,MODEL", dev]))?;
    let (ok, why) = flash_target_ok(&info);
    if !ok {
        bail!("{dev} {why}; pick the usb stick from `lsblk -d -o NAME,SIZE,TRAN,MODEL`");
    }
    let mounted = ctx.output(Command::new("lsblk").args(["-nro", "MOUNTPOINTS", dev]))?;
    if mounted.lines().any(|l| l.trim() == "/" || l.trim().starts_with("/nix") || l.trim().starts_with("/boot")) {
        bail!("{dev} holds the running system; refusing");
    }
    let _ = ctx.run(Command::new("lsblk").args(["-o", "NAME,SIZE,TRAN,MODEL,LABEL,MOUNTPOINTS", dev]));
    let labels = ctx.probe(Command::new("lsblk").args(["-nro", "LABEL", dev])).unwrap_or_default();
    if labels.lines().any(|l| l == "Ventoy" || l == "VTOYEFI") {
        ui::warn(&format!("{dev} is a ventoy stick: flashing replaces ventoy and every iso on it. vx iso --ventoy copies the iso onto it instead"));
    }
    ui::warn(&format!("everything on {dev} will be erased"));
    print!("type the device name ({dev}) to continue: ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    if s.trim() != dev {
        bail!("not confirmed; nothing was written");
    }
    // Unmount anything the desktop auto-mounted from the stick.
    for m in mounted.lines().map(str::trim).filter(|m| !m.is_empty()) {
        let _ = ctx.run(ctx.sudo("umount").arg(m));
    }
    ctx.run(ctx.sudo("dd").arg(format!("if={}", image.display())).arg(format!("of={dev}")).args([
        "bs=4M",
        "conv=fsync",
        "oflag=direct",
        "status=progress",
    ]))?;
    ui::ok(&format!("{dev} is ready: boot it in uefi mode (secure boot off), the installer starts by itself"));
    Ok(())
}

/// `lsblk -dnbo TYPE,TRAN,RM,SIZE,MODEL` output -> (ok, reason when not).
fn flash_target_ok(info: &str) -> (bool, &'static str) {
    let f: Vec<&str> = info.split_whitespace().collect();
    match f.as_slice() {
        [ty, ..] if *ty != "disk" => (false, "is not a whole disk"),
        [_, tran, rm, size, ..] => {
            let usb = *tran == "usb" || *rm == "1";
            let size: u64 = size.parse().unwrap_or(0);
            if !usb {
                (false, "is not a usb / removable drive")
            } else if size > 256 * (1u64 << 30) {
                (false, "is bigger than 256 GiB (not a usb stick?)")
            } else {
                (true, "")
            }
        }
        // No TRAN column (some USB readers): only RM is trustworthy then.
        [_, rm, size] => flash_target_ok(&format!("disk none {rm} {size}")),
        _ => (false, "was not found"),
    }
}

fn preview(ctx: &Ctx, gpu: Gpu, reset: bool) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let home = PathBuf::from(std::env::var("HOME")?);
    let cache = home.join(".cache/arc-preview");
    let disk = cache.join("arc.qcow2");
    if reset {
        match std::fs::remove_file(&disk) {
            Ok(()) => ui::ok("preview disk removed: next boot is a fresh first boot"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ui::ok("no preview disk to remove"),
            Err(e) => return Err(e).context(format!("could not remove {}", disk.display())),
        }
        return Ok(());
    }
    let f = flake(ctx, &format!("nixosConfigurations.{}.config.system.build.vm", ctx.nixos_host()))?;
    ctx.run(ctx.nix().args(["build", &f, "-o", "/tmp/arc-preview"]))?;
    std::fs::create_dir_all(&cache)?;
    let wsl = std::fs::read_to_string("/proc/version").map(|v| v.to_lowercase().contains("microsoft")).unwrap_or(false);
    let soft = match gpu {
        Gpu::Soft => true,
        Gpu::Gl => false,
        Gpu::Auto => wsl, // WSLg has no GL for QEMU
    };
    let (dev, disp) = if soft { ("virtio-vga", "gtk") } else { ("virtio-vga-gl", "gtk,gl=on") };
    if !Path::new("/dev/kvm").exists() {
        ui::warn("no /dev/kvm here: it will run, but slowly");
    }
    let mut c = Command::new(format!("/tmp/arc-preview/bin/run-{}-vm", ctx.nixos_host()));
    c.env("NIX_DISK_IMAGE", &disk).env("QEMU_OPTS", format!("-vga none -device {dev} -display {disp}"));
    ctx.run(&mut c)
}

/// The SSH destination for a machine: arc and the Mac have fixed NetBird
/// names, any other host is `<user>@<name>.netbird.cloud`. VX_<NAME>_SSH overrides.
fn ssh_target(target: &str) -> Result<String> {
    let target = if target == "pc" { "arc" } else { target };
    let valid = !target.is_empty()
        && target.len() <= 31
        && target.starts_with(|c: char| c.is_ascii_lowercase())
        && target.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        bail!("unknown machine @{target}: use @arc, @mac or a host name from modules/hosts");
    }
    let var = format!("VX_{}_SSH", target.to_uppercase().replace('-', "_"));
    if let Some(v) = std::env::var(&var).ok().filter(|s| !s.is_empty()) {
        return Ok(v);
    }
    let user = std::env::var("USER").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| "vainnor".into());
    Ok(default_ssh(target, &user))
}

/// `vx @<name>`'s destination without overrides.
pub fn default_ssh(target: &str, user: &str) -> String {
    match target {
        "arc" => ARC_SSH.to_string(),
        "mac" => MAC_SSH.to_string(),
        other => format!("{user}@{other}.netbird.cloud"),
    }
}

/// The command line run on the other machine. vx is called by its
/// home-manager path first (non-interactive shells may not have it on PATH,
/// e.g. Homebrew fish on the Mac), then from PATH. `&&`/`||`/`exec` parse the
/// same in fish and sh; exec means a failing vx doesn't run the fallback.
pub fn remote_command(user: &str, bin: Option<&str>, args: &[String]) -> String {
    let quoted: Vec<String> = args.iter().map(|a| util::shell_quote(a)).collect();
    let rest = if quoted.is_empty() { String::new() } else { format!(" {}", quoted.join(" ")) };
    if let Some(bin) = bin {
        return format!("exec {}{rest}", util::shell_quote(bin));
    }
    let hm = util::shell_quote(&format!("/etc/profiles/per-user/{user}/bin/vx"));
    format!("test -x {hm} && exec {hm}{rest} || exec vx{rest}")
}

/// `vx @arc ...` / `vx @<host> ...`: run vx on another machine.
pub fn remote(target: &str, args: &[String]) -> Result<()> {
    let here = if cfg!(target_os = "macos") { "mac".to_string() } else { ctx::nixos_host() };
    let target = if target == "pc" { "arc" } else { target };
    if target == here {
        // Already here: just run it.
        let status = Command::new(std::env::current_exe()?).args(args).status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
    let dest = ssh_target(target)?;
    let user = dest.split_once('@').map(|(u, _)| u.to_string()).or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "vainnor".into());
    let mut c = Command::new("ssh");
    if std::io::stdin().is_terminal() {
        c.arg("-t");
    }
    let bin = std::env::var("VX_REMOTE_BIN").ok().filter(|s| !s.is_empty());
    c.arg(&dest).arg(remote_command(&user, bin.as_deref(), args));
    if std::env::args().any(|a| a == "-v" || a == "--verbose") {
        ui::dim(&format!("$ ssh {dest} {}", remote_command(&user, bin.as_deref(), args)));
    }
    let status = c.status().context("could not start ssh")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(255) => bail!("ssh to {dest} failed. is NetBird up on both, and is ssh on there? (override with VX_{}_SSH)", target.to_uppercase()),
        Some(code) => std::process::exit(code),
        None => bail!("ssh was killed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generations() {
        let names: Vec<String> = ["system", "system-9-link", "system-10-link", "system-2-link", "default"].iter().map(|s| s.to_string()).collect();
        assert_eq!(last_two_generations(&names), Some(("system-9-link".into(), "system-10-link".into())));
        assert_eq!(last_two_generations(&names[..2]), None);
    }

    #[test]
    fn remote_cmd() {
        assert_eq!(default_ssh("arc", "x"), ARC_SSH);
        assert_eq!(default_ssh("mac", "x"), MAC_SSH);
        assert_eq!(default_ssh("forge", "someone"), "someone@forge.netbird.cloud");
        assert!(ssh_target("Bad!").is_err());
        assert_eq!(ctx::host_from(Some("forge\n")), "forge");
        assert_eq!(ctx::host_from(Some("NixOS box")), "arc");
        assert_eq!(ctx::host_from(None), "arc");
        let c = remote_command("vainnor", None, &["commit".into(), "fix: it's done".into()]);
        assert_eq!(
            c,
            r"test -x /etc/profiles/per-user/vainnor/bin/vx && exec /etc/profiles/per-user/vainnor/bin/vx commit 'fix: it'\''s done' || exec vx commit 'fix: it'\''s done'"
        );
        assert_eq!(remote_command("vainnor", None, &[]), "test -x /etc/profiles/per-user/vainnor/bin/vx && exec /etc/profiles/per-user/vainnor/bin/vx || exec vx");
    }

    #[test]
    fn lock_diff() {
        let a = r#"{"root":"root","nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs","hm":"hm"}},"nixpkgs":{"locked":{"lastModified":1,"rev":"aaaaaaaaa"}},"hm":{"locked":{"lastModified":1,"rev":"same"}}}}"#;
        let b = r#"{"root":"root","nodes":{"root":{"inputs":{"nixpkgs":"nixpkgs","hm":"hm"}},"nixpkgs":{"locked":{"lastModified":2,"rev":"bbbbbbbbb"}},"hm":{"locked":{"lastModified":1,"rev":"same"}}}}"#;
        let c = lock_changes(a, b);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, "nixpkgs");
        assert!(c[0].1.ends_with("(aaaaaaa -> bbbbbbb)"), "{}", c[0].1);
    }
}

#[cfg(test)]
mod flash_tests {
    use super::{flash_target_ok, ventoy_partition};

    #[test]
    fn finds_the_ventoy_partition() {
        let l = "/dev/nvme0n1  \n/dev/sdb  \n/dev/sdb1 Ventoy /run/media/vainnor/Ventoy\n/dev/sdb2 VTOYEFI \n";
        assert_eq!(ventoy_partition(l), Some(("/dev/sdb1".into(), Some("/run/media/vainnor/Ventoy".into()))));
        assert_eq!(ventoy_partition("/dev/sdb1 Ventoy \n"), Some(("/dev/sdb1".into(), None)));
        assert_eq!(ventoy_partition("/dev/sdc1 Ventoy /run/media/my\\x20disk\n").unwrap().1.as_deref(), Some("/run/media/my disk"));
        assert_eq!(ventoy_partition("/dev/sdb1 USB /mnt\n"), None);
    }

    #[test]
    fn only_usb_sticks() {
        assert!(flash_target_ok("disk usb 0 32017047552 SanDisk_3.2Gen1").0);
        assert!(flash_target_ok("disk sata 1 15931539456 Card_Reader").0);
        assert!(!flash_target_ok("disk nvme 0 1000204886016 CT1000P310SSD8").0);
        assert!(!flash_target_ok("part usb 0 32017047552").0);
        assert!(!flash_target_ok("disk usb 0 2000398934016 Seagate_Expansion").0);
        assert!(!flash_target_ok("").0);
    }
}
