//! Plan building from screen state, the key flow, and render dumps of every
//! screen (tools/vx/target/screens/<screen>-<size>.txt).

use super::super::engine::github::{Account, DeviceCode, Fork};
use super::super::plan::{
    Carry, DataMode, GitHubMode, Gpu, HomePlan, HostPlan, Outcome, Previous, Secret, StepState, Unlocked, OLD_MOUNT, WORK_REPO,
};
use super::super::engine::netbird::{Login as NbLogin, Start as NbStart, Status as NbStatus};
use super::app::{App, Exit, Field, GhOut, GhPhase, HomeKind, Modal, NbOut, NbPhase, Progress, Screen, TaskOut, Use};
use super::backend::Backend;
use super::{demo, view};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn app() -> App {
    let mut a = App::new(Backend::Demo, false, None, None, false);
    a.write_log = false;
    a.apply(TaskOut::Probed(Box::new(demo::machine()), Ok(demo::disks())));
    a.go(Screen::Welcome);
    a
}

fn owner() -> Account {
    Account {
        login: "Vainnor".into(),
        name: Some("Vainnor".into()),
        email: Some("vainnor@example.com".into()),
        keys: vec!["ssh-ed25519 AAAAowner".into()],
        owner: true,
        fork: None,
    }
}

fn someone(fork: bool) -> Account {
    Account {
        login: "someone".into(),
        name: Some("Some One".into()),
        email: None,
        keys: vec!["ssh-ed25519 AAAAsomeone".into()],
        owner: false,
        fork: fork.then(|| Fork {
            full_name: "someone/nixos".into(),
            url: "https://github.com/someone/nixos.git".into(),
            branch: "main".into(),
        }),
    }
}

fn logged_in(a: &mut App, acc: Account) {
    a.apply(TaskOut::Gh(GhOut::Account(Ok(Box::new(acc))), vec!["$ gh api user".into()]));
}

fn prepared_repo() -> TaskOut {
    TaskOut::Prepared(Ok((Box::new(demo::data()), PathBuf::from(WORK_REPO), None)), vec![])
}

/// Past welcome and github (logged in as the owner), repo prepared: on the machine step.
fn prepared() -> App {
    let mut a = app();
    a.apply(TaskOut::Loaded(Ok(Box::new(demo::data()))));
    logged_in(&mut a, owner());
    a.apply(prepared_repo());
    // The netbird step is its own test; plan tests skip it.
    a.nb.skipped = true;
    a
}

fn nb_status() -> NbStatus {
    NbStatus { ip: "100.92.14.7".into(), fqdn: "arc.netbird.cloud".into() }
}

fn code() -> DeviceCode {
    DeviceCode { user_code: "WDJB-MJHT".into(), uri: "https://github.com/login/device".into(), device_code: Secret("x".into()), interval: 5, expires_in: 900 }
}

fn fake_unlocked(prev: &Previous) -> Unlocked {
    Unlocked {
        previous: prev.clone(),
        persistent: PathBuf::from(format!("{OLD_MOUNT}/persistent")),
        home: PathBuf::from(format!("{OLD_MOUNT}/home")),
        available: Carry::ALL.into_iter().collect(),
        home_bytes: 312_400_000_000,
        users: vec!["vainnor".into()],
        swap_gb: 16,
    }
}

fn unlocked() -> App {
    let mut a = prepared();
    a.unlock_pw.value = "old-pass".into();
    let u = fake_unlocked(a.previous.as_ref().unwrap());
    a.apply(TaskOut::Unlocked(Ok(Box::new(u))));
    a
}

fn idx(a: &App, path: &str) -> usize {
    a.disks.iter().position(|d| d.path == path).unwrap()
}

fn key(a: &mut App, c: KeyCode) {
    a.key(KeyEvent::new(c, KeyModifiers::NONE));
}

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        key(a, KeyCode::Char(c));
    }
}

fn passwords(a: &mut App) {
    for i in [&mut a.disk_pw1, &mut a.disk_pw2, &mut a.login_pw1, &mut a.login_pw2] {
        i.value = "s3cr3t-pw".into();
    }
}

// ---------------------------------------------------------------- plan building

