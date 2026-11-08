//! One door to everything that reads the machine or does work. The engine
//! always does the work (`--demo` and `--dry-run` use its DryRun mode); the
//! demo only fakes what a Mac can't provide: disks, the machine, install data.

use super::super::engine::{self, Mode};
use super::super::plan::{DiskInfo, Event, InstallData, Machine, Plan, Previous, Secret, Unlocked};
use super::super::probe::{self, BackupTarget};
use super::demo;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Fake machine, disks and install data; the engine dry-runs. Runs anywhere.
    Demo,
    /// The real probe and engine, really (`Real`) or printing commands (`DryRun`).
    Engine(Mode),
}

impl Backend {
    /// The engine's mode: the demo only ever dry-runs.
    pub fn mode(self) -> Mode {
        match self {
            Backend::Demo => Mode::DryRun,
            Backend::Engine(m) => m,
        }
    }

    pub fn is_demo(self) -> bool {
        self == Backend::Demo
    }

    /// Commands really run (anything else is safe to quit at any time).
    pub fn is_real(self) -> bool {
        self == Backend::Engine(Mode::Real)
    }

    pub fn label(self) -> &'static str {
        match self {
            Backend::Demo => "demo",
            Backend::Engine(Mode::DryRun) => "dry run",
            Backend::Engine(Mode::Real) => "",
        }
    }

    pub fn machine(self) -> Machine {
        match self {
            Backend::Demo => demo::machine(),
            Backend::Engine(_) => probe::machine(),
        }
    }

    pub fn disks(self) -> Result<Vec<DiskInfo>> {
        match self {
            Backend::Demo => Ok(demo::disks()),
            Backend::Engine(_) => probe::disks(),
        }
    }

    pub fn load_data(self, path: Option<&Path>, repo: Option<&Path>) -> Result<InstallData> {
        match self {
            Backend::Demo => Ok(demo::data()),
            Backend::Engine(_) => engine::load_data(path, repo),
        }
    }

    pub fn prepare_repo(self, data: &InstallData, update: bool) -> Result<PathBuf> {
        match self {
            Backend::Demo => demo::prepare_repo(update),
            Backend::Engine(m) => engine::prepare_repo(data, update, m),
        }
    }

    pub fn unlock(self, prev: &Previous, password: Option<&Secret>) -> Result<Unlocked> {
        engine::unlock_previous(prev, password, self.mode())
    }

    pub fn close_previous(self) {
        let _ = engine::close_previous(self.mode());
    }

    pub fn backup_targets(self, disks: &[DiskInfo], erased: &[String]) -> Vec<BackupTarget> {
        match self {
            Backend::Demo => demo::backup_targets(disks, erased),
            Backend::Engine(_) => probe::backup_targets(disks, erased),
        }
    }

    pub fn steps(self, plan: &Plan) -> Vec<String> {
        engine::steps(plan)
    }

    pub fn run(self, plan: &Plan, tx: Sender<Event>) -> Result<()> {
        engine::run(plan, tx, self.mode())
    }
}
