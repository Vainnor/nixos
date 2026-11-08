//! Installer state: what each screen has collected, background tasks, the
//! keys, and turning it all into a [`Plan`].

use super::super::engine::github::{Account, DeviceCode, Fork};
use super::super::engine::netbird::{self as nbe, Login as NbLoginInfo, Status as NbStatus};
use super::super::engine::settings::host_name;
use super::super::engine::wifi::Network;
use super::super::plan::{
    Carry, DataDisk, DataMode, DiskInfo, DiskKind, Event, GitHubMode, GitHubPlan, Gpu, HomePlan, HostInfo, HostPlan, InstallData,
    Machine, NewHost, Outcome, Plan, Previous, Secret, Settings, StepState, SystemDisk, Unlocked, WORK, WORK_REPO, with_requires,
};
use super::super::probe::{self, BackupTarget};
use super::backend::Backend;
use super::form::{Input, Row};
use super::screens;
use super::theme::Theme;
use super::validate;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub const LOG_FILE: &str = "/tmp/vx-install/install.log";
pub const SWAP_CHOICES: [u32; 5] = [0, 8, 16, 32, 64];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Screen {
    Welcome,
    GitHub,
    Machine,
    Previous,
    NetBird,
    System,
    Data,
    Options,
    Passwords,
    Review,
    Install,
    Summary,
    Failed,
    /// Off the welcome screen: connect to Wi-Fi (not a step of its own).
    Wifi,
}

pub const STEPS: [&str; 11] =
    ["welcome", "github", "machine", "previous", "netbird", "system", "data", "options", "passwords", "review", "install"];

impl Screen {
    pub fn step(self) -> usize {
        if self == Screen::Wifi {
            return 0;
        }
        (self as usize).min(10)
    }
    pub fn title(self) -> &'static str {
        match self {
            Screen::Welcome => "welcome",
            Screen::GitHub => "github",
            Screen::Machine => "machine",
            Screen::Previous => "previous install",
            Screen::NetBird => "netbird",
            Screen::System => "system drive",
            Screen::Data => "data drives",
            Screen::Options => "options",
            Screen::Passwords => "passwords",
            Screen::Review => "review",
            Screen::Install => "installing",
            Screen::Summary => "done",
            Screen::Failed => "install failed",
            Screen::Wifi => "wi-fi",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeKind {
    Fresh,
    Keep,
    Backup,
}

/// What a data drive is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    NotUsed,
    Format,
    Mount,
}

/// Everything the cursor can land on. `usize` in disk fields: index into `App::disks`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    FetchLatest,
    Start,
    /// Welcome: open the Wi-Fi screen.
    WifiOpen,
    /// A network from the scan (index into App::wifi.networks).
    WifiNet(usize),
    WifiPw,
    WifiConnect,
    WifiRescan,
    WifiBack,
    GhLogin,
    GhCancel,
    GhSkip,
    GhPush,
    GhFork,
    GhQuit,
    NbLogin,
    NbCancel,
    NbSkip,
    /// An existing host (index into App::hosts()).
    Host(usize),
    NewHost,
    HostName,
    /// A feature of a new host (index into the install data's aspects).
    Aspect(usize),
    Gpu,
    FullName,
    Email,
    UnlockPassword,
    Unlock,
    SkipPrevious,
    Carry(Carry),
    CarryNone,
    Home(HomeKind),
    Backup(usize),
    Rescan,
    Disk(usize),
    DataUse(usize),
    DataName(usize),
    DataMount(usize),
    DataEncrypt(usize),
    DataReadOnly(usize),
    Encrypt,
    Tpm,
    Swap,
    Username,
    TimeZone,
    Theme,
    Wallpaper(usize),
    DiskPw1,
    DiskPw2,
    SameAsDisk,
    LoginPw1,
    LoginPw2,
    ConfirmYes,
    Install,
    Next,
    Reboot,
    /// Reboot into the firmware setup (`systemctl reboot --firmware-setup`).
    RebootFirmware,
    Shell,
}

