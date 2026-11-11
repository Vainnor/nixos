//! One door to everything that reads the machine or does work. The engine
//! always does the work (`--demo` and `--dry-run` use its DryRun mode); the
//! demo only fakes what a Mac can't provide: disks, the machine, install data.

use super::super::engine::github::{Account, DeviceCode, Fork};
use super::super::engine::netbird;
use super::super::engine::restic::OpLogin;
use super::super::engine::wifi::Network;
use super::super::engine::{self, Mode};
use super::super::plan::{DiskInfo, Event, InstallData, Machine, Plan, Previous, ResticCreds, Secret, Unlocked};
use super::super::probe::{self, BackupTarget};
use super::demo;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
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

    /// The work repo from `origin` (clone) or the usb, and the commands run.
    pub fn prepare_repo(self, data: &InstallData, update: bool, origin: &str) -> (Result<PathBuf>, Vec<String>) {
        if self.is_demo() {
            demo::pause(if update { 1400 } else { 500 });
        }
        engine::prepare_repo(data, update, origin, self.mode())
    }

    // GitHub: the demo and --dry-run pretend (engine DryRun fakes the answers).

    pub fn gh_device_code(self) -> (Result<DeviceCode>, Vec<String>) {
        engine::gh_device_code(self.mode())
    }

    pub fn gh_login(self, dc: &DeviceCode, cancel: &AtomicBool) -> (Result<()>, Vec<String>) {
        engine::gh_login(dc, cancel, self.mode())
    }

    pub fn gh_account(self, owner: &str, repo: &str) -> (Result<Account>, Vec<String>) {
        engine::gh_account(owner, repo, self.mode())
    }

    pub fn gh_fork(self, owner: &str, repo: &str, login: &str) -> (Result<Fork>, Vec<String>) {
        engine::gh_fork(owner, repo, login, self.mode())
    }

    // NetBird: the demo and --dry-run pretend (a fake link and code).

    pub fn nb_start(self, host: &str) -> (Result<netbird::Start>, Vec<String>) {
        engine::nb_start(host, self.mode())
    }

    pub fn nb_wait(self, cancel: &AtomicBool) -> (Result<netbird::Status>, Vec<String>) {
        engine::nb_wait(cancel, self.mode())
    }

    pub fn nb_stop(self) {
        engine::nb_stop();
    }

    // Wi-Fi: the demo and --dry-run pretend (a few fake networks, connects at once).

    pub fn wifi_scan(self) -> (Result<Vec<Network>>, Vec<String>) {
        if self.is_demo() {
            demo::pause(900);
        }
        engine::wifi_scan(self.mode())
    }

    pub fn wifi_connect(self, n: &Network, password: Option<&Secret>) -> (Result<bool>, Vec<String>) {
        if self.is_demo() {
            demo::pause(1500);
        }
        engine::wifi_connect(n, password, self.mode())
    }

    // The home backup: the demo and --dry-run pretend (a made-up login and snapshot).

    pub fn restic_sops(self, repo: &Path, key: &Path) -> (Result<ResticCreds>, Vec<String>) {
        if self.is_demo() {
            demo::pause(800);
        }
        engine::restic_from_sops(repo, key, self.mode())
    }

    pub fn restic_op(self, login: &OpLogin, item: &str) -> (Result<ResticCreds>, Vec<String>) {
        if self.is_demo() {
            demo::pause(1500);
        }
        engine::restic_from_op(login, item, self.mode())
    }

    pub fn restic_check(self, creds: &ResticCreds, repo: &Path) -> (Result<String>, Vec<String>) {
        if self.is_demo() {
            demo::pause(1200);
        }
        engine::restic_check(creds, repo, self.mode())
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