#[test]
fn defaults_from_the_probe() {
    let a = prepared();
    assert_eq!(a.screen, Screen::Machine);
    assert_eq!(a.system_disk().unwrap().path, "/dev/nvme0n1", "the previous install's disk");
    assert_eq!(a.host_choice.as_deref(), Some("arc"), "a previous install: reinstall arc");
    assert_eq!(a.host_name(), "arc");
    assert_eq!(a.new_gpu, Gpu::Amd, "lspci's answer");
    assert!(a.new_aspects.contains("desktop") && !a.new_aspects.contains("gaming") && !a.new_aspects.contains("base"));
    assert_eq!(a.data.as_ref().unwrap().wallpapers[a.wallpaper_idx].name, "eye");
    assert!(a.fetch_latest);
    let c = idx(&a, "/dev/nvme1n1");
    assert!(a.data_rows[c].read_only, "windows c: read-only by default");
    assert!(!a.data_rows[idx(&a, "/dev/sda")].read_only);
}

#[test]
fn keep_forces_encryption_and_swap() {
    let mut a = unlocked();
    assert_eq!(a.home, HomeKind::Keep, "same disk: keep is the default");
    a.encrypt = false;
    a.swap_gb = 64;
    passwords(&mut a);
    let p = a.build_plan().unwrap();
    assert_eq!(p.home, HomePlan::Keep);
    assert!(p.system.encrypt, "kept from the old install");
    assert_eq!(p.system.swap_gb, 16, "Unlocked.swap_gb");
    assert_eq!(p.disk_password.unwrap().0, "old-pass", "the unlock password");
    assert_eq!(p.system.device, "/dev/disk/by-id/nvme-CT1000P310SSD8_24304A1B2C3D");
}

#[test]
fn another_system_disk_turns_keep_into_fresh() {
    let mut a = unlocked();
    a.select_system(idx(&a, "/dev/sdc"));
    assert_eq!(a.home, HomeKind::Fresh);
    assert!(a.status.as_ref().unwrap().text.contains("/home will be fresh"));
    a.encrypt = false;
    a.swap_gb = 32;
    passwords(&mut a);
    let p = a.build_plan().unwrap();
    assert_eq!((p.home, p.system.encrypt, p.system.swap_gb), (HomePlan::Fresh, false, 32));
    assert!(p.disk_password.is_none());
}

#[test]
fn carried_password_means_no_user_password() {
    let mut a = unlocked();
    passwords(&mut a);
    assert!(a.build_plan().unwrap().user_password.is_none());
    a.carry.remove(&Carry::Password);
    assert_eq!(a.build_plan().unwrap().user_password.unwrap().0, "s3cr3t-pw");
    // Carried, but the old install has no password for this username.
    a.carry.insert(Carry::Password);
    a.username.value = "someone".into();
    assert!(a.build_plan().unwrap().user_password.is_some());
}

#[test]
fn same_as_disk_password() {
    let mut a = prepared();
    a.skipped_previous = true;
    a.select_system(idx(&a, "/dev/nvme0n1"));
    a.disk_pw1.value = "disk".into();
    a.disk_pw2.value = "disk".into();
    a.same_as_disk = true;
    let p = a.build_plan().unwrap();
    assert_eq!(p.user_password.unwrap().0, "disk");
    assert!(p.previous.is_none() && p.carry.is_empty());
}

#[test]
fn windows_needs_the_typed_confirm() {
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    let c = idx(&a, "/dev/nvme1n1");
    a.select_system(c);
    assert!(matches!(a.modal, Some(Modal::Windows { .. })));
    assert_ne!(a.system, Some(c));
    // Forcing it past the screen still fails.
    a.modal = None;
    a.system = Some(c);
    assert!(a.build_plan().unwrap_err().contains("windows"));
    // Typing it.
    a.system = Some(idx(&a, "/dev/nvme0n1"));
    a.select_system(c);
    typ(&mut a, "window");
    key(&mut a, KeyCode::Enter);
    assert!(a.modal.is_some(), "a typo doesn't count");
    typ(&mut a, "windows");
    key(&mut a, KeyCode::Enter);
    assert!(a.modal.is_none());
    assert_eq!(a.system, Some(c));
    assert!(a.build_plan().is_ok());
    // Esc leaves it alone.
    let sda = idx(&a, "/dev/sda");
    a.set_use(sda, Use::Format);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.data_rows[sda].use_, Use::NotUsed);
}

