//! Does the install described by a Plan. Stub: filled in by the engine agent.
//!
//! Every external command goes through a runner that, in dry-run/demo mode,
//! only logs "$ cmd args" instead of running it.

use super::plan::{Event, InstallData, Plan, Previous, Secret, Unlocked};
use anyhow::Result;
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
pub fn load_data(_path: Option<&Path>, _repo: Option<&Path>) -> Result<InstallData> {
    todo!("engine::load_data")
}

/// Make WORK_REPO: a fresh `git clone` of data.repo_url when `update` (and
/// online), otherwise a writable copy of data.flake. Returns the path.
pub fn prepare_repo(_data: &InstallData, _update: bool, _mode: Mode) -> Result<PathBuf> {
    todo!("engine::prepare_repo")
}

/// Unlock (when encrypted) and mount a previous install read-only under
/// OLD_MOUNT, and list what can be carried over.
pub fn unlock_previous(_prev: &Previous, _password: Option<&Secret>, _mode: Mode) -> Result<Unlocked> {
    todo!("engine::unlock_previous")
}

/// Unmount and close what unlock_previous opened (safe to call when nothing is open).
pub fn close_previous(_mode: Mode) -> Result<()> {
    todo!("engine::close_previous")
}

/// The step names for this plan, in order (shown on the progress screen).
pub fn steps(_plan: &Plan) -> Vec<String> {
    todo!("engine::steps")
}

/// Run the plan, sending Events; ends with Event::Done on success. Blocking:
/// the UI calls it on its own thread.
pub fn run(_plan: &Plan, _tx: Sender<Event>, _mode: Mode) -> Result<()> {
    todo!("engine::run")
}
