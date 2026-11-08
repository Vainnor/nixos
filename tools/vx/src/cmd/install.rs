//! `vx install`: the full-screen installer (runs on the installer ISO).
//!
//! - `plan`   the contract: what the screens collect and the engine carries out
//! - `probe`  reads this machine: disks, previous installs, firmware   [ui]
//! - `ui`     the screens (ratatui)                                     [ui]
//! - `engine` unlocks / carries over / partitions / installs           [engine]

pub mod engine;
pub mod plan;
pub mod probe;
pub mod ui;

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

/// Erase drives you pick and install NixOS from this repo (on the installer ISO)
#[derive(Args, Debug, Clone)]
pub struct InstallArgs {
    /// Fake disks and a previous install, and print commands instead of running them (try it anywhere)
    #[arg(long)]
    pub demo: bool,

    /// Real disks, but print every command instead of running it
    #[arg(long)]
    pub dry_run: bool,

    /// install-data.json to use (default: $VX_INSTALL_DATA, else built from the repo)
    #[arg(long, env = "VX_INSTALL_DATA")]
    pub data: Option<PathBuf>,

    /// Don't offer to fetch the latest repo from GitHub; install the copy on the ISO
    #[arg(long)]
    pub offline: bool,

    /// With --demo: the GitHub login to pretend to be (the owner "Vainnor", or anyone else to see the fork path)
    #[arg(long, hide = true, value_name = "LOGIN")]
    pub demo_user: Option<String>,
}

/// `repo`: the global --repo, used to build install-data when neither
/// --data nor $VX_INSTALL_DATA is given (e.g. `vx install` from a clone).
pub fn run(args: InstallArgs, repo: Option<PathBuf>) -> Result<()> {
    ui::run(args, repo)
}