#[test]
fn data_drives() {
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    let (c, l, t7) = (idx(&a, "/dev/nvme1n1"), idx(&a, "/dev/nvme2n1"), idx(&a, "/dev/sdc"));
    a.set_use(c, Use::Mount);
    a.set_use(l, Use::Mount);
    a.set_use(t7, Use::Format);
    a.sync();
    assert_eq!((a.data_rows[c].name.value.as_str(), a.data_rows[l].name.value.as_str()), ("c", "l"));
    assert_eq!(a.data_rows[t7].name.value, "games");
    let p = a.build_plan().unwrap();
    assert_eq!(p.data.len(), 3);
    let cd = p.data.iter().find(|d| d.name == "c").unwrap();
    assert_eq!(cd.mount, "/mnt/c");
    assert_eq!(cd.mode, DataMode::Mount { uuid: "7A3E5B2C3E5AE7F1".into(), fs_type: "ntfs3".into(), read_only: true });
    let g = p.data.iter().find(|d| d.name == "games").unwrap();
    assert_eq!(g.mount, "/home/vainnor/games");
    assert_eq!(g.mode, DataMode::Format { device: "/dev/disk/by-id/usb-Samsung_PSSD_T7_S6WBNS0T123456A-0:0".into(), encrypt: true });
    // The mount path follows the username until edited.
    a.username.value = "carson".into();
    a.sync();
    assert_eq!(a.data_rows[t7].mount.value, "/home/carson/games");
    // Duplicate names are refused.
    a.data_rows[l].name.value = "c".into();
    a.data_rows[l].name_edited = true;
    a.sync();
    assert!(a.build_plan().unwrap_err().contains("called c"));
}

#[test]
fn backup_target_must_survive() {
    let mut a = unlocked();
    passwords(&mut a);
    a.home = HomeKind::Backup;
    let disks = a.disks.clone();
    a.apply(TaskOut::Backup(demo::backup_targets(&disks, &["/dev/nvme0n1".into()])));
    assert_eq!(a.backup_sel.as_deref(), Some("/dev/sdc1"));
    assert_eq!(a.build_plan().unwrap().home, HomePlan::Backup { partition: "/dev/sdc1".into() });
    // Formatting the T7 drops the choice.
    a.set_use(idx(&a, "/dev/sdc"), Use::Format);
    assert!(a.backup_sel.is_none());
    assert!(a.build_plan().is_err());
}

#[test]
fn walk_the_screens_with_keys() {
    let mut a = app();
    a.fix_focus();
    // welcome: down to [ start ], enter.
    assert_eq!(a.focus, Some(Field::FetchLatest));
    key(&mut a, KeyCode::Char(' '));
    assert!(!a.fetch_latest);
    key(&mut a, KeyCode::Down);
    assert_eq!(a.focus, Some(Field::WifiOpen));
    key(&mut a, KeyCode::Down);
    assert_eq!(a.focus, Some(Field::Start));
    // (tasks run on threads; apply their results directly)
    a.apply(TaskOut::Loaded(Ok(Box::new(demo::data()))));
    assert_eq!(a.screen, Screen::GitHub);
    assert_eq!(a.focus, Some(Field::GhLogin));
    // [ next ] isn't there before logging in, and skipping is refused while github is reachable.
    assert!(a.check(Screen::GitHub).unwrap_err().contains("log in"));
    a.focus = Some(Field::GhSkip);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.screen, Screen::GitHub);
    a.apply(TaskOut::Gh(GhOut::Code(Ok(code())), vec![]));
    assert!(matches!(a.gh.phase, GhPhase::Code(_)));
    a.gh_cancel();
    logged_in(&mut a, owner());
    a.fix_focus();
    assert_eq!(a.focus, Some(Field::GhPush));
    key(&mut a, KeyCode::Down);
    assert_eq!(a.focus, Some(Field::Next));
    a.apply(prepared_repo());
    assert_eq!(a.screen, Screen::Machine);
    a.focus = Some(Field::Next);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.screen, Screen::Previous);
    // skip the previous install
    a.focus = Some(Field::SkipPrevious);
    key(&mut a, KeyCode::Enter);
    // netbird: can't go on without logging in or skipping
    assert_eq!(a.screen, Screen::NetBird);
    assert_eq!(a.focus, Some(Field::NbLogin));
    a.focus = Some(Field::NbSkip);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.screen, Screen::System);
    // q quits before anything ran.
    let mut b = app();
    key(&mut b, KeyCode::Char('q'));
    assert!(b.exit.is_some());
    // esc goes back
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.screen, Screen::NetBird);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.screen, Screen::Previous);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.screen, Screen::Machine);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.screen, Screen::GitHub);
}

