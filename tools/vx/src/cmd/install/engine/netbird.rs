//! NetBird login on the installer, so the new system is on the mesh from its
//! first boot (no `sudo netbird up` afterwards).
//!
//! How: the ISO runs the same NetBird service as the installed system
//! (services.netbird, state in [`NETBIRD_STATE`]), so this is just
//! `netbird up --no-browser --hostname <host>` against it. `up` asks the
//! daemon to log in; a daemon without a graphical session uses the
//! device-code flow, and `up` prints
//! `Use this URL to log in:\n\n<url> [and enter the code <code> to authenticate.]`
//! (NetBird 0.82, client/cmd/login.go openURL) and then waits until the login
//! is done. We show that URL and code on the screen instead of a browser.
//! Why not `up --foreground-mode`: it runs the whole client inside `up`
//! (no daemon to ask for status, it never returns while connected) and would
//! need its own config; the daemon keeps the login exactly where the
//! installed system looks for it.
//!
//! Before the copy into the new system (restore) the engine runs
//! `netbird down`, so the installer and the installed system never connect
//! with the same key at the same time. The home backup restore (restic.rs)
//! runs before that, while the usb can still reach the homelab. The private
//! key in config.json is never logged.
//!
//! Graphical-session variables are cleared for `up` so the browser (PKCE)
//! flow, which needs a browser on this machine, is never picked.

use super::super::plan::{NETBIRD_CONFIG, NETBIRD_STATE};
use super::runner::{Cmd, Runner};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// The running `netbird up` (waits for the login), and its output lines.
static UP: Mutex<Option<(Child, Receiver<String>)>> = Mutex::new(None);

/// What `up` showed: where to log in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub url: String,
    /// The code to enter, when known.
    pub code: Option<String>,
}

/// This peer on the mesh (`netbird status --json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub ip: String,
    pub fqdn: String,
}

impl Status {
    pub fn show(&self) -> String {
        match (self.ip.is_empty(), self.fqdn.is_empty()) {
            (false, false) => format!("{} ({})", self.ip, self.fqdn),
            (false, true) => self.ip.clone(),
            (true, false) => self.fqdn.clone(),
            (true, true) => "connected".into(),
        }
    }
}

/// What [`start`] found.
#[derive(Clone, Debug)]
pub enum Start {
    /// Log in at this URL; [`wait`] then waits for it.
    Login(Login),
    /// Already logged in (e.g. a second try after a failed install).
    Connected(Status),
}

pub fn state_dir() -> PathBuf {
    PathBuf::from(NETBIRD_STATE)
}

/// `netbird` (the NixOS wrapper sets the daemon address and paths).
pub fn nb() -> Cmd {
    Cmd::new("netbird")
}

/// `netbird up` for `host`: prints the login URL instead of opening a browser.
pub fn up_cmd(host: &str) -> Cmd {
    let mut c = nb().args(["up", "--no-browser", "--hostname", host]);
    for v in ["DISPLAY", "WAYLAND_DISPLAY", "BROWSER", "DESKTOP_SESSION", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE"] {
        c = c.env(v, "");
    }
    c
}

/// `netbird down`: disconnects (the login stays in config.json).
pub fn down_cmd() -> Cmd {
    nb().arg("down")
}

/// Copy the whole state folder into the new system: dirs 0700, files 0600.
pub fn copy_cmd(to: &Path) -> Cmd {
    Cmd::new("rsync")
        .args(["-rlt", "--chmod=D700,F600"])
        .arg(format!("{}/", state_dir().display()))
        .arg(format!("{}/", to.display()))
}

fn command(c: &Cmd) -> Command {
    let mut cmd = Command::new(&c.prog);
    cmd.args(&c.args).current_dir("/");
    for (k, v) in &c.env {
        cmd.env(k, v);
    }
    cmd
}

