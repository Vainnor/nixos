//! Shared context: where the repo is, which machine we're on, and helpers to
//! run programs. Every command goes through these so `-v` and errors behave
//! the same everywhere.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    /// A NixOS machine of this flake (arc, or any other host in modules/hosts).
    NixOs,
    /// The MacBook (nix-darwin).
    Mac,
}

pub struct Ctx {
    pub repo: PathBuf,
    pub host: Host,
    pub verbose: bool,
    /// `nixosConfigurations.<this>`: see [`nixos_host`].
    nixos_host: String,
}

/// The NixOS host used when this machine's own name isn't known (on the Mac,
/// `vx diff` / `vx preview` of the PC).
pub const DEFAULT_NIXOS_HOST: &str = "arc";
pub const MAC_HOST: &str = "Vainnors-MacBook-Pro";
/// SSH targets for `vx @arc ...` / `vx @mac ...` (NetBird names); other hosts
/// are `<user>@<name>.netbird.cloud`.
pub const ARC_SSH: &str = "vainnor@arc-211-139.netbird.cloud";
pub const MAC_SSH: &str = "vainnor@vainnors-macbook-pro.netbird.cloud";

/// The NixOS host to build: this machine's hostname on Linux (each host's
/// hostname is its name in the flake), else "arc".
pub fn nixos_host() -> String {
    if cfg!(target_os = "macos") {
        return DEFAULT_NIXOS_HOST.into();
    }
    host_from(std::fs::read_to_string("/proc/sys/kernel/hostname").ok().as_deref())
}

/// A hostname that can be a flake host, else the default.
pub fn host_from(hostname: Option<&str>) -> String {
    let h = hostname.map(str::trim).unwrap_or("");
    let ok = !h.is_empty() && h.len() <= 63 && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok { h.to_string() } else { DEFAULT_NIXOS_HOST.into() }
}

impl Ctx {
    pub fn new(repo: Option<PathBuf>, verbose: bool) -> Result<Self> {
        let host = if cfg!(target_os = "macos") { Host::Mac } else { Host::NixOs };
        let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
        let repo = repo.unwrap_or_else(|| match host {
            Host::Mac => home.join("Programing/nixos"),
            Host::NixOs => home.join("nixos"),
        });
        Ok(Self { repo, host, verbose, nixos_host: nixos_host() })
    }

    /// The NixOS host this works on: `nixosConfigurations.<this>`, `nh -H <this>`.
    pub fn nixos_host(&self) -> &str {
        &self.nixos_host
    }

    /// The repo, checked to exist (most commands need it).
    pub fn repo(&self) -> Result<&Path> {
        if !self.repo.join("flake.nix").exists() {
            bail!("no flake at {} (pass --repo or set VX_REPO)", self.repo.display());
        }
        Ok(&self.repo)
    }

    pub fn is_nixos(&self) -> bool {
        self.host == Host::NixOs
    }

    /// Fail with a clear message when a command only makes sense on one machine.
    pub fn need(&self, host: Host) -> Result<()> {
        if self.host != host {
            bail!("this only works on {}", match host { Host::NixOs => "a NixOS machine (arc, ...)", Host::Mac => "the Mac" });
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