#[test]
fn netbird_step() {
    let mut a = unlocked();
    passwords(&mut a);
    a.nb.skipped = false;
    // Carried over: no screen, nothing logged in here.
    assert!(a.carry.contains(&super::super::plan::Carry::NetBird));
    assert!(!a.order().contains(&Screen::NetBird));
    assert!(!a.build_plan().unwrap().netbird);
    a.carry.remove(&super::super::plan::Carry::NetBird);
    assert!(a.order().contains(&Screen::NetBird));
    let o = a.order();
    let at = |s: Screen| o.iter().position(|x| *x == s).unwrap();
    assert!(at(Screen::Previous) < at(Screen::NetBird) && at(Screen::NetBird) < at(Screen::System), "after the machine is named");
    assert!(a.build_plan().unwrap_err().contains("netbird"));
    a.go(Screen::NetBird);
    a.apply(TaskOut::Nb(NbOut::Started(Ok(NbStart::Login(NbLogin { url: "https://login.netbird.io/activate?user_code=QXRT-WZPL".into(), code: Some("QXRT-WZPL".into()) }))), vec![]));
    assert!(matches!(a.nb.phase, NbPhase::Code(_)));
    a.nb_cancel();
    assert!(matches!(a.nb.phase, NbPhase::Start));
    a.nb.host = "arc".into();
    a.apply(TaskOut::Nb(NbOut::Done(Ok(nb_status())), vec!["$ netbird up --no-browser --hostname arc".into()]));
    assert!(a.nb.done());
    assert!(a.build_plan().unwrap().netbird);
    // Offline: no netbird step.
    let mut b = App::new(Backend::Demo, true, None, None, false);
    b.apply(TaskOut::Probed(Box::new(demo::machine()), Ok(demo::disks())));
    assert!(!b.order().contains(&Screen::NetBird));
}

#[test]
fn secure_boot_setup_mode_and_firmware_reboot() {
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    assert!(a.build_plan().unwrap().setup_mode, "the demo firmware is in setup mode");
    a.machine.as_mut().unwrap().setup_mode = false;
    assert!(!a.build_plan().unwrap().setup_mode);
    // The welcome screen offers the bios when not in setup mode.
    let mut w = app();
    w.machine.as_mut().unwrap().setup_mode = false;
    w.fix_focus();
    assert!(w.focusables().contains(&Field::RebootFirmware));
    w.focus = Some(Field::RebootFirmware);
    key(&mut w, KeyCode::Enter);
    assert_eq!(w.exit, Some(Exit::Firmware));
    assert!(!app().focusables().contains(&Field::RebootFirmware), "setup mode already: nothing to do");
}