impl Field {
    pub fn is_input(self) -> bool {
        matches!(
            self,
            Field::UnlockPassword
                | Field::WifiPw
                | Field::DataName(_)
                | Field::DataMount(_)
                | Field::HostName
                | Field::FullName
                | Field::Email
                | Field::Username
                | Field::TimeZone
                | Field::DiskPw1
                | Field::DiskPw2
                | Field::LoginPw1
                | Field::LoginPw2
                | Field::ConfirmYes
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// enter
    Select,
    /// space
    Toggle,
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub struct DataRow {
    pub use_: Use,
    pub name: Input,
    pub name_edited: bool,
    pub mount: Input,
    pub mount_edited: bool,
    /// None: same as the system disk.
    pub encrypt: Option<bool>,
    pub read_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinPurpose {
    System,
    Format,
}

pub enum Modal {
    /// Type `windows` to allow erasing a Windows disk.
    Windows { disk: usize, purpose: WinPurpose, input: Input },
    Quit,
}

pub enum TaskOut {
    Probed(Box<Machine>, Result<Vec<DiskInfo>, String>),
    /// The usb's install data (before the github step).
    Loaded(Result<Box<InstallData>, String>),
    /// The work repo is ready (data rebuilt from it, a warning), with the commands it ran.
    Prepared(Result<Prepared, String>, Vec<String>),
    Unlocked(Result<Box<Unlocked>, String>),
    Backup(Vec<BackupTarget>),
    /// GitHub work, with the commands it ran (for the install log).
    Gh(GhOut, Vec<String>),
    /// NetBird login, with the commands it ran.
    Nb(NbOut, Vec<String>),
    /// Wi-Fi scan / connect, with the commands it ran.
    Wifi(WifiOut, Vec<String>),
}

pub enum WifiOut {
    Scanned(Result<Vec<Network>, String>),
    /// The network's name, and Ok(online).
    Connected(String, Result<bool, String>),
}

/// The Wi-Fi screen.
pub struct Wifi {
    /// None: not scanned yet.
    pub networks: Option<Vec<Network>>,
    pub sel: Option<usize>,
    pub pw: Input,
    pub error: Option<String>,
    /// Connected here, to this network.
    pub connected: Option<String>,
}

impl Wifi {
    pub fn selected(&self) -> Option<&Network> {
        self.networks.as_ref()?.get(self.sel?)
    }
}

pub enum NbOut {
    Started(Result<nbe::Start, String>),
    Done(Result<NbStatus, String>),
}

/// Where the netbird step is.
#[derive(Clone, Debug)]
pub enum NbPhase {
    Start,
    /// Showing the link, waiting for the login.
    Code(NbLoginInfo),
    /// Logged in.
    Done(NbStatus),
}

pub struct Nb {
    pub phase: NbPhase,
    pub error: Option<String>,
    pub skipped: bool,
    /// The host name it logged in as.
    pub host: String,
    pub cancel: Option<Arc<AtomicBool>>,
    pub rx: Option<Receiver<TaskOut>>,
}

impl Nb {
    pub fn done(&self) -> bool {
        matches!(self.phase, NbPhase::Done(_))
    }
}

pub enum GhOut {
    Code(Result<DeviceCode, String>),
    LoggedIn(Result<(), String>),
    Account(Result<Box<Account>, String>),
    Forked(Result<Fork, String>),
}

/// Where the github step is.
#[derive(Clone, Debug)]
pub enum GhPhase {
    /// Not logged in yet.
    Start,
    /// Showing the code, waiting for it to be entered on github.com.
    Code(DeviceCode),
    /// Logged in.
    Account(Box<Account>),
}

pub struct Gh {
    pub phase: GhPhase,
    pub error: Option<String>,
    /// GitHub couldn't be reached: skipping (offline) is allowed.
    pub unreachable: bool,
    /// Skipped: an offline install.
    pub skipped: bool,
    /// Owner: push the install commit at the end.
    pub push: bool,
    /// Stops the background wait for the code.
    pub cancel: Option<Arc<AtomicBool>>,
    /// The background wait (the screen stays usable meanwhile).
    pub rx: Option<Receiver<TaskOut>>,
}

impl Gh {
    pub fn account(&self) -> Option<&Account> {
        match &self.phase {
            GhPhase::Account(a) => Some(a),
            _ => None,
        }
    }

    /// Logged in as the owner, or with a fork to use.
    pub fn ready(&self) -> bool {
        self.account().is_some_and(|a| a.owner || a.fork.is_some())
    }
}

pub struct Task {
    pub label: String,
    pub rx: Receiver<TaskOut>,
}

pub struct Progress {
    pub steps: Vec<(String, Option<StepState>)>,
    pub log: Vec<String>,
    /// None: follow the end. Some(n): first visible line.
    pub scroll: Option<usize>,
    pub rx: Option<Receiver<Event>>,
    pub done_rx: Option<Receiver<Result<(), String>>>,
    pub outcome: Option<Outcome>,
    pub error: Option<String>,
    pub log_file: Option<std::fs::File>,
}

impl Progress {
    pub fn new(steps: Vec<String>) -> Progress {
        Progress {
            steps: steps.into_iter().map(|s| (s, None)).collect(),
            log: vec![],
            scroll: None,
            rx: None,
            done_rx: None,
            outcome: None,
            error: None,
            log_file: None,
        }
    }
    pub fn finished(&self) -> bool {
        self.outcome.is_some() || self.error.is_some()
    }
    fn push_log(&mut self, l: String) {
        if let Some(f) = &mut self.log_file {
            let _ = writeln!(f, "{l}");
        }
        self.log.push(l);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Shell,
    Reboot,
    /// Reboot into the BIOS / UEFI setup.
    Firmware,
}

pub struct App {
    pub backend: Backend,
    pub offline: bool,
    pub data_path: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub ansi: bool,
    pub theme: Theme,
    pub screen: Screen,
    pub focus: Option<Field>,
    pub status: Option<Status>,
    pub modal: Option<Modal>,
    pub task: Option<Task>,
    pub tick: usize,
    pub exit: Option<Exit>,
    /// Write the install log to LOG_FILE (off in tests).
    pub write_log: bool,

    pub machine: Option<Machine>,
    pub disks: Vec<DiskInfo>,
    pub previous: Option<Previous>,
    pub data: Option<InstallData>,
    pub work_repo: Option<PathBuf>,
    pub fetch_latest: bool,
    /// The fetch setting and origin the repo was prepared with.
    pub prepared_with: Option<(bool, String)>,
    pub probe_error: Option<String>,
    /// Commands run before the install (github, the repo): go first in the install log.
    pub pre_log: Vec<String>,

    pub gh: Gh,
    pub nb: Nb,
    pub wifi: Wifi,
    pub full_name: Input,
    pub email: Input,

    /// The machine: an existing host's name, or None for a new machine.
    pub host_choice: Option<String>,
    pub host_defaults_set: bool,
    pub new_name: Input,
    pub new_aspects: BTreeSet<String>,
    pub new_gpu: Gpu,

    pub unlock_pw: Input,
    pub unlock_error: Option<String>,
    pub unlocked: Option<Unlocked>,
    pub unlock_secret: Option<Secret>,
    /// Skipped the previous install: nothing is carried over.
    pub skipped_previous: bool,
    pub carry: BTreeSet<Carry>,
    pub home: HomeKind,
    pub backup_targets: Option<Vec<BackupTarget>>,
    pub backup_sel: Option<String>,

    pub system: Option<usize>,
    pub confirmed_windows: BTreeSet<String>,
    pub data_rows: Vec<DataRow>,

    pub encrypt: bool,
    pub tpm: bool,
    pub swap_gb: u32,
    pub username: Input,
    pub time_zone: Input,
    pub theme_idx: usize,
    pub wallpaper_idx: usize,
    pub defaults_set: bool,

    pub disk_pw1: Input,
    pub disk_pw2: Input,
    pub login_pw1: Input,
    pub login_pw2: Input,
    pub same_as_disk: bool,

    pub confirm: Input,
    pub progress: Option<Progress>,
}

/// A panic in a worker thread (the engine): shown as the error instead of tearing the screen down.
pub static WORKER_PANIC: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

impl App {
    pub fn new(backend: Backend, offline: bool, data_path: Option<PathBuf>, repo: Option<PathBuf>, ansi: bool) -> App {
        App {
            backend,
            offline,
            data_path,
            repo,
            ansi,
            theme: if ansi { Theme::ansi() } else { Theme::slate() },
            screen: Screen::Welcome,
            focus: None,
            status: None,
            modal: None,
            task: None,
            tick: 0,
            exit: None,
            write_log: true,
            machine: None,
            disks: vec![],
            previous: None,
            data: None,
            work_repo: None,
            fetch_latest: !offline,
            prepared_with: None,
            probe_error: None,
            pre_log: vec![],
            gh: Gh { phase: GhPhase::Start, error: None, unreachable: false, skipped: false, push: true, cancel: None, rx: None },
            wifi: Wifi { networks: None, sel: None, pw: Input::secret(), error: None, connected: None },
            nb: Nb { phase: NbPhase::Start, error: None, skipped: false, host: String::new(), cancel: None, rx: None },
            full_name: Input::default(),
            email: Input::default(),
            host_choice: None,
            host_defaults_set: false,
            new_name: Input::default(),
            new_aspects: BTreeSet::new(),
            new_gpu: Gpu::None,
            unlock_pw: Input::secret(),
            unlock_error: None,
            unlocked: None,
            unlock_secret: None,
            skipped_previous: false,
            carry: BTreeSet::new(),
            home: HomeKind::Fresh,
            backup_targets: None,
            backup_sel: None,
            system: None,
            confirmed_windows: BTreeSet::new(),
            data_rows: vec![],
            encrypt: true,
            tpm: false,
            swap_gb: 16,
            username: Input::default(),
            time_zone: Input::default(),
            theme_idx: 0,
            wallpaper_idx: 0,
            defaults_set: false,
            disk_pw1: Input::secret(),
            disk_pw2: Input::secret(),
            login_pw1: Input::secret(),
            login_pw2: Input::secret(),
            same_as_disk: false,
            confirm: Input::new(""),
            progress: None,
        }
    }

    /// Probe the machine (on a thread: the online check takes a moment).
    pub fn start(&mut self) {
        let b = self.backend;
        self.spawn("checking this machine", move || {
            let m = b.machine();
            TaskOut::Probed(Box::new(m), b.disks().map_err(|e| format!("{e:#}")))
        });
    }

    pub fn spawn(&mut self, label: &str, f: impl FnOnce() -> TaskOut + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("task".into()).spawn(move || {
            let _ = tx.send(f());
        });
        self.task = Some(Task { label: label.into(), rx });
    }

    pub fn say(&mut self, level: Level, text: impl Into<String>) {
        self.status = Some(Status { level, text: text.into() });
    }

    // ------------------------------------------------------------ background work

    /// Called every frame: collect finished tasks and install progress.
    pub fn poll(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if let Some(t) = &self.task {
            match t.rx.try_recv() {
                Ok(out) => {
                    self.task = None;
                    self.apply(out);
                }
                Err(TryRecvError::Disconnected) => {
                    self.task = None;
                    let msg = WORKER_PANIC.lock().ok().and_then(|mut p| p.take()).unwrap_or_else(|| "unknown error".into());
                    self.say(Level::Error, format!("that failed: {msg}"));
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.nb.rx {
            match rx.try_recv() {
                Ok(out) => {
                    self.nb.rx = None;
                    self.apply(out);
                }
                Err(TryRecvError::Disconnected) => self.nb.rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.gh.rx {
            match rx.try_recv() {
                Ok(out) => {
                    self.gh.rx = None;
                    self.apply(out);
                }
                Err(TryRecvError::Disconnected) => self.gh.rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        self.poll_progress();
    }

    pub fn apply(&mut self, out: TaskOut) {
        match out {
            TaskOut::Probed(m, disks) => {
                self.machine = Some(*m);
                match disks {
                    Ok(d) => self.set_disks(d),
                    Err(e) => self.probe_error = Some(e),
                }
            }
            TaskOut::Loaded(Ok(data)) => {
                self.set_data(*data);
                self.go(Screen::GitHub);
            }
            TaskOut::Loaded(Err(e)) => self.say(Level::Error, e),
            TaskOut::Prepared(res, logs) => {
                self.pre_log.extend(logs);
                match res {
                    Ok((data, work, warning)) => {
                        let origin = self.origin();
                        self.set_data(*data);
                        self.set_host_defaults();
                        self.work_repo = Some(work);
                        self.prepared_with = Some((self.fetch_latest, origin));
                        self.go(Screen::Machine);
                        if let Some(w) = warning {
                            self.say(Level::Warn, w);
                        }
                    }
                    Err(e) => self.say(Level::Error, e),
                }
            }
            TaskOut::Gh(out, logs) => {
                self.pre_log.extend(logs);
                self.apply_gh(out);
            }
            TaskOut::Nb(out, logs) => {
                self.pre_log.extend(logs);
                self.apply_nb(out);
            }
            TaskOut::Wifi(out, logs) => {
                self.pre_log.extend(logs);
                self.apply_wifi(out);
            }
            TaskOut::Unlocked(Ok(u)) => {
                self.carry = u.available.clone();
                self.unlock_secret = u.previous.encrypted.then(|| Secret(self.unlock_pw.value.clone()));
                self.unlock_pw.clear();
                self.unlock_error = None;
                // Reinstalling onto the same disk and keeping /home is the default
                // when that's where we're installing anyway.
                self.home = if self.system_disk().map(|d| &d.path) == Some(&u.previous.disk) { HomeKind::Keep } else { HomeKind::Fresh };
                self.unlocked = Some(*u);
                self.focus = None;
                self.say(Level::Ok, "unlocked");
            }
            TaskOut::Unlocked(Err(e)) => {
                self.unlock_error = Some(e);
                self.unlock_pw.clear();
                self.focus = Some(Field::UnlockPassword);
            }
            TaskOut::Backup(t) => {
                if self.backup_sel.as_ref().is_some_and(|s| !t.iter().any(|x| &x.partition == s)) {
                    self.backup_sel = None;
                }
                if self.backup_sel.is_none() {
                    let need = self.unlocked.as_ref().map(|u| u.home_bytes).unwrap_or(0);
                    self.backup_sel = t.iter().find(|x| x.free.is_none_or(|f| f >= need)).map(|x| x.partition.clone());
                }
                self.backup_targets = Some(t);
            }
        }
    }

    pub fn set_disks(&mut self, disks: Vec<DiskInfo>) {
        self.previous = probe::previous(&disks).into_iter().next();
        self.data_rows = disks
            .iter()
            .map(|d| DataRow {
                use_: Use::NotUsed,
                name: Input::default(),
                name_edited: false,
                mount: Input::default(),
                mount_edited: false,
                encrypt: None,
                read_only: matches!(d.kind, DiskKind::Windows { system: true, .. }),
            })
            .collect();
        self.system = self
            .previous
            .as_ref()
            .and_then(|p| disks.iter().position(|d| d.path == p.disk))
            .or_else(|| disks.iter().position(|d| d.kind == DiskKind::Empty));
        self.disks = disks;
        if let Some(m) = &self.machine {
            self.tpm = m.tpm;
            if m.ram_gb > 0 && m.ram_gb <= 8 {
                self.swap_gb = 8;
            }
            if !m.online {
                self.fetch_latest = false;
            }
        }
    }

    fn apply_gh(&mut self, out: GhOut) {
        match out {
            GhOut::Code(Ok(dc)) => {
                self.gh.error = None;
                self.gh.unreachable = false;
                self.gh.phase = GhPhase::Code(dc.clone());
                self.wait_for_login(dc);
            }
            GhOut::Code(Err(e)) => {
                // No code at all: github can't be reached from here.
                self.gh.unreachable = true;
                self.gh.error = Some(e);
            }
            GhOut::LoggedIn(Ok(())) => {
                self.gh.cancel = None;
                let (b, (owner, repo)) = (self.backend, self.upstream());
                self.spawn("reading your github account", move || {
                    let (res, logs) = b.gh_account(&owner, &repo);
                    TaskOut::Gh(GhOut::Account(res.map(Box::new).map_err(|e| format!("{e:#}"))), logs)
                });
            }
            GhOut::LoggedIn(Err(e)) => {
                self.gh.cancel = None;
                self.gh.phase = GhPhase::Start;
                if e != "cancelled" {
                    self.gh.error = Some(e);
                }
            }
            GhOut::Account(Ok(a)) => {
                // From this account (they're edited on the options screen, after this one).
                self.gh.error = None;
                self.full_name = Input::new(a.name.as_deref().unwrap_or(&a.login));
                self.email = Input::new(a.email.as_deref().unwrap_or(""));
                self.gh.phase = GhPhase::Account(a);
                self.focus = None;
            }
            GhOut::Account(Err(e)) => {
                self.gh.phase = GhPhase::Start;
                self.gh.error = Some(e);
            }
            GhOut::Forked(Ok(f)) => {
                if let GhPhase::Account(a) = &mut self.gh.phase {
                    a.fork = Some(f);
                }
                self.focus = None;
                self.say(Level::Ok, "forked");
            }
            GhOut::Forked(Err(e)) => self.gh.error = Some(e),
        }
    }

    fn apply_nb(&mut self, out: NbOut) {
        match out {
            NbOut::Started(Ok(nbe::Start::Login(l))) => {
                self.nb.error = None;
                self.nb.phase = NbPhase::Code(l);
                self.wait_for_netbird();
            }
            NbOut::Started(Ok(nbe::Start::Connected(st))) | NbOut::Done(Ok(st)) => {
                self.nb.cancel = None;
                self.nb.error = None;
                self.nb.phase = NbPhase::Done(st);
                self.focus = None;
            }
            NbOut::Started(Err(e)) => self.nb.error = Some(e),
            NbOut::Done(Err(e)) => {
                self.nb.cancel = None;
                self.nb.phase = NbPhase::Start;
                if e != "cancelled" {
                    self.nb.error = Some(e);
                }
            }
        }
    }

    // ------------------------------------------------------------ wi-fi

    /// Welcome's [ connect to wi-fi ]: the wi-fi screen, scanning.
    pub fn wifi_open(&mut self) {
        self.wifi.error = None;
        self.go(Screen::Wifi);
        self.wifi_scan();
    }

    pub fn wifi_scan(&mut self) {
        let b = self.backend;
        self.spawn("looking for wi-fi networks", move || {
            let (r, logs) = b.wifi_scan();
            TaskOut::Wifi(WifiOut::Scanned(r.map_err(|e| format!("{e:#}"))), logs)
        });
    }

    /// Pick a network: the password field next (or connect, for an open one).
    pub fn wifi_pick(&mut self, i: usize) {
        if self.wifi.sel != Some(i) {
            self.wifi.pw.clear();
        }
        self.wifi.sel = Some(i);
        self.wifi.error = None;
        match self.wifi.selected() {
            Some(n) if n.security.needs_password() => self.focus = Some(Field::WifiPw),
            Some(_) => self.wifi_connect(),
            None => {}
        }
    }

    pub fn wifi_connect(&mut self) {
        let Some(n) = self.wifi.selected().cloned() else {
            self.wifi.error = Some("pick a network first".into());
            return;
        };
        if n.security.needs_password() && self.wifi.pw.value.is_empty() {
            self.wifi.error = Some("enter the wi-fi password".into());
            self.focus = Some(Field::WifiPw);
            return;
        }
        let pw = n.security.needs_password().then(|| Secret(self.wifi.pw.value.clone()));
        let b = self.backend;
        self.spawn(&format!("connecting to {}", n.ssid), move || {
            let (r, logs) = b.wifi_connect(&n, pw.as_ref());
            TaskOut::Wifi(WifiOut::Connected(n.ssid.clone(), r.map_err(|e| format!("{e:#}"))), logs)
        });
    }

    fn apply_wifi(&mut self, out: WifiOut) {
        match out {
            WifiOut::Scanned(Ok(n)) => {
                // Keep the pick when it's still there.
                let keep = self.wifi.selected().map(|x| x.ssid.clone());
                self.wifi.sel = keep.and_then(|k| n.iter().position(|x| x.ssid == k));
                self.wifi.networks = Some(n);
                self.wifi.error = None;
                self.fix_focus();
            }
            WifiOut::Scanned(Err(e)) => {
                self.wifi.networks.get_or_insert_with(Vec::new);
                self.wifi.error = Some(format!("couldn't scan: {e}"));
            }
            WifiOut::Connected(ssid, Ok(online)) => {
                self.wifi.pw.clear();
                self.wifi.error = None;
                self.wifi.connected = Some(ssid.clone());
                if let Some(m) = &mut self.machine {
                    m.online = online;
                }
                if online && !self.offline {
                    self.fetch_latest = true;
                }
                self.focus = Some(Field::WifiBack);
                if online {
                    self.say(Level::Ok, format!("connected to {ssid}"));
                } else {
                    self.say(Level::Warn, format!("connected to {ssid}, but the internet isn't reachable"));
                }
            }
            WifiOut::Connected(_, Err(e)) => {
                self.wifi.pw.clear();
                self.wifi.error = Some(e);
                self.focus = Some(if self.wifi.selected().is_some_and(|n| n.security.needs_password()) { Field::WifiPw } else { Field::WifiConnect });
            }
        }
    }

    /// [ log in ] on the netbird step: `netbird up` shows a link; the wait runs in the background.
    pub fn nb_login(&mut self) {
        self.nb.error = None;
        self.nb.skipped = false;
        let host = self.host_name();
        self.nb.host = host.clone();
        let b = self.backend;
        self.spawn("starting netbird", move || {
            let (res, logs) = b.nb_start(&host);
            TaskOut::Nb(NbOut::Started(res.map_err(|e| format!("{e:#}"))), logs)
        });
    }

    fn wait_for_netbird(&mut self) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.nb.cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        let b = self.backend;
        let _ = std::thread::Builder::new().name("nb-login".into()).spawn(move || {
            let (res, logs) = b.nb_wait(&cancel);
            let _ = tx.send(TaskOut::Nb(NbOut::Done(res.map_err(|e| format!("{e:#}"))), logs));
        });
        self.nb.rx = Some(rx);
    }

    pub fn nb_cancel(&mut self) {
        if let Some(c) = self.nb.cancel.take() {
            c.store(true, Ordering::SeqCst);
        }
        self.nb.rx = None;
        if !self.nb.done() {
            self.nb.phase = NbPhase::Start;
            self.backend.nb_stop();
        }
    }

    /// The old install's NetBird login is carried over: no login screen.
    pub fn netbird_carried(&self) -> bool {
        self.carry.contains(&Carry::NetBird) && self.unlocked.as_ref().is_some_and(|u| u.available.contains(&Carry::NetBird))
    }

    /// The netbird step is part of this install (online, nothing carried).
    pub fn uses_netbird(&self) -> bool {
        self.uses_github() && !self.netbird_carried()
    }

    /// [ log in ]: get a code; the wait for it runs in the background.
    pub fn gh_login(&mut self) {
        self.gh.error = None;
        let b = self.backend;
        self.spawn("asking github for a code", move || {
            let (res, logs) = b.gh_device_code();
            TaskOut::Gh(GhOut::Code(res.map_err(|e| format!("couldn't reach github: {e:#}"))), logs)
        });
    }

    fn wait_for_login(&mut self, dc: DeviceCode) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.gh.cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        let b = self.backend;
        let _ = std::thread::Builder::new().name("gh-login".into()).spawn(move || {
            let (res, logs) = b.gh_login(&dc, &cancel);
            let _ = tx.send(TaskOut::Gh(GhOut::LoggedIn(res.map_err(|e| format!("{e:#}"))), logs));
        });
        self.gh.rx = Some(rx);
    }

    pub fn gh_cancel(&mut self) {
        if let Some(c) = self.gh.cancel.take() {
            c.store(true, Ordering::SeqCst);
        }
        self.gh.rx = None;
        self.gh.phase = GhPhase::Start;
    }

    pub fn gh_fork(&mut self) {
        let Some(login) = self.gh.account().map(|a| a.login.clone()) else { return };
        let (b, (owner, repo)) = (self.backend, self.upstream());
        self.gh.error = None;
        self.spawn(&format!("forking {owner}/{repo}"), move || {
            let (res, logs) = b.gh_fork(&owner, &repo, &login);
            TaskOut::Gh(GhOut::Forked(res.map_err(|e| format!("{e:#}"))), logs)
        });
    }

    /// Upstream (owner, repo).
    pub fn upstream(&self) -> (String, String) {
        self.data.as_ref().map(|d| d.upstream()).unwrap_or_default()
    }

    /// The github step is part of this install (online, not --offline).
    pub fn uses_github(&self) -> bool {
        !self.offline && self.machine.as_ref().is_some_and(|m| m.online)
    }

    /// Where the work repo comes from: the fork, else upstream.
    pub fn origin(&self) -> String {
        if let Some(f) = self.gh.account().and_then(|a| a.fork.as_ref()).filter(|_| !self.gh.skipped) {
            return f.url.clone();
        }
        self.data.as_ref().map(|d| d.repo_url.clone()).unwrap_or_default()
    }

    /// Hosts in the repo (older data: just the default host).
    pub fn hosts(&self) -> Vec<HostInfo> {
        let Some(d) = &self.data else { return vec![] };
        if d.hosts.is_empty() {
            return vec![HostInfo { name: d.host.clone(), aspects: vec![], gpu: Gpu::None }];
        }
        d.hosts.clone()
    }

    /// Every aspect's name (a new host can't be called one).
    pub fn aspect_names(&self) -> Vec<&str> {
        self.data.as_ref().map(|d| d.aspects.iter().map(|a| a.name.as_str()).collect()).unwrap_or_default()
    }

    /// Reinstall the previous install's host when there is one, else a new machine.
    pub fn set_host_defaults(&mut self) {
        let hosts = self.hosts();
        if self.host_choice.as_ref().is_some_and(|h| !hosts.iter().any(|x| &x.name == h)) {
            self.host_choice = None;
        }
        if self.host_defaults_set {
            return;
        }
        let Some(d) = &self.data else { return };
        self.host_choice = if self.previous.is_some() && hosts.iter().any(|h| h.name == d.host) { Some(d.host.clone()) } else { None };
        self.new_aspects = d.aspects.iter().filter(|a| a.default && a.name != "base").map(|a| a.name.clone()).collect();
        with_requires(&d.aspects, &mut self.new_aspects);
        self.new_gpu = self.machine.as_ref().map(|m| m.gpu).unwrap_or_default();
        self.host_defaults_set = true;
    }

    /// The host name: the existing host's, or the typed one.
    pub fn host_name(&self) -> String {
        self.host_choice.clone().unwrap_or_else(|| self.new_name.value.clone())
    }

    pub fn set_data(&mut self, data: InstallData) {
        self.theme = Theme::pick(Some(&data), self.ansi);
        if !self.defaults_set {
            let d = &data.defaults;
            self.username = Input::new(&d.username);
            self.time_zone = Input::new(&d.time_zone);
            self.theme_idx = self.theme_names_of(&data).iter().position(|t| *t == d.theme).unwrap_or(0);
            self.wallpaper_idx = data.wallpapers.iter().position(|w| w.name == d.wallpaper).unwrap_or(0);
            self.defaults_set = true;
        }
        self.data = Some(data);
    }

    fn theme_names_of(&self, d: &InstallData) -> Vec<String> {
        std::iter::once("match".to_string()).chain(d.themes.iter().cloned()).collect()
    }

    pub fn theme_names(&self) -> Vec<String> {
        self.data.as_ref().map(|d| self.theme_names_of(d)).unwrap_or_else(|| vec!["match".into()])
    }

    /// Welcome's [ start ]: the github step when online, else straight to preparing the repo.
    pub fn start_flow(&mut self) {
        if !self.uses_github() {
            return self.prepare();
        }
        if self.data.is_some() {
            return self.go(Screen::GitHub);
        }
        let (b, path, repo) = (self.backend, self.data_path.clone(), self.repo.clone());
        self.spawn("loading the install data", move || {
            let d = b.load_data(path.as_deref(), if path.is_some() { None } else { repo.as_deref() });
            TaskOut::Loaded(d.map(Box::new).map_err(|e| format!("couldn't load the install data: {e:#}")))
        });
    }

    /// Make the work repo (clone of the fork or upstream, or the usb's copy),
    /// rebuild the data from it, then the machine step.
    pub fn prepare(&mut self) {
        let origin = self.origin();
        if self.work_repo.is_some() && self.prepared_with.as_ref() == Some(&(self.fetch_latest, origin.clone())) {
            self.go(Screen::Machine);
            return;
        }
        let (b, fetch, path, repo) = (self.backend, self.fetch_latest, self.data_path.clone(), self.repo.clone());
        let have = self.data.clone();
        let label = if fetch { format!("fetching {} from github", origin.trim_start_matches("https://github.com/")) } else { "copying the repo from the usb".into() };
        self.spawn(&label, move || {
            let mut logs = vec![];
            let r = prepare_task(b, fetch, path.as_deref(), repo.as_deref(), have, origin, &mut logs);
            // The commands go to the install log with everything else.
            TaskOut::Prepared(r, logs)
        });
    }

    pub fn unlock(&mut self) {
        let Some(prev) = self.previous.clone() else { return };
        if prev.encrypted && self.unlock_pw.value.is_empty() {
            self.unlock_error = Some("type the disk password first".into());
            return;
        }
        let b = self.backend;
        let pw = prev.encrypted.then(|| Secret(self.unlock_pw.value.clone()));
        self.unlock_error = None;
        self.spawn(if prev.encrypted { "unlocking" } else { "opening" }, move || {
            TaskOut::Unlocked(b.unlock(&prev, pw.as_ref()).map(Box::new).map_err(|e| format!("{e:#}")))
        });
    }

    pub fn scan_backup(&mut self) {
        let erased: Vec<String> = self.erased_disks().into_iter().map(|i| self.disks[i].path.clone()).collect();
        let (b, disks) = (self.backend, self.disks.clone());
        self.spawn("looking for space to back up to", move || TaskOut::Backup(b.backup_targets(&disks, &erased)));
    }

    // ------------------------------------------------------------ derived state

    pub fn system_disk(&self) -> Option<&DiskInfo> {
        self.system.and_then(|i| self.disks.get(i))
    }

    /// /home is kept: the system disk is the previous install's disk.
    pub fn keeping(&self) -> bool {
        self.home == HomeKind::Keep
            && self.unlocked.as_ref().is_some_and(|u| self.system_disk().is_some_and(|d| d.path == u.previous.disk))
    }

    pub fn eff_encrypt(&self) -> bool {
        if self.keeping() { self.unlocked.as_ref().is_some_and(|u| u.previous.encrypted) } else { self.encrypt }
    }

    pub fn eff_swap(&self) -> u32 {
        if self.keeping() { self.unlocked.as_ref().map(|u| u.swap_gb).unwrap_or(0) } else { self.swap_gb }
    }

    pub fn user(&self) -> &str {
        &self.username.value
    }

    /// Disks other than the system disk and the installer: candidates for data drives.
    pub fn data_disks(&self) -> Vec<usize> {
        (0..self.disks.len()).filter(|&i| Some(i) != self.system && self.disks[i].kind != DiskKind::Installer).collect()
    }

    /// Indices of disks the plan erases (system disk unless kept, formatted data drives).
    pub fn erased_disks(&self) -> Vec<usize> {
        let mut v = vec![];
        if let Some(s) = self.system
            && !self.keeping()
        {
            v.push(s);
        }
        for i in self.data_disks() {
            if self.data_rows[i].use_ == Use::Format {
                v.push(i);
            }
        }
        // Before a system disk is picked, the previous install's disk is the likely one.
        if self.system.is_none()
            && let Some(p) = &self.previous
            && let Some(i) = self.disks.iter().position(|d| d.path == p.disk)
        {
            v.push(i);
        }
        v
    }

    pub fn is_windows(&self, i: usize) -> bool {
        matches!(self.disks[i].kind, DiskKind::Windows { .. })
    }

    /// The old install's password is carried over for this username.
    pub fn carry_password(&self) -> bool {
        self.carry.contains(&Carry::Password) && self.unlocked.as_ref().is_some_and(|u| u.users.iter().any(|x| x == self.user()))
    }

    pub fn need_disk_pw(&self) -> bool {
        self.eff_encrypt() && !self.keeping()
    }

    /// The disk password the plan uses: typed, or the unlock password when kept.
    pub fn disk_secret(&self) -> Option<Secret> {
        if self.keeping() {
            return self.unlock_secret.clone().filter(|_| self.eff_encrypt());
        }
        self.eff_encrypt().then(|| Secret(self.disk_pw1.value.clone()))
    }

    pub fn same_as_disk_offered(&self) -> bool {
        !self.carry_password() && self.eff_encrypt()
    }

    pub fn default_name(&self, i: usize, use_: Use) -> String {
        let taken: Vec<String> = self
            .data_disks()
            .into_iter()
            .filter(|&j| j != i && self.data_rows[j].use_ != Use::NotUsed)
            .map(|j| self.data_rows[j].name.value.clone())
            .collect();
        let d = &self.disks[i];
        let mut wants: Vec<String> = vec![];
        if use_ == Use::Format {
            wants.extend(["games", "data", "storage"].map(String::from));
        } else {
            if matches!(d.kind, DiskKind::Windows { system: true, .. }) {
                wants.push("c".into());
            }
            if let Some(l) = probe::mountable_partition(d).and_then(|p| p.label.clone()) {
                let s: String = l.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
                let s = s.trim_matches('-').to_string();
                if !s.is_empty() && s.len() <= 16 {
                    wants.push(s);
                }
            }
            wants.extend("defghijk".chars().map(|c| c.to_string()));
        }
        wants
            .into_iter()
            .chain((2..).map(|n| format!("data-{n}")))
            .find(|w| !taken.contains(w) && validate::data_name(w).is_ok())
            .unwrap_or_default()
    }

    pub fn default_mount(&self, i: usize) -> String {
        let r = &self.data_rows[i];
        match r.use_ {
            Use::Format => format!("/home/{}/{}", self.user(), r.name.value),
            _ => format!("/mnt/{}", r.name.value),
        }
    }

    /// Fill in fields that follow others until edited (data drive mount paths).
    pub fn sync(&mut self) {
        for i in 0..self.data_rows.len() {
            if !self.data_rows[i].mount_edited {
                let m = self.default_mount(i);
                self.data_rows[i].mount.value = m;
            }
        }
    }

    pub fn data_encrypt(&self, i: usize) -> bool {
        self.data_rows[i].encrypt.unwrap_or(self.eff_encrypt())
    }

    pub fn set_use(&mut self, i: usize, u: Use) {
        if u == Use::Mount && probe::mountable_partition(&self.disks[i]).is_none() {
            return;
        }
        if u == Use::Format && self.is_windows(i) && !self.confirmed_windows.contains(&self.disks[i].path) {
            self.modal = Some(Modal::Windows { disk: i, purpose: WinPurpose::Format, input: Input::new("") });
            return;
        }
        self.data_rows[i].use_ = u;
        if u != Use::NotUsed && !self.data_rows[i].name_edited {
            let n = self.default_name(i, u);
            self.data_rows[i].name.value = n;
        }
        self.after_disk_change();
    }

    pub fn select_system(&mut self, i: usize) {
        if self.disks[i].kind == DiskKind::Installer {
            self.say(Level::Error, "that's the installer usb: pick another disk");
            return;
        }
        if self.is_windows(i) && !self.confirmed_windows.contains(&self.disks[i].path) {
            self.modal = Some(Modal::Windows { disk: i, purpose: WinPurpose::System, input: Input::new("") });
            return;
        }
        self.system = Some(i);
        self.data_rows[i].use_ = Use::NotUsed;
        self.after_disk_change();
    }

    /// Keep the /home plan consistent with the disks being erased.
    fn after_disk_change(&mut self) {
        if self.home == HomeKind::Keep
            && let Some(u) = &self.unlocked
            && self.system_disk().is_some_and(|d| d.path != u.previous.disk)
        {
            let disk = u.previous.disk.clone();
            self.home = HomeKind::Fresh;
            self.say(Level::Warn, format!("keeping /home only works when installing onto {disk}: /home will be fresh"));
        }
        if let Some(sel) = self.backup_sel.clone() {
            let erased: Vec<String> = self.erased_disks().into_iter().map(|i| self.disks[i].path.clone()).collect();
            if self.disks.iter().any(|d| erased.contains(&d.path) && d.partitions.iter().any(|p| p.path == sel)) {
                self.backup_sel = None;
                self.backup_targets = None;
                self.say(Level::Warn, format!("{sel} is on a disk that will be erased: pick another backup place"));
            }
        }
    }

    // ------------------------------------------------------------ navigation

    pub fn order(&self) -> Vec<Screen> {
        let mut v = vec![Screen::Welcome];
        if self.uses_github() {
            v.push(Screen::GitHub);
        }
        v.push(Screen::Machine);
        if self.previous.is_some() {
            v.push(Screen::Previous);
        }
        if self.uses_netbird() {
            v.push(Screen::NetBird);
        }
        v.extend([Screen::System, Screen::Data, Screen::Options, Screen::Passwords, Screen::Review]);
        v
    }

    pub fn go(&mut self, s: Screen) {
        self.screen = s;
        self.focus = None;
        self.status = None;
        if s == Screen::Review {
            self.confirm.clear();
        }
        self.fix_focus();
    }

    pub fn back(&mut self) {
        if self.screen == Screen::Wifi {
            return self.go(Screen::Welcome);
        }
        if self.screen == Screen::GitHub {
            self.gh_cancel();
        }
        if self.screen == Screen::NetBird {
            self.nb_cancel();
        }
        let o = self.order();
        if let Some(i) = o.iter().position(|s| *s == self.screen)
            && i > 0
        {
            self.go(o[i - 1]);
        }
    }

    /// Validate this screen and move on.
    pub fn next(&mut self) {
        if let Err(e) = self.check(self.screen) {
            self.say(Level::Error, e);
            return;
        }
        if self.screen == Screen::Review {
            self.start_install();
            return;
        }
        if self.screen == Screen::GitHub {
            return self.prepare();
        }
        let o = self.order();
        if let Some(i) = o.iter().position(|s| *s == self.screen)
            && i + 1 < o.len()
        {
            self.go(o[i + 1]);
            if self.screen == Screen::Previous && self.unlocked.is_none() {
                self.focus = Some(Field::UnlockPassword);
            }
        }
    }

    pub fn check(&self, s: Screen) -> Result<(), String> {
        match s {
            Screen::Welcome => Ok(()),
            Screen::GitHub => self.check_github(),
            Screen::Machine => self.check_machine(),
            Screen::Previous => self.check_previous(),
            Screen::NetBird => {
                if self.nb.done() || self.nb.skipped || !self.uses_netbird() {
                    Ok(())
                } else {
                    Err("log in to netbird, or skip it".into())
                }
            }
            Screen::System => self.check_system(),
            Screen::Data => self.check_data(),
            Screen::Options => self.check_options(),
            Screen::Passwords => self.check_passwords(),
            Screen::Review => {
                for s in self.order() {
                    if s != Screen::Review {
                        self.check(s)?;
                    }
                }
                if self.confirm.value != "YES" {
                    return Err("type YES (in capitals) to install".into());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_github(&self) -> Result<(), String> {
        if !self.uses_github() || self.gh.skipped || self.gh.ready() {
            return Ok(());
        }
        match self.gh.account() {
            Some(a) => {
                let (owner, repo) = self.upstream();
                Err(format!("{} has no fork of {owner}/{repo}: fork it first", a.login))
            }
            None => Err("log in to github first".into()),
        }
    }

    fn check_machine(&self) -> Result<(), String> {
        let hosts = self.hosts();
        match &self.host_choice {
            Some(h) if hosts.iter().any(|x| &x.name == h) => Ok(()),
            Some(h) => Err(format!("{h} isn't in the repo: pick the machine again")),
            None => {
                let n = &self.new_name.value;
                host_name(n, &self.aspect_names())?;
                if hosts.iter().any(|h| h.name.eq_ignore_ascii_case(n)) {
                    return Err(format!("name: {n} is already in the repo: pick it above to reinstall it"));
                }
                if self.data.as_ref().is_some_and(|d| d.state_version.is_empty()) {
                    return Err("the install data has no stateVersion: rebuild the usb from a newer repo".into());
                }
                Ok(())
            }
        }
    }

    fn check_previous(&self) -> Result<(), String> {
        if self.unlocked.is_none() {
            return if self.previous.is_none() || self.skipped_previous { Ok(()) } else { Err("unlock the old install, or skip it".into()) };
        }
        if self.home == HomeKind::Backup {
            let Some(sel) = &self.backup_sel else { return Err("pick where to back /home up to".into()) };
            let need = self.unlocked.as_ref().map(|u| u.home_bytes).unwrap_or(0);
            if let Some(t) = self.backup_targets.iter().flatten().find(|t| &t.partition == sel)
                && t.free.is_some_and(|f| f < need)
            {
                return Err(format!("{sel} is too small for /home: pick another place"));
            }
        }
        Ok(())
    }

    fn check_system(&self) -> Result<(), String> {
        let Some(i) = self.system else { return Err("pick the disk to install onto".into()) };
        let d = &self.disks[i];
        if d.kind == DiskKind::Installer {
            return Err("that's the installer usb: pick another disk".into());
        }
        if self.is_windows(i) && !self.confirmed_windows.contains(&d.path) {
            return Err("confirm erasing windows first (enter on the disk)".into());
        }
        Ok(())
    }

    fn check_data(&self) -> Result<(), String> {
        let mut names = BTreeSet::new();
        let mut mounts = BTreeSet::new();
        for i in self.data_disks() {
            let r = &self.data_rows[i];
            if r.use_ == Use::NotUsed {
                continue;
            }
            let d = &self.disks[i];
            validate::data_name(&r.name.value).map_err(|e| format!("{}: {e}", short(&d.path)))?;
            validate::mount_path(&r.mount.value, self.user()).map_err(|e| format!("{}: {e}", short(&d.path)))?;
            if !names.insert(r.name.value.clone()) {
                return Err(format!("two data drives are called {}: rename one", r.name.value));
            }
            if !mounts.insert(r.mount.value.clone()) {
                return Err(format!("two data drives mount at {}: change one", r.mount.value));
            }
            if r.use_ == Use::Mount && probe::mountable_partition(d).is_none() {
                return Err(format!("{}: nothing on it can be mounted", short(&d.path)));
            }
            if r.use_ == Use::Format && self.is_windows(i) && !self.confirmed_windows.contains(&d.path) {
                return Err(format!("{}: confirm erasing windows first", short(&d.path)));
            }
        }
        Ok(())
    }

    fn check_options(&self) -> Result<(), String> {
        validate::hostname(&self.host_name())?;
        validate::username(&self.username.value)?;
        if self.forking() {
            validate::full_name(&self.full_name.value)?;
            validate::email(&self.email.value)?;
        }
        validate::time_zone(&self.time_zone.value, Some(Path::new("/etc/zoneinfo")))?;
        // Data mount paths under the old username would now be wrong.
        self.check_data()
    }

    fn check_passwords(&self) -> Result<(), String> {
        if self.need_disk_pw() {
            if self.disk_pw1.value.is_empty() {
                return Err("type a disk password".into());
            }
            if self.disk_pw1.value != self.disk_pw2.value {
                return Err("the disk passwords don't match".into());
            }
        }
        if !self.carry_password() && !(self.same_as_disk && self.same_as_disk_offered()) {
            if self.login_pw1.value.is_empty() {
                return Err("type a login password".into());
            }
            if self.login_pw1.value != self.login_pw2.value {
                return Err("the login passwords don't match".into());
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ the plan

    /// Installing from the user's fork (meta.nix gets their name and email).
    pub fn forking(&self) -> bool {
        !self.gh.skipped && self.gh.account().is_some_and(|a| !a.owner && a.fork.is_some())
    }

    pub fn github_plan(&self) -> GitHubPlan {
        let (owner, repo) = self.upstream();
        let upstream = format!("{owner}/{repo}");
        let Some(a) = self.gh.account().filter(|_| self.uses_github() && !self.gh.skipped) else {
            return GitHubPlan::offline(&upstream);
        };
        let mode = match (&a.fork, a.owner) {
            (_, true) => GitHubMode::Owner,
            (Some(f), false) => GitHubMode::Fork { url: f.url.clone(), branch: f.branch.clone() },
            (None, false) => return GitHubPlan::offline(&upstream),
        };
        let fork = matches!(mode, GitHubMode::Fork { .. });
        let typed = |i: &Input, fallback: &Option<String>| {
            if fork { Some(i.value.trim().to_string()).filter(|s| !s.is_empty()) } else { fallback.clone() }
        };
        GitHubPlan {
            login: Some(a.login.clone()),
            upstream,
            push: if fork { true } else { self.gh.push },
            mode,
            keys: a.keys.clone(),
            full_name: typed(&self.full_name, &a.name),
            email: typed(&self.email, &a.email),
        }
    }

    pub fn host_plan(&self) -> Result<HostPlan, String> {
        self.check_machine()?;
        Ok(match &self.host_choice {
            Some(h) => HostPlan::Existing(h.clone()),
            None => {
                let data = self.data.as_ref().ok_or("the install data isn't loaded")?;
                let mut chosen = self.new_aspects.clone();
                with_requires(&data.aspects, &mut chosen);
                let mut aspects = vec!["base".to_string()];
                // In the data's order.
                aspects.extend(data.aspects.iter().filter(|a| chosen.contains(&a.name)).map(|a| a.name.clone()));
                HostPlan::New(NewHost { name: self.new_name.value.clone(), aspects, gpu: self.new_gpu, state_version: data.state_version.clone() })
            }
        })
    }

    pub fn build_plan(&self) -> Result<Plan, String> {
        for s in [Screen::GitHub, Screen::Machine, Screen::Previous, Screen::NetBird, Screen::System, Screen::Data, Screen::Options, Screen::Passwords] {
            self.check(s)?;
        }
        let data = self.data.as_ref().ok_or("the install data isn't loaded")?;
        let sys = self.system_disk().ok_or("pick a system disk")?;
        let id = |d: &DiskInfo| d.by_id.clone().unwrap_or_else(|| d.path.clone());
        let keep = self.keeping();
        let encrypt = self.eff_encrypt();
        let mut disks = vec![];
        for i in self.data_disks() {
            let (r, d) = (&self.data_rows[i], &self.disks[i]);
            let mode = match r.use_ {
                Use::NotUsed => continue,
                Use::Format => {
                    if self.is_windows(i) && !self.confirmed_windows.contains(&d.path) {
                        return Err(format!("{}: windows not confirmed", d.path));
                    }
                    DataMode::Format { device: id(d), encrypt: self.data_encrypt(i) }
                }
                Use::Mount => {
                    let p = probe::mountable_partition(d).ok_or(format!("{}: nothing to mount", d.path))?;
                    DataMode::Mount {
                        uuid: p.uuid.clone().unwrap_or_default(),
                        fs_type: probe::mount_fs_type(p.fstype.as_deref().unwrap_or("")),
                        read_only: r.read_only,
                    }
                }
            };
            disks.push(DataDisk { name: r.name.value.clone(), mount: r.mount.value.clone(), mode });
        }
        if self.is_windows(self.system.unwrap_or(0)) && !self.confirmed_windows.contains(&sys.path) {
            return Err(format!("{}: windows not confirmed", sys.path));
        }
        let home = match self.home {
            _ if self.unlocked.is_none() => HomePlan::Fresh,
            HomeKind::Fresh => HomePlan::Fresh,
            HomeKind::Keep if keep => HomePlan::Keep,
            HomeKind::Keep => HomePlan::Fresh,
            HomeKind::Backup => {
                let p = self.backup_sel.clone().ok_or("pick where to back /home up to")?;
                let erased: Vec<&str> = self.erased_disks().into_iter().map(|i| self.disks[i].path.as_str()).collect();
                if self.disks.iter().any(|d| erased.contains(&d.path.as_str()) && d.partitions.iter().any(|x| x.path == p)) {
                    return Err(format!("{p} is on a disk that will be erased: pick another backup place"));
                }
                HomePlan::Backup { partition: p }
            }
        };
        let disk_password = self.disk_secret();
        let user_password = if self.carry_password() {
            None
        } else if self.same_as_disk && self.same_as_disk_offered() {
            disk_password.clone()
        } else {
            Some(Secret(self.login_pw1.value.clone()))
        };
        let themes = self.theme_names();
        let host = self.host_plan()?;
        Ok(Plan {
            flake: PathBuf::from(WORK_REPO),
            settings: Settings {
                hostname: host.name().to_string(),
                username: self.username.value.clone(),
                time_zone: self.time_zone.value.clone(),
                wallpaper: data.wallpapers.get(self.wallpaper_idx).map(|w| w.name.clone()).unwrap_or_else(|| data.defaults.wallpaper.clone()),
                theme: themes.get(self.theme_idx).cloned().unwrap_or_else(|| "match".into()),
            },
            host,
            github: self.github_plan(),
            system: SystemDisk { device: id(sys), encrypt, tpm: encrypt && self.tpm, swap_gb: self.eff_swap() },
            data: disks,
            disk_password,
            user_password,
            previous: self.unlocked.clone(),
            carry: if self.unlocked.is_some() { self.carry.clone() } else { BTreeSet::new() },
            home,
            setup_mode: self.machine.as_ref().is_some_and(|m| m.setup_mode),
            netbird: self.uses_netbird() && self.nb.done(),
        })
    }

    pub fn start_install(&mut self) {
        let plan = match self.build_plan() {
            Ok(p) => p,
            Err(e) => return self.say(Level::Error, e),
        };
        let mut p = Progress::new(self.backend.steps(&plan));
        if self.write_log {
            let _ = std::fs::create_dir_all(WORK);
            p.log_file = std::fs::File::create(LOG_FILE).ok();
        }
        for l in self.pre_log.clone() {
            p.push_log(l);
        }
        let (tx, rx) = mpsc::channel();
        let (dtx, drx) = mpsc::channel();
        let b = self.backend;
        let _ = std::thread::Builder::new().name("engine".into()).spawn(move || {
            let r = b.run(&plan, tx).map_err(|e| format!("{e:#}"));
            let _ = dtx.send(r);
        });
        p.rx = Some(rx);
        p.done_rx = Some(drx);
        self.progress = Some(p);
        self.screen = Screen::Install;
        self.focus = None;
        self.status = None;
    }

    fn poll_progress(&mut self) {
        let Some(p) = &mut self.progress else { return };
        let mut events = vec![];
        if let Some(rx) = &p.rx {
            while let Ok(e) = rx.try_recv() {
                events.push(e);
            }
        }
        for e in events {
            match e {
                Event::Step { index, state } => {
                    if let Some(s) = p.steps.get_mut(index) {
                        s.1 = Some(state);
                    }
                }
                Event::Log(l) => p.push_log(l),
                Event::Done(o) => p.outcome = Some(o),
            }
        }
        if let Some(drx) = &p.done_rx {
            match drx.try_recv() {
                Ok(Ok(())) => {
                    p.done_rx = None;
                    if p.outcome.is_none() {
                        p.outcome = Some(Outcome::default());
                    }
                }
                Ok(Err(e)) => {
                    p.done_rx = None;
                    p.push_log(format!("error: {e}"));
                    p.error = Some(e);
                }
                Err(TryRecvError::Disconnected) => {
                    p.done_rx = None;
                    let msg = WORKER_PANIC.lock().ok().and_then(|mut x| x.take()).unwrap_or_else(|| "the install stopped unexpectedly".into());
                    p.push_log(format!("error: {msg}"));
                    p.error = Some(msg);
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.screen == Screen::Install {
            if p.error.is_some() {
                // A step still marked running is where it failed.
                for s in &mut p.steps {
                    if s.1 == Some(StepState::Running) {
                        s.1 = Some(StepState::Failed("failed".into()));
                    }
                }
                self.screen = Screen::Failed;
                self.focus = None;
            } else if p.outcome.is_some() && p.done_rx.is_none() {
                self.screen = Screen::Summary;
                self.focus = None;
            }
        }
        self.fix_focus();
    }

    /// Destructive steps are running: quitting would leave the disks half done.
    pub fn installing(&self) -> bool {
        self.progress.as_ref().is_some_and(|p| !p.finished())
    }

    // ------------------------------------------------------------ keys

    pub fn rows(&self, width: u16) -> Vec<Row> {
        screens::rows(self, width)
    }

    pub fn focusables(&self) -> Vec<Field> {
        self.rows(80).into_iter().filter_map(|r| r.field).collect()
    }

    pub fn fix_focus(&mut self) {
        let f = self.focusables();
        if self.focus.is_none_or(|x| !f.contains(&x)) {
            self.focus = f.first().copied();
        }
    }

    fn move_focus(&mut self, d: i32) {
        let f = self.focusables();
        if f.is_empty() {
            return;
        }
        let i = self.focus.and_then(|x| f.iter().position(|y| *y == x)).unwrap_or(0) as i32;
        let j = (i + d).clamp(0, f.len() as i32 - 1) as usize;
        self.focus = Some(f[j]);
    }

    pub fn request_quit(&mut self) {
        if self.installing() {
            self.say(Level::Warn, "installing: quitting now would leave the disks half done. wait for it to finish");
            return;
        }
        if self.progress.is_some() {
            self.modal = Some(Modal::Quit);
        } else {
            self.exit = Some(Exit::Shell);
        }
    }

    pub fn key(&mut self, k: KeyEvent) {
        if k.kind == KeyEventKind::Release {
            return;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            return self.request_quit();
        }
        if self.modal.is_some() {
            return self.modal_key(k);
        }
        if self.task.is_some() {
            if k.code == KeyCode::Char('q') {
                self.request_quit();
            }
            return;
        }
        if self.screen == Screen::Install {
            return self.progress_key(k);
        }
        let focus = self.focus;
        let input = focus.is_some_and(Field::is_input);
        if input && let Some(f) = focus {
            match k.code {
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.edit(f, |s| s.clear());
                    return;
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.edit(f, |s| s.push(c));
                    return;
                }
                KeyCode::Backspace => {
                    self.edit(f, |s| {
                        s.pop();
                    });
                    return;
                }
                KeyCode::Enter => {
                    // Enter in the last field of a group acts like the button below it.
                    match f {
                        Field::UnlockPassword => self.unlock(),
                        Field::WifiPw => self.wifi_connect(),
                        Field::ConfirmYes => self.next(),
                        _ => self.move_focus(1),
                    }
                    return;
                }
                _ => {}
            }
        }
        match k.code {
            KeyCode::Up | KeyCode::BackTab => self.move_focus(-1),
            KeyCode::Char('k') if !input => self.move_focus(-1),
            KeyCode::Down | KeyCode::Tab => self.move_focus(1),
            KeyCode::Char('j') if !input => self.move_focus(1),
            KeyCode::Esc => {
                if matches!(self.screen, Screen::Summary | Screen::Failed) {
                    return;
                }
                self.back()
            }
            KeyCode::Char('q') if !input => self.request_quit(),
            KeyCode::Char(' ') => self.act(Act::Toggle),
            KeyCode::Enter => self.act(Act::Select),
            KeyCode::Left => self.act(Act::Left),
            KeyCode::Char('h') if !input => self.act(Act::Left),
            KeyCode::Right => self.act(Act::Right),
            KeyCode::Char('l') if !input => self.act(Act::Right),
            _ => {}
        }
        self.sync();
        self.fix_focus();
    }

    fn act(&mut self, a: Act) {
        if let Some(f) = self.focus {
            screens::act(self, f, a);
        }
    }

    fn edit(&mut self, f: Field, op: impl FnOnce(&mut String)) {
        let input = match f {
            Field::UnlockPassword => &mut self.unlock_pw,
            Field::WifiPw => {
                self.wifi.error = None;
                &mut self.wifi.pw
            }
            Field::DataName(i) => {
                self.data_rows[i].name_edited = true;
                &mut self.data_rows[i].name
            }
            Field::DataMount(i) => {
                self.data_rows[i].mount_edited = true;
                &mut self.data_rows[i].mount
            }
            Field::HostName => &mut self.new_name,
            Field::FullName => &mut self.full_name,
            Field::Email => &mut self.email,
            Field::Username => &mut self.username,
            Field::TimeZone => &mut self.time_zone,
            Field::DiskPw1 => &mut self.disk_pw1,
            Field::DiskPw2 => &mut self.disk_pw2,
            Field::LoginPw1 => &mut self.login_pw1,
            Field::LoginPw2 => &mut self.login_pw2,
            Field::ConfirmYes => &mut self.confirm,
            _ => return,
        };
        op(&mut input.value);
        if matches!(f, Field::Username | Field::DataName(_)) {
            self.sync();
        }
        if self.status.as_ref().is_some_and(|s| s.level == Level::Error) {
            self.status = None;
        }
    }

    fn modal_key(&mut self, k: KeyEvent) {
        match self.modal.take() {
            Some(Modal::Quit) => match k.code {
                KeyCode::Char('y') | KeyCode::Char('q') | KeyCode::Enter => self.exit = Some(Exit::Shell),
                _ => {}
            },
            Some(Modal::Windows { disk, purpose, mut input }) => match k.code {
                KeyCode::Esc => self.say(Level::Info, "left windows alone"),
                KeyCode::Enter if input.value == "windows" => {
                    self.confirmed_windows.insert(self.disks[disk].path.clone());
                    match purpose {
                        WinPurpose::System => self.select_system(disk),
                        WinPurpose::Format => self.set_use(disk, Use::Format),
                    }
                    self.sync();
                }
                KeyCode::Enter => {
                    input.clear();
                    self.modal = Some(Modal::Windows { disk, purpose, input });
                }
                KeyCode::Backspace => {
                    input.value.pop();
                    self.modal = Some(Modal::Windows { disk, purpose, input });
                }
                KeyCode::Char(c) => {
                    input.value.push(c);
                    self.modal = Some(Modal::Windows { disk, purpose, input });
                }
                _ => self.modal = Some(Modal::Windows { disk, purpose, input }),
            },
            None => {}
        }
    }

    fn progress_key(&mut self, k: KeyEvent) {
        let Some(p) = &mut self.progress else { return };
        let len = p.log.len();
        let cur = p.scroll.unwrap_or(len.saturating_sub(1));
        let new = match k.code {
            KeyCode::Up | KeyCode::Char('k') => Some(cur.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => Some(cur + 1),
            KeyCode::PageUp => Some(cur.saturating_sub(10)),
            KeyCode::PageDown => Some(cur + 10),
            KeyCode::Home | KeyCode::Char('g') => Some(0),
            KeyCode::End | KeyCode::Char('G') => None,
            KeyCode::Char('q') | KeyCode::Esc => {
                self.request_quit();
                return;
            }
            _ => return,
        };
        // Scrolling to the end follows again.
        p.scroll = new.filter(|&n| n + 1 < len);
    }
}

pub fn short(dev: &str) -> &str {
    dev.strip_prefix("/dev/").unwrap_or(dev)
}

/// The data rebuilt from the work repo, the repo, and a warning to show.
pub type Prepared = (Box<InstallData>, PathBuf, Option<String>);

/// The background half of [`App::prepare`].
fn prepare_task(
    b: Backend,
    fetch: bool,
    path: Option<&Path>,
    repo: Option<&Path>,
    have: Option<InstallData>,
    origin: String,
    logs: &mut Vec<String>,
) -> Result<Prepared, String> {
    let initial = match have {
        Some(d) => d,
        None => b
            .load_data(path, if path.is_some() { None } else { repo })
            .map_err(|e| format!("couldn't load the install data: {e:#}"))?,
    };
    let origin = if origin.is_empty() { initial.repo_url.clone() } else { origin };
    let (work, l) = b.prepare_repo(&initial, fetch, &origin);
    logs.extend(l);
    let work = work.map_err(|e| format!("couldn't prepare the repo: {e:#}"))?;
    if !fetch {
        return Ok((Box::new(initial), work, None));
    }
    match b.load_data(None, Some(&work)) {
        Ok(d) => Ok((Box::new(d), work, None)),
        Err(e) => Ok((Box::new(initial), work, Some(format!("using the usb's defaults ({e:#})")))),
    }
}
