//! Shared context: where the repo is, which machine we're on, and helpers to
//! run programs. Every command goes through these so `-v` and errors behave
//! the same everywhere.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    /// The NixOS PC.
    Arc,
    /// The MacBook (nix-darwin).
    Mac,
}

pub struct Ctx {
    pub repo: PathBuf,
    pub host: Host,
    pub verbose: bool,
}

/// Names that match the flake outputs (`nixosConfigurations.arc`, `darwinConfigurations.<mac>`).
pub const NIXOS_HOST: &str = "arc";
pub const MAC_HOST: &str = "Vainnors-MacBook-Pro";
/// SSH targets for `vx @arc ...` / `vx @mac ...` (NetBird names).
pub const ARC_SSH: &str = "vainnor@arc-211-139.netbird.cloud";
pub const MAC_SSH: &str = "vainnor@vainnors-macbook-pro.netbird.cloud";

impl Ctx {
    pub fn new(repo: Option<PathBuf>, verbose: bool) -> Result<Self> {
        let host = if cfg!(target_os = "macos") { Host::Mac } else { Host::Arc };
        let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
        let repo = repo.unwrap_or_else(|| match host {
            Host::Mac => home.join("Programing/nixos"),
            Host::Arc => home.join("nixos"),
        });
        Ok(Self { repo, host, verbose })
    }

    /// The repo, checked to exist (most commands need it).
    pub fn repo(&self) -> Result<&Path> {
        if !self.repo.join("flake.nix").exists() {
            bail!("no flake at {} (pass --repo or set VX_REPO)", self.repo.display());
        }
        Ok(&self.repo)
    }

    pub fn is_arc(&self) -> bool {
        self.host == Host::Arc
    }

    /// Fail with a clear message when a command only makes sense on one machine.
    pub fn need(&self, host: Host) -> Result<()> {
        if self.host != host {
            bail!("this only works on {}", match host { Host::Arc => "the PC (arc)", Host::Mac => "the Mac" });
        }
        Ok(())
    }

    /// A Command that runs in the repo.
    pub fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.current_dir(&self.repo);
        c
    }

    /// `nix` with flakes enabled even where the system config doesn't enable them.
    pub fn nix(&self) -> Command {
        let mut c = self.cmd("nix");
        c.args(["--extra-experimental-features", "nix-command flakes"]);
        c
    }

    /// Run, inheriting the terminal. Errors if the program fails.
    pub fn run(&self, c: &mut Command) -> Result<()> {
        self.trace(c);
        let status = c.status().with_context(|| format!("could not start {}", prog(c)))?;
        if !status.success() {
            bail!("{} failed ({})", prog(c), status);
        }
        Ok(())
    }

    /// Run and return stdout (trimmed). stderr passes through.
    pub fn output(&self, c: &mut Command) -> Result<String> {
        self.trace(c);
        let out = c.stderr(Stdio::inherit()).output().with_context(|| format!("could not start {}", prog(c)))?;
        if !out.status.success() {
            bail!("{} failed ({})", prog(c), out.status);
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }

    /// Run quietly; Some(stdout) on success, None on failure or if it's missing.
    pub fn probe(&self, c: &mut Command) -> Option<String> {
        self.trace(c);
        let out = c.stderr(Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }

    /// `sudo <program> ...` as a Command.
    pub fn sudo(&self, program: &str) -> Command {
        let mut c = self.cmd("sudo");
        c.arg(program);
        c
    }

    fn trace(&self, c: &Command) {
        if self.verbose {
            let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
            crate::ui::dim(&format!("$ {} {}", prog(c), args.join(" ")));
        }
    }
}

fn prog(c: &Command) -> String {
    c.get_program().to_string_lossy().into_owned()
}

/// Is a program on PATH?
pub fn has(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
        .unwrap_or(false)
}