#[test]
fn new_machine_plan() {
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    a.focus = Some(Field::NewHost);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.host_choice, None);
    assert_eq!(a.focus, Some(Field::HostName), "typing the name next");
    assert!(a.build_plan().unwrap_err().contains("name"), "no name yet");
    typ(&mut a, "arc");
    assert!(a.build_plan().unwrap_err().contains("already in the repo"));
    a.new_name.value = "mac".into();
    assert!(a.build_plan().unwrap_err().contains("taken"));
    a.new_name.value = "forge".into();
    // gaming on, gpu to nvidia.
    let gaming = a.data.as_ref().unwrap().aspects.iter().position(|x| x.name == "gaming").unwrap();
    a.focus = Some(Field::Aspect(gaming));
    key(&mut a, KeyCode::Char(' '));
    a.focus = Some(Field::Gpu);
    key(&mut a, KeyCode::Right);
    let p = a.build_plan().unwrap();
    let HostPlan::New(h) = &p.host else { panic!("{:?}", p.host) };
    assert_eq!(h.name, "forge");
    assert_eq!(h.aspects, ["base", "desktop", "dev", "privacy", "gaming"]);
    assert_eq!(h.gpu, Gpu::Nvidia);
    assert_eq!(h.state_version, "26.11");
    assert_eq!(p.settings.hostname, "forge", "the hostname is the machine's name");
    // desktop off takes dev and gaming with it (they need it); gaming back on brings it back.
    let pos = |a: &App, n: &str| a.data.as_ref().unwrap().aspects.iter().position(|x| x.name == n).unwrap();
    a.focus = Some(Field::Aspect(pos(&a, "desktop")));
    key(&mut a, KeyCode::Char(' '));
    let HostPlan::New(h) = a.build_plan().unwrap().host else { panic!() };
    assert_eq!(h.aspects, ["base", "privacy"]);
    a.focus = Some(Field::Aspect(gaming));
    key(&mut a, KeyCode::Char(' '));
    let HostPlan::New(h) = a.build_plan().unwrap().host else { panic!() };
    assert_eq!(h.aspects, ["base", "desktop", "privacy", "gaming"]);
    a.new_name.value = "gaming".into();
    assert!(a.build_plan().unwrap_err().contains("feature"), "a host can't share an aspect's name");
    a.new_name.value = "forge".into();
    // Back to arc.
    a.host_choice = Some("arc".into());
    let p = a.build_plan().unwrap();
    assert_eq!(p.host, HostPlan::Existing("arc".into()));
    assert_eq!(p.settings.hostname, "arc");
}

#[test]
fn github_plans() {
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    let g = a.build_plan().unwrap().github;
    assert_eq!((g.mode.clone(), g.push, g.login.as_deref()), (GitHubMode::Owner, true, Some("Vainnor")));
    assert_eq!(g.upstream, "vainnor/nixos");
    assert_eq!(g.email.as_deref(), Some("vainnor@example.com"));
    a.gh.push = false;
    assert!(!a.build_plan().unwrap().github.pushes(), "the owner can choose not to push");

    // Someone without a fork can't go on.
    logged_in(&mut a, someone(false));
    assert!(a.check(Screen::GitHub).unwrap_err().contains("no fork"));
    assert!(a.build_plan().is_err());
    // Forked: cloned from and pushed to the fork, always.
    a.apply(TaskOut::Gh(GhOut::Forked(Ok(someone(true).fork.unwrap())), vec![]));
    assert!(a.check(Screen::GitHub).is_ok());
    assert_eq!(a.origin(), "https://github.com/someone/nixos.git");
    assert!(a.forking());
    // No email on the profile: asked on the options screen.
    a.email.value.clear();
    assert!(a.build_plan().unwrap_err().contains("email"));
    a.email.value = "someone@example.com".into();
    let g = a.build_plan().unwrap().github;
    assert_eq!(g.mode, GitHubMode::Fork { url: "https://github.com/someone/nixos.git".into(), branch: "main".into() });
    assert!(g.push);
    assert_eq!((g.full_name.as_deref(), g.email.as_deref()), (Some("Some One"), Some("someone@example.com")));
    assert_eq!(g.keys, ["ssh-ed25519 AAAAsomeone"]);

    // Offline (no network): no github step at all.
    let mut b = App::new(Backend::Demo, true, None, None, false);
    b.write_log = false;
    b.apply(TaskOut::Probed(Box::new(demo::machine()), Ok(demo::disks())));
    assert!(!b.uses_github());
    assert!(!b.order().contains(&Screen::GitHub));
    // GitHub unreachable: skipping becomes possible.
    let mut c = app();
    c.apply(TaskOut::Loaded(Ok(Box::new(demo::data()))));
    c.apply(TaskOut::Gh(GhOut::Code(Err("couldn't reach github: timeout".into())), vec![]));
    assert!(c.gh.unreachable);
    c.focus = Some(Field::GhSkip);
    key(&mut c, KeyCode::Enter);
    assert!(c.gh.skipped);
    assert_eq!(c.github_plan().mode, GitHubMode::Offline);
}