/// Pretend waits in the demo, none in tests.
fn pause(ms: u64) {
    if !cfg!(test) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// Where to log in, from `up`'s output (None until the URL line came).
pub fn parse_login(out: &str) -> Option<Login> {
    let url = out.split_whitespace().find(|w| w.starts_with("https://") || w.starts_with("http://"))?.to_string();
    let code = out
        .split("enter the code ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .map(|c| c.trim_end_matches('.').to_string())
        .filter(|c| !c.is_empty())
        .or_else(|| url.split("user_code=").nth(1).map(|c| c.split('&').next().unwrap_or(c).to_string()));
    Some(Login { url, code })
}

pub fn parse_status(json: &str) -> Result<Status> {
    let v: Value = serde_json::from_str(json).context("netbird status isn't json")?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let ip = s("netbirdIp");
    let ip = ip.split('/').next().unwrap_or("").to_string();
    Ok(Status { ip, fqdn: s("fqdn") })
}

fn last_lines(s: &str) -> String {
    let l: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let t = l[l.len().saturating_sub(3)..].join(" / ");
    if t.is_empty() { "no output".into() } else { t }
}

/// Run `netbird up`; returns the URL to log in at, or Connected when the
/// daemon is logged in already.
pub fn start(r: &Runner, host: &str) -> Result<Start> {
    if r.dry() {
        r.log(format!("$ {}", up_cmd(host).line()));
        pause(1200);
        return Ok(Start::Login(Login { url: "https://login.netbird.io/activate?user_code=QXRT-WZPL".into(), code: Some("QXRT-WZPL".into()) }));
    }
    stop();
    let c = up_cmd(host);
    r.log(format!("$ {}", c.line()));
    let mut child = command(&c)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not start netbird (is it on the installer?)")?;
    let (tx, rx) = mpsc::channel();
    let streams: Vec<Box<dyn std::io::Read + Send>> = [
        child.stdout.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|e| Box::new(e) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    for s in streams {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for l in BufReader::new(s).lines().map_while(Result::ok) {
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let mut out = String::new();
    let end = Instant::now() + Duration::from_secs(90);
    loop {
        out.extend(rx.try_iter().map(|l| l + "\n"));
        if let Some(login) = parse_login(&out) {
            *UP.lock().unwrap_or_else(|e| e.into_inner()) = Some((child, rx));
            return Ok(Start::Login(login));
        }
        if let Some(st) = child.try_wait()? {
            std::thread::sleep(Duration::from_millis(100));
            out.extend(rx.try_iter().map(|l| l + "\n"));
            if let Some(login) = parse_login(&out) {
                return Ok(Start::Login(login));
            }
            if st.success() {
                return Ok(Start::Connected(status(r)?));
            }
            bail!("netbird up failed: {}", last_lines(&out));
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            bail!("netbird up showed no login link: {}", last_lines(&out));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Wait until the login is done (`up` exits), then this peer's address.
/// `cancel` stops waiting (the screen's [ cancel ]).
pub fn wait(r: &Runner, cancel: &AtomicBool) -> Result<Status> {
    if r.dry() {
        let end = Instant::now() + Duration::from_millis(if cfg!(test) { 0 } else { 3000 });
        loop {
            if cancel.load(Ordering::SeqCst) {
                bail!("cancelled");
            }
            if Instant::now() >= end {
                return status(r);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    loop {
        if cancel.load(Ordering::SeqCst) {
            stop();
            bail!("cancelled");
        }
        let done = {
            let mut g = UP.lock().unwrap_or_else(|e| e.into_inner());
            let Some((child, rx)) = g.as_mut() else { bail!("netbird up isn't running: log in again") };
            match child.try_wait()? {
                None => None,
                Some(st) => {
                    let out: Vec<String> = rx.try_iter().collect();
                    *g = None;
                    Some((st, out))
                }
            }
        };
        if let Some((st, out)) = done {
            if !st.success() {
                bail!("the netbird login failed: {}", last_lines(&out.join("\n")));
            }
            return status(r);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// `netbird status --json`.
pub fn status(r: &Runner) -> Result<Status> {
    let out = r.query(
        nb().args(["status", "--json"])
            .fake(r#"{"netbirdIp":"100.92.14.7/16","fqdn":"forge.netbird.cloud","management":{"connected":true}}"#),
    )?;
    parse_status(&out)
}

/// Stop a waiting `up`, if any (the login screen was left, or the installer quits).
pub fn stop() {
    if let Some((mut c, _)) = UP.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Logged in here: the login file is there.
pub fn logged_in(r: &Runner) -> bool {
    let f = state_dir().join(NETBIRD_CONFIG);
    r.exists(&f, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_text() {
        let out = "Use this URL to log in:\n\nhttps://login.netbird.io/activate?user_code=ABCD-EFGH \n\n";
        assert_eq!(
            parse_login(out),
            Some(Login { url: "https://login.netbird.io/activate?user_code=ABCD-EFGH".into(), code: Some("ABCD-EFGH".into()) })
        );
        let out = "Use this URL to log in:\n\nhttps://idp.example.com/device and enter the code WXYZ-1234 to authenticate.\n";
        assert_eq!(parse_login(out), Some(Login { url: "https://idp.example.com/device".into(), code: Some("WXYZ-1234".into()) }));
        assert_eq!(parse_login("Connecting ...\n"), None);
    }

    #[test]
    fn status_json() {
        let s = parse_status(r#"{"netbirdIp":"100.92.14.7/16","fqdn":"forge.netbird.cloud","peers":{}}"#).unwrap();
        assert_eq!(s, Status { ip: "100.92.14.7".into(), fqdn: "forge.netbird.cloud".into() });
        assert_eq!(s.show(), "100.92.14.7 (forge.netbird.cloud)");
        assert!(parse_status("Daemon status: NeedsLogin").is_err());
    }

    #[test]
    fn commands() {
        let u = up_cmd("forge");
        assert_eq!(u.line(), "netbird up --no-browser --hostname forge");
        assert!(u.env.iter().any(|(k, v)| k == "DISPLAY" && v.is_empty()));
        assert_eq!(
            copy_cmd(Path::new("/mnt/persistent/var/lib/netbird")).line(),
            "rsync -rlt --chmod=D700,F600 /var/lib/netbird/ /mnt/persistent/var/lib/netbird/"
        );
    }

    #[test]
    fn dry_login() {
        let r = Runner::new(super::super::Mode::DryRun, None);
        let Start::Login(l) = start(&r, "forge").unwrap() else { panic!() };
        assert_eq!(l.code.as_deref(), Some("QXRT-WZPL"));
        assert_eq!(wait(&r, &AtomicBool::new(false)).unwrap().fqdn, "forge.netbird.cloud");
        assert!(wait(&r, &AtomicBool::new(true)).is_err(), "a cancel stops the wait");
        stop();
    }
}
