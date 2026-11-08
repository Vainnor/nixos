//! Rebuilding, updating and checking the system, on the PC (nh) and the Mac
//! (darwin-rebuild), plus `vx @arc ...` / `vx @mac ...` over SSH.

mod doctor;
mod mac;
mod status;
mod util;

use crate::ctx::{self, ARC_SSH, Ctx, Host, MAC_HOST, MAC_SSH, NIXOS_HOST};
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
    /// Build and switch to the current config (darwin-rebuild on the Mac)
    Switch {
        /// git pull --ff-only first
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
    /// Build the custom installer ISO (x86_64 Linux)
    Iso,
    /// Boot arc's config in a VM window (any Linux with Nix; login vainnor / preview)
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
        Cmd::Switch { pull } => {
            if pull {
                ctx.run(ctx.cmd("git").args(["pull", "--ff-only"]))?;
            }
            switch(ctx)
        }
        Cmd::Test => nh_os(ctx, "test"),
        Cmd::Boot => nh_os(ctx, "boot"),
        Cmd::Build => match ctx.host {
            Host::Arc => nh_os(ctx, "build"),
            Host::Mac => build_and_diff(ctx),
        },
        Cmd::Update { inputs, yes } => update(ctx, &inputs, yes),
        Cmd::Upgrade { inputs } => update(ctx, &inputs, true),
        Cmd::Rollback => match ctx.host {
            Host::Arc => ctx.run(ctx.sudo("nixos-rebuild").args(["switch", "--rollback"])),
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
                Host::Arc => format!("nixosConfigurations.{NIXOS_HOST}"),
                Host::Mac => format!("darwinConfigurations.{MAC_HOST}"),
            };
            ctx.run(ctx.nix().arg("repl").arg(flake(ctx, &attr)?))
        }
        Cmd::CachePush => cache_push(ctx),
        Cmd::New { lang, dir } => new_project(ctx, &lang, &dir),
        Cmd::Password => password(ctx),
        Cmd::Iso => iso(ctx),
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
        Host::Arc => format!("nixosConfigurations.{NIXOS_HOST}.config.system.build.toplevel"),
        Host::Mac => format!("darwinConfigurations.{MAC_HOST}.system"),
    }
}

/// `nh os <action> <repo>` on the PC.
fn nh_os(ctx: &Ctx, action: &str) -> Result<()> {
    if ctx.host == Host::Mac {
        bail!("`vx {action}` is for the PC; on the Mac use vx switch or vx build");
    }
    let repo = ctx.repo()?.to_path_buf();
    // -H: the hostname may differ from the configuration's name (the installer can rename the PC).
    ctx.run(ctx.cmd("nh").args(["os", action]).arg(repo).args(["-H", NIXOS_HOST]))
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
        Host::Arc => nh_os(ctx, "switch"),
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
    ui::heading("switch");
    switch(ctx)?;
    let other = match ctx.host {
        Host::Arc => "mac",
        Host::Mac => "arc",
    };
    if yes || ui::confirm(&format!("pull and switch {other} too?")) {
        remote(other, &["switch".into(), "--pull".into()])?;
    }
    Ok(())
}

fn deploy(ctx: &Ctx, boot: bool) -> Result<()> {
    if ctx.is_arc() {
        bail!("you're on the PC: vx switch (vx sync also pushes)");
    }
    push(ctx)?;
    let action = if boot { "boot" } else { "switch" };
    // Same command as the recipe; it parses in fish (arc's login shell) and sh.
    let script = format!("cd ~/nixos && git pull --ff-only && nh os {action} .");
    ctx.run(Command::new("ssh").arg("-t").arg(ssh_target("arc")?).arg(script))
}

fn cache_push(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::Arc)?;
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
    ctx.need(Host::Arc)?;
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

fn iso(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::Arc)?;
    let f = flake(ctx, "nixosConfigurations.installer.config.system.build.isoImage")?;
    ctx.run(ctx.nix().args(["build", &f]))?;
    for e in std::fs::read_dir(ctx.repo.join("result/iso"))?.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "iso") {
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            ui::ok(&format!("{} ({:.1} GiB)", p.display(), size as f64 / (1u64 << 30) as f64));
        }
    }
    Ok(())
}

fn preview(ctx: &Ctx, gpu: Gpu, reset: bool) -> Result<()> {
    ctx.need(Host::Arc)?;
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
    let f = flake(ctx, &format!("nixosConfigurations.{NIXOS_HOST}.config.system.build.vm"))?;
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
    let mut c = Command::new(format!("/tmp/arc-preview/bin/run-{NIXOS_HOST}-vm"));
    c.env("NIX_DISK_IMAGE", &disk).env("QEMU_OPTS", format!("-vga none -device {dev} -display {disp}"));
    ctx.run(&mut c)
}

/// The SSH destination for a machine; VX_ARC_SSH / VX_MAC_SSH override.
fn ssh_target(target: &str) -> Result<String> {
    let (var, default) = match target {
        "arc" | "pc" => ("VX_ARC_SSH", ARC_SSH),
        "mac" => ("VX_MAC_SSH", MAC_SSH),
        other => bail!("unknown machine @{other}: use @arc or @mac"),
    };
    Ok(std::env::var(var).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_string()))
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

/// `vx @arc ...`: run vx on another machine.
pub fn remote(target: &str, args: &[String]) -> Result<()> {
    let here = if cfg!(target_os = "macos") { "mac" } else { "arc" };
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