#[test]
fn install_end_to_end_with_the_dry_run_engine() {
    let mut a = unlocked();
    passwords(&mut a);
    a.go(Screen::Review);
    a.focus = Some(Field::ConfirmYes);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.screen, Screen::Review, "no YES, no install");
    typ(&mut a, "YES");
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.screen, Screen::Install);
    // Quitting is refused while it runs.
    key(&mut a, KeyCode::Char('q'));
    assert!(a.exit.is_none());
    let t = Instant::now();
    while matches!(a.screen, Screen::Install) && t.elapsed() < Duration::from_secs(30) {
        a.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    let p = a.progress.as_ref().unwrap();
    assert_eq!(a.screen, Screen::Summary, "error: {:?}\n{}", p.error, p.log.join("\n"));
    assert!(p.steps.iter().all(|s| matches!(s.1, Some(StepState::Done | StepState::Skipped))), "{:?}", p.steps);
    assert!(p.log.iter().any(|l| l.starts_with("$ ")), "commands are logged");
    assert_eq!(p.log[0], "$ gh api user", "github's commands go first");
    assert!(p.log.iter().any(|l| l.contains(" push -u origin HEAD:main")));
    let all = p.log.join("\n");
    let leak: Vec<&str> = all.lines().filter(|l| l.contains("old-pass") || l.contains("s3cr3t-pw")).collect();
    assert!(leak.is_empty(), "no passwords in the log: {leak:?}");
}

// ---------------------------------------------------------------- render dumps

fn render(a: &App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| view::draw(f, a)).unwrap();
    let buf = t.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..h {
        let mut line = String::new();
        for x in 0..w {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// The same render as HTML with colours (target/screens/<name>.html), for reviewing the look.
fn render_html(a: &App, w: u16, h: u16) -> String {
    use ratatui::style::{Color, Modifier};
    fn css(c: Color, default: &str) -> String {
        match c {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            Color::Reset => default.to_string(),
            other => format!("{other}").to_lowercase(),
        }
    }
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| view::draw(f, a)).unwrap();
    let buf = t.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            let c = &buf[(x, y)];
            let (fg, bg, bold) = (c.fg, c.bg, c.modifier.contains(Modifier::BOLD));
            let mut run = String::new();
            while x < w {
                let d = &buf[(x, y)];
                if d.fg != fg || d.bg != bg || d.modifier.contains(Modifier::BOLD) != bold {
                    break;
                }
                run.push_str(&d.symbol().replace('&', "&amp;").replace('<', "&lt;"));
                x += 1;
            }
            out.push_str(&format!(
                "<span style=\"color:{};background:{}{}\">{run}</span>",
                css(fg, "inherit"),
                css(bg, "transparent"),
                if bold { ";font-weight:700" } else { "" }
            ));
        }
        out.push('\n');
    }
    out
}

fn dump(name: &str, a: &mut App) {
    let html = render_html(a, 120, 40);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/screens");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.html")), html).unwrap();
    a.fix_focus();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/screens");
    std::fs::create_dir_all(&dir).unwrap();
    for (w, h) in [(120, 40), (80, 24)] {
        let s = render(a, w, h);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), h as usize);
        assert!(lines.iter().all(|l| l.chars().count() <= w as usize));
        assert!(lines[0].contains("install"), "{name}: header");
        assert!(lines[1].starts_with('┌') && lines[h as usize - 2].starts_with('└'), "{name}: body border\n{s}");
        assert!(!lines[h as usize - 1].trim().is_empty(), "{name}: help line");
        std::fs::write(dir.join(format!("{name}-{w}x{h}.txt")), &s).unwrap();
    }
}

fn focus_last(a: &mut App) {
    a.focus = a.focusables().last().copied();
}

