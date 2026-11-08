//! Plan building from screen state, the key flow, and render dumps of every
//! screen (tools/vx/target/screens/<screen>-<size>.txt).

use super::super::plan::{Carry, DataMode, HomePlan, Outcome, Previous, StepState, Unlocked, OLD_MOUNT, WORK_REPO};
use super::app::{App, Field, HomeKind, Modal, Progress, Screen, TaskOut, Use};
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

/// Past the welcome screen (repo prepared).
fn prepared() -> App {
    let mut a = app();
    a.apply(TaskOut::Prepared(Ok((Box::new(demo::data()), PathBuf::from(WORK_REPO), None))));
    a
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
    assert_eq!(a.screen, Screen::Previous);
    assert_eq!(a.system_disk().unwrap().path, "/dev/nvme0n1", "the previous install's disk");
    assert_eq!(a.hostname.value, "arc");
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
    assert_eq!(a.focus, Some(Field::Start));
    // (prepare runs on a thread; apply its result directly)
    a.apply(TaskOut::Prepared(Ok((Box::new(demo::data()), PathBuf::from(WORK_REPO), None))));
    assert_eq!(a.screen, Screen::Previous);
    // skip the previous install
    a.focus = Some(Field::SkipPrevious);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.screen, Screen::System);
    // q quits before anything ran.
    let mut b = app();
    key(&mut b, KeyCode::Char('q'));
    assert!(b.exit.is_some());
    // esc goes back
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.screen, Screen::Previous);
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
            "remove the usb stick and reboot; if windows starts, pick the nixos drive with f11.".into(),
            "secure boot: in the bios, reset to setup mode, boot nixos once so it enrolls its keys, then turn secure boot on.".into(),
            "secrets: on the mac run  vx secrets add-host <the age key above>, push, then  vx switch  here.".into(),
            "github: gh auth login (so git push works)".into(),
        ],
    });
    a.screen = Screen::Summary;
    a.focus = None;
    dump("summary", &mut a);

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