#[test]
fn render_every_screen() {
    let mut a = App::new(Backend::Demo, false, None, None, false);
    dump("welcome-probing", &mut a);
    let mut a = app();
    dump("welcome", &mut a);
    a.machine.as_mut().unwrap().setup_mode = false;
    a.focus = Some(Field::RebootFirmware);
    dump("welcome-no-setup-mode", &mut a);

    // Wi-Fi: offline welcome, the list with a password, connected.
    let mut a = app();
    a.machine.as_mut().unwrap().online = false;
    a.fetch_latest = false;
    a.focus = None;
    dump("welcome-offline", &mut a);
    assert_eq!(a.focus, Some(Field::WifiOpen));
    a.go(Screen::Wifi);
    dump("wifi-scanning", &mut a);
    a.wifi.networks = Some(super::super::engine::wifi::parse_scan(
        ":82:WPA2:home\n:61:WPA2 WPA3:home-5g\n:40::cafe guest\n:22:WPA2 802.1X:work\n",
    ));
    a.wifi_pick(0);
    assert_eq!(a.focus, Some(Field::WifiPw));
    a.wifi.pw.value = "hunter2hunter2".into();
    a.wifi.error = Some("wrong password".into());
    dump("wifi-pick", &mut a);
    assert!(!render(&a, 120, 40).contains("hunter2"));
    a.wifi.error = None;
    a.wifi.connected = Some("home".into());
    a.machine.as_mut().unwrap().online = true;
    a.focus = None;
    dump("wifi-connected", &mut a);
    a.back();
    assert_eq!(a.screen, Screen::Welcome);

    // NetBird: log in, the link and code, logged in.
    let mut a = unlocked();
    a.carry.remove(&super::super::plan::Carry::NetBird);
    a.nb.skipped = false;
    a.go(Screen::NetBird);
    dump("netbird-login", &mut a);
    a.nb.host = "arc".into();
    a.nb.phase = NbPhase::Code(NbLogin { url: "https://login.netbird.io/activate?user_code=QXRT-WZPL".into(), code: Some("QXRT-WZPL".into()) });
    a.focus = Some(Field::NbCancel);
    dump("netbird-code", &mut a);
    a.nb.phase = NbPhase::Done(nb_status());
    a.focus = None;
    dump("netbird-done", &mut a);

    // GitHub: log in, the code, the owner, someone without and with a fork.
    let mut a = app();
    a.apply(TaskOut::Loaded(Ok(Box::new(demo::data()))));
    dump("github-login", &mut a);
    a.apply(TaskOut::Gh(GhOut::Code(Err("couldn't reach github: curl: (6) could not resolve host: github.com".into())), vec![]));
    a.focus = Some(Field::GhSkip);
    dump("github-unreachable", &mut a);
    a.gh.phase = GhPhase::Code(code());
    a.focus = Some(Field::GhCancel);
    dump("github-code", &mut a);
    logged_in(&mut a, owner());
    a.focus = Some(Field::GhPush);
    dump("github-owner", &mut a);
    logged_in(&mut a, someone(false));
    a.focus = Some(Field::GhFork);
    dump("github-no-fork", &mut a);
    logged_in(&mut a, someone(true));
    dump("github-fork", &mut a);

    // Machine: reinstall arc, or a new one.
    let mut a = prepared();
    dump("machine-reinstall", &mut a);
    a.host_choice = None;
    a.new_name.value = "forge".into();
    a.focus = Some(Field::HostName);
    dump("machine-new", &mut a);

    let mut a = prepared();
    a.focus = Some(Field::UnlockPassword);
    a.unlock_pw.value = "hunter2".into();
    a.unlock_error = Some("wrong disk password: try again".into());
    dump("previous-unlock", &mut a);

    let mut a = unlocked();
    a.home = HomeKind::Backup;
    let disks = a.disks.clone();
    a.apply(TaskOut::Backup(demo::backup_targets(&disks, &["/dev/nvme0n1".into()])));
    a.focus = Some(Field::Home(HomeKind::Backup));
    dump("previous-carry", &mut a);

    let mut a = unlocked();
    a.go(Screen::System);
    a.focus = Some(Field::Disk(0));
    dump("system", &mut a);
    a.select_system(idx(&a, "/dev/nvme1n1"));
    typ(&mut a, "wind");
    dump("system-windows", &mut a);

    let mut a = unlocked();
    a.go(Screen::Data);
    let (c, l, t7) = (idx(&a, "/dev/nvme1n1"), idx(&a, "/dev/nvme2n1"), idx(&a, "/dev/sdc"));
    a.set_use(c, Use::Mount);
    a.set_use(t7, Use::Format);
    a.sync();
    a.focus = Some(Field::DataUse(l));
    dump("data", &mut a);

    let mut a = prepared();
    a.skipped_previous = true;
    a.go(Screen::Options);
    a.username.value = "carson".into();
    a.focus = Some(Field::Swap);
    dump("options", &mut a);
    focus_last(&mut a);
    dump("options-bottom", &mut a);
    let mut a = unlocked();
    a.go(Screen::Options);
    dump("options-keep", &mut a);

    let mut a = prepared();
    a.skipped_previous = true;
    a.go(Screen::Passwords);
    a.disk_pw1.value = "abc".into();
    a.disk_pw2.value = "abd".into();
    a.focus = Some(Field::LoginPw1);
    dump("passwords", &mut a);

    // Review: install onto the Sabrent C: with the old NixOS disk left in.
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    let c = idx(&a, "/dev/nvme1n1");
    a.confirmed_windows.insert("/dev/nvme1n1".into());
    a.select_system(c);
    a.set_use(idx(&a, "/dev/sdc"), Use::Format);
    a.set_use(idx(&a, "/dev/nvme2n1"), Use::Mount);
    a.sync();
    a.go(Screen::Review);
    a.confirm.value = "YE".into();
    a.focus = Some(Field::ConfirmYes);
    dump("review", &mut a);
    let mut a = unlocked();
    passwords(&mut a);
    a.go(Screen::Review);
    a.confirm.value = "YES".into();
    a.focus = Some(Field::Install);
    dump("review-keep", &mut a);

    // A new machine from someone's fork.
    let mut a = prepared();
    a.skipped_previous = true;
    passwords(&mut a);
    logged_in(&mut a, someone(true));
    a.email.value = "someone@example.com".into();
    a.host_choice = None;
    a.new_name.value = "forge".into();
    a.select_system(idx(&a, "/dev/sdc"));
    a.go(Screen::Options);
    a.focus = Some(Field::Email);
    dump("options-fork", &mut a);
    a.go(Screen::Review);
    a.focus = Some(Field::ConfirmYes);
    dump("review-new-fork", &mut a);

    // Progress, summary, failure.
    let mut a = unlocked();
    passwords(&mut a);
    let plan = a.build_plan().unwrap();
    let mut p = Progress::new(a.backend.steps(&plan));
    for (i, s) in p.steps.iter_mut().enumerate() {
        s.1 = match i {
            0 | 1 => Some(StepState::Done),
            2 => Some(StepState::Skipped),
            3 => Some(StepState::Running),
            _ => None,
        };
    }
    p.log = (0..40).map(|i| if i % 4 == 0 { format!("$ mount -o ro /dev/mapper/old /run/vx-install/old/{i}") } else { format!("copying path {i}") }).collect();
    a.progress = Some(p);
    a.screen = Screen::Install;
    dump("install", &mut a);

    let p = a.progress.as_mut().unwrap();
    p.outcome = Some(Outcome {
        age_key: "age1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqs3290gq".into(),
        host_key_carried: false,
        next_steps: vec![
            "remove the usb stick and reboot into nixos".into(),
            "turn secure boot on in the bios (your keys are enrolled); the disk then unlocks itself".into(),
            "secrets: your other machines add this one automatically (or run vx secrets rekey on the mac); it switches by itself once they have".into(),
        ],
        pushed: Some("vainnor/nixos main".into()),
        not_pushed: None,
        sb_enrolled: true,
        secrets_pending: true,
        netbird: Some("100.92.14.7 (arc.netbird.cloud)".into()),
    });
    a.screen = Screen::Summary;
    a.focus = None;
    dump("summary", &mut a);
    let o = a.progress.as_mut().unwrap().outcome.as_mut().unwrap();
    o.pushed = None;
    o.not_pushed = Some("the push failed (! [rejected] HEAD -> main (fetch first)): push from ~/nixos later".into());
    dump("summary-not-pushed", &mut a);

    let p = a.progress.as_mut().unwrap();
    p.outcome = None;
    p.steps[3].1 = Some(StepState::Failed("mkfs.btrfs failed".into()));
    p.error = Some("mkfs.btrfs -L nixos /dev/mapper/cryptroot: exit status 1 (device busy)".into());
    a.screen = Screen::Failed;
    a.focus = None;
    dump("failed", &mut a);

    // The raw console: ANSI colours, no braille or arrows.
    let mut a = App::new(Backend::Demo, false, None, None, true);
    a.write_log = false;
    a.apply(TaskOut::Probed(Box::new(demo::machine()), Ok(demo::disks())));
    a.go(Screen::Welcome);
    let s = render(&a, 80, 24);
    assert!(s.contains("up/down") && !s.contains('↑'));
}
