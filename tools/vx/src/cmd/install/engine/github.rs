//! GitHub for the installer: logging in, who you are, your fork of the
//! upstream repo, your public SSH keys, and forking.
//!
//! Logging in is GitHub's OAuth device flow, done here with curl and gh's
//! public OAuth client id; the token is then handed to
//! `gh auth login --with-token` on stdin, so gh (and git through
//! `gh auth git-credential`) are logged in from then on. That is sturdier than
//! driving `gh auth login --web`: gh only prints its one-time code as
//! decorated text meant for people, asks "press enter to open the browser"
//! when it thinks it has a terminal, and its browser launch fails on the ISO.
//! The device flow itself is a documented API with JSON answers.
//!
//! gh keeps its login in [`GH_CONFIG`] (`--insecure-storage`: there is no
//! keyring on the ISO, and the file is copied into the new home). Tokens and
//! device codes only ever go through stdin; command output that could hold
//! one is never logged.

use super::super::plan::{GH_CONFIG, Secret};
use super::runner::{Cmd, Runner};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// gh's OAuth app (public; the same id gh itself uses for `gh auth login`).
pub const CLIENT_ID: &str = "178c6fc778ccc68e1d6a";
/// repo + read:org are what gh needs; workflow lets a push touch .github/workflows;
/// user:email finds the email when the profile hides it.
pub const SCOPES: &str = "repo read:org gist workflow user:email";
pub const DEVICE_URL: &str = "https://github.com/login/device";

/// What the device flow's first request gives back.
#[derive(Clone, Debug)]
pub struct DeviceCode {
    /// Typed on github.com/login/device, e.g. WDJB-MJHT.
    pub user_code: String,
    pub uri: String,
    /// Proves it's us while polling. Never logged, only sent through stdin.
    pub device_code: Secret,
    /// Seconds between polls.
    pub interval: u64,
    /// Seconds until the code expires.
    pub expires_in: u64,
}

/// A fork of the upstream repo owned by the login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fork {
    /// login/repo
    pub full_name: String,
    /// https clone URL.
    pub url: String,
    pub branch: String,
}

/// The logged-in GitHub account, as far as the installer cares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    /// Public SSH keys (github.com/<login>.keys).
    pub keys: Vec<String>,
    /// The login owns the upstream repo.
    pub owner: bool,
    /// Their fork (None for the owner, or when there is none yet).
    pub fork: Option<Fork>,
}

// ---------------------------------------------------------------- demo / dry run

/// The login the dry run pretends to be (`--demo-user`).
static DEMO_LOGIN: Mutex<Option<String>> = Mutex::new(None);
/// The dry run's `gh repo fork` "made" the fork.
static DEMO_FORKED: AtomicBool = AtomicBool::new(false);

pub fn set_demo_login(login: &str) {
    if let Ok(mut l) = DEMO_LOGIN.lock() {
        *l = Some(login.to_string());
    }
    DEMO_FORKED.store(false, Ordering::SeqCst);
}

fn demo_login() -> String {
    DEMO_LOGIN.lock().ok().and_then(|l| l.clone()).unwrap_or_else(|| "Vainnor".into())
}

/// Pretend waits in the demo, none in tests.
fn pause(ms: u64) {
    if !cfg!(test) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

// ---------------------------------------------------------------- commands

/// `gh` with its config in GH_CONFIG and no prompts.
pub fn gh() -> Cmd {
    Cmd::new("gh")
        .env("GH_CONFIG_DIR", GH_CONFIG)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
}

/// `git -c ...` arguments that make git ask gh for GitHub credentials (and
/// nothing else), for clone and push. Pair with [`git_env`].
pub fn git_auth_args() -> [&'static str; 4] {
    ["-c", "credential.https://github.com.helper=", "-c", "credential.https://github.com.helper=!gh auth git-credential"]
}

/// Environment for git commands that use [`git_auth_args`].
pub fn git_env(c: Cmd) -> Cmd {
    c.env("GH_CONFIG_DIR", GH_CONFIG).env("GIT_TERMINAL_PROMPT", "0")
}

fn curl_post(url: &str, body: String) -> Cmd {
    Cmd::new("curl")
        .args(["-sS", "--max-time", "20", "-X", "POST", "-H", "Accept: application/json", "--data", "@-", url])
        .stdin(body.into_bytes())
        .quiet()
}

/// Percent-encoding for form values.
pub fn form_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Step 1: ask GitHub for a one-time code.
pub fn device_code(r: &Runner) -> Result<DeviceCode> {
    let body = format!("client_id={CLIENT_ID}&scope={}", form_encode(SCOPES));
    let out = r
        .run(curl_post("https://github.com/login/device/code", body).fake(
            r#"{"device_code":"dryrun-device-code","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":899,"interval":5}"#,
        ))
        .context("couldn't reach github")?;
    parse_device_code(&out)
}

pub fn parse_device_code(json: &str) -> Result<DeviceCode> {
    let v: Value = serde_json::from_str(json).context("github's answer isn't json")?;
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        bail!("github said {e}: {}", v.get("error_description").and_then(Value::as_str).unwrap_or(""));
    }
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Ok(DeviceCode {
        user_code: s("user_code").context("no user_code in github's answer")?,
        uri: s("verification_uri").unwrap_or_else(|| DEVICE_URL.into()),
        device_code: Secret(s("device_code").context("no device_code in github's answer")?),
        interval: v.get("interval").and_then(Value::as_u64).unwrap_or(5),
        expires_in: v.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
    })
}

/// One answer while polling for the token.
#[derive(Debug, PartialEq, Eq)]
pub enum Poll {
    Token(String),
    Pending,
    SlowDown,
    Expired,
    Denied,
    Other(String),
}

pub fn parse_poll(json: &str) -> Poll {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Poll::Other("github's answer isn't json".into()) };
    if let Some(t) = v.get("access_token").and_then(Value::as_str).filter(|t| !t.is_empty()) {
        return Poll::Token(t.to_string());
    }
    match v.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Poll::Pending,
        Some("slow_down") => Poll::SlowDown,
        Some("expired_token") => Poll::Expired,
        Some("access_denied") => Poll::Denied,
        Some(e) => Poll::Other(e.to_string()),
        None => Poll::Other("no token in github's answer".into()),
    }
}

fn poll_cmd(dc: &DeviceCode) -> Cmd {
    let body = format!(
        "client_id={CLIENT_ID}&device_code={}&grant_type={}",
        form_encode(&dc.device_code.0),
        form_encode("urn:ietf:params:oauth:grant-type:device_code")
    );
    curl_post("https://github.com/login/oauth/access_token", body)
}

/// Step 2: wait until the code was entered on github.com, then log gh in.
/// `cancel` stops the wait (the screen's [ cancel ]).
pub fn wait_and_login(r: &Runner, dc: &DeviceCode, cancel: &AtomicBool) -> Result<()> {
    let token = if r.dry() {
        pause(3000);
        let out = r.run(poll_cmd(dc).fake(r#"{"access_token":"gho_dryrunTOKENdryrunTOKEN","token_type":"bearer","scope":"repo"}"#))?;
        match parse_poll(&out) {
            Poll::Token(t) => Secret(t),
            p => bail!("unexpected answer: {p:?}"),
        }
    } else {
        poll(r, dc, cancel)?
    };
    login_with_token(r, &token)
}

fn poll(r: &Runner, dc: &DeviceCode, cancel: &AtomicBool) -> Result<Secret> {
    let mut interval = dc.interval.max(1);
    let end = Instant::now() + Duration::from_secs(dc.expires_in.max(60));
    let mut failures = 0;
    r.log(format!("$ {} (every {interval}s until the code is entered)", poll_cmd(dc).line()));
    loop {
        // Sleep in slices so cancelling is quick.
        let wake = Instant::now() + Duration::from_secs(interval);
        while Instant::now() < wake {
            if cancel.load(Ordering::SeqCst) {
                bail!("cancelled");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if Instant::now() > end {
            bail!("the code expired: log in again");
        }
        let out = match r.query(poll_cmd(dc)) {
            Ok(o) => o,
            Err(e) => {
                failures += 1;
                if failures >= 4 {
                    return Err(e.context("lost the connection to github"));
                }
                continue;
            }
        };
        failures = 0;
        match parse_poll(&out) {
            Poll::Token(t) => return Ok(Secret(t)),
            Poll::Pending => {}
            Poll::SlowDown => interval += 5,
            Poll::Expired => bail!("the code expired: log in again"),
            Poll::Denied => bail!("the login was cancelled on github: log in again"),
            Poll::Other(e) => bail!("github said {e}: log in again"),
        }
    }
}

/// `gh auth login --with-token`, the token through stdin.
pub fn login_cmd(token: &Secret) -> Cmd {
    gh().args(["auth", "login", "--hostname", "github.com", "--git-protocol", "https", "--insecure-storage", "--with-token"])
        .stdin(token.0.clone().into_bytes())
        .quiet()
}

pub fn login_with_token(r: &Runner, token: &Secret) -> Result<()> {
    r.mkdir(Path::new(GH_CONFIG), 0o700)?;
    r.run(login_cmd(token)).context("gh couldn't log in with the token")?;
    Ok(())
}

/// `gh api <path>`: Some(body), None on 404, Err when github can't be reached.
/// DryRun: Some(fake), or None when `fake` is empty.
fn api(r: &Runner, path: &str, paginate: bool, fake: String) -> Result<Option<String>> {
    let mut c = gh().arg("api");
    if paginate {
        c = c.arg("--paginate");
    }
    let c = c.arg(path).fake(fake.clone());
    if r.dry() {
        r.log(format!("$ {}", c.line()));
        return Ok((!fake.is_empty()).then_some(fake));
    }
    let (ok, out, tail) = r.output(&c)?;
    if ok {
        return Ok(Some(out));
    }
    if tail.contains("HTTP 404") || tail.contains("Not Found") {
        return Ok(None);
    }
    bail!("gh api {path} failed: {}", tail.lines().last().unwrap_or(""))
}

/// Who's logged in, their email, keys, and (unless they own `owner/repo`) their fork.
pub fn account(r: &Runner, owner: &str, repo: &str) -> Result<Account> {
    let demo = demo_login();
    let fake_user = if demo.eq_ignore_ascii_case(owner) {
        format!(r#"{{"login":"{demo}","id":61234567,"name":"{demo}","email":null}}"#)
    } else {
        format!(r#"{{"login":"{demo}","id":71234567,"name":"Some One","email":null}}"#)
    };
    let user = api(r, "user", false, fake_user)?.context("gh api user found nothing: log in again")?;
    let User { login, name, email, id } = parse_user(&user).context("couldn't read gh api user")?;
    let email = match email {
        Some(e) => Some(e),
        None => {
            let fake = format!(r#"[{{"email":"{}@example.com","primary":true,"verified":true,"visibility":"private"}}]"#, login.to_lowercase());
            // Needs the user:email scope; fine to come back empty.
            let list = api(r, "user/emails", false, fake).ok().flatten().unwrap_or_default();
            parse_primary_email(&list).or_else(|| id.map(|id| format!("{id}+{login}@users.noreply.github.com")))
        }
    };
    let fake_keys = r#"[{"id":1,"key":"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDryRunKeyDryRunKeyDryRunKeyDryRunKeyDryRun0"}]"#.to_string();
    let keys = api(r, &format!("users/{login}/keys"), false, fake_keys)?.map(|k| parse_keys(&k)).unwrap_or_default();
    let owner_login = login.eq_ignore_ascii_case(owner);
    let fork = if owner_login { None } else { find_fork(r, owner, repo, &login)? };
    Ok(Account { login, name, email, keys, owner: owner_login, fork })
}

/// What `gh api user` says (name and email are often empty).
#[derive(Debug, PartialEq, Eq)]
pub struct User {
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub id: Option<u64>,
}

pub fn parse_user(json: &str) -> Option<User> {
    let v: Value = serde_json::from_str(json).ok()?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    Some(User { login: s("login")?, name: s("name"), email: s("email"), id: v.get("id").and_then(Value::as_u64) })
}

/// The primary, verified address from `gh api user/emails`.
pub fn parse_primary_email(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let list = v.as_array()?;
    let pick = |want_primary: bool| {
        list.iter()
            .filter(|e| e.get("verified").and_then(Value::as_bool).unwrap_or(false))
            .find(|e| !want_primary || e.get("primary").and_then(Value::as_bool).unwrap_or(false))
            .and_then(|e| e.get("email").and_then(Value::as_str))
            .map(str::to_string)
    };
    pick(true).or_else(|| pick(false))
}

/// Keys from `gh api users/<login>/keys` (or github.com/<login>.keys lines).
pub fn parse_keys(text: &str) -> Vec<String> {
    let keys: Vec<String> = match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(a)) => a.iter().filter_map(|k| k.get("key").and_then(Value::as_str)).map(str::to_string).collect(),
        _ => text.lines().map(str::to_string).collect(),
    };
    keys.into_iter()
        .map(|k| k.trim().to_string())
        .filter(|k| k.starts_with("ssh-") || k.starts_with("ecdsa-") || k.starts_with("sk-"))
        .filter(|k| !k.contains('"') && !k.contains('\\') && !k.contains("${"))
        .collect()
}

/// A repo from `gh api repos/<x>/<y>` that is a fork of owner/repo (directly,
/// or further down: `source` is the root of the fork tree).
pub fn parse_fork(json: &str, owner: &str, repo: &str) -> Option<Fork> {
    let v: Value = serde_json::from_str(json).ok()?;
    fork_of(&v, &format!("{owner}/{repo}"))
}

fn fork_of(v: &Value, upstream: &str) -> Option<Fork> {
    if !v.get("fork").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let up = |k: &str| v.get(k).and_then(|p| p.get("full_name")).and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(upstream));
    if !(up("parent") || up("source")) {
        return None;
    }
    let full_name = v.get("full_name").and_then(Value::as_str)?.to_string();
    let url = v.get("clone_url").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("https://github.com/{full_name}.git"));
    let branch = v.get("default_branch").and_then(Value::as_str).unwrap_or("main").to_string();
    Some(Fork { full_name, url, branch })
}

/// The login's fork in `gh api --paginate repos/<owner>/<repo>/forks` output
/// (one JSON array per page, back to back).
pub fn find_in_forks(pages: &str, login: &str, owner: &str, repo: &str) -> Option<Fork> {
    let upstream = format!("{owner}/{repo}");
    for page in serde_json::Deserializer::from_str(pages).into_iter::<Value>().flatten() {
        let Some(list) = page.as_array() else { continue };
        for f in list {
            let mine = f.get("owner").and_then(|o| o.get("login")).and_then(Value::as_str).is_some_and(|l| l.eq_ignore_ascii_case(login));
            if !mine {
                continue;
            }
            // Listed under the upstream's forks: its parent is the upstream even when the list leaves it out.
            let mut f = f.clone();
            if f.get("parent").is_none()
                && let Some(o) = f.as_object_mut()
            {
                o.insert("parent".into(), serde_json::json!({ "full_name": upstream }));
            }
            if let Some(x) = fork_of(&f, &upstream) {
                return Some(x);
            }
        }
    }
    None
}

/// The login's fork of owner/repo: `repos/<login>/<repo>` first, else the upstream's fork list.
pub fn find_fork(r: &Runner, owner: &str, repo: &str, login: &str) -> Result<Option<Fork>> {
    let forked = DEMO_FORKED.load(Ordering::SeqCst);
    let fake = if forked {
        format!(
            r#"{{"full_name":"{login}/{repo}","fork":true,"clone_url":"https://github.com/{login}/{repo}.git","default_branch":"main","parent":{{"full_name":"{owner}/{repo}"}},"source":{{"full_name":"{owner}/{repo}"}}}}"#
        )
    } else {
        String::new()
    };
    if let Some(j) = api(r, &format!("repos/{login}/{repo}"), false, fake)?
        && let Some(f) = parse_fork(&j, owner, repo)
    {
        return Ok(Some(f));
    }
    let list = api(r, &format!("repos/{owner}/{repo}/forks?per_page=100"), true, "[]".into())?.unwrap_or_default();
    Ok(find_in_forks(&list, login, owner, repo))
}

/// `gh repo fork`, then wait (up to a minute) until GitHub shows it.
pub fn fork(r: &Runner, owner: &str, repo: &str, login: &str) -> Result<Fork> {
    r.run(gh().args(["repo", "fork", &format!("{owner}/{repo}"), "--clone=false"])).context("gh couldn't fork it")?;
    if r.dry() {
        pause(1500);
        DEMO_FORKED.store(true, Ordering::SeqCst);
    }
    for _ in 0..30 {
        if let Some(f) = find_fork(r, owner, repo, login)? {
            return Ok(f);
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    bail!("the fork doesn't show up on github yet: wait a moment and try again")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_answer() {
        let d = parse_device_code(
            r#"{"device_code":"3584d83530557fdd1f46af8289938c8ef79f9dc5","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#,
        )
        .unwrap();
        assert_eq!((d.user_code.as_str(), d.interval, d.expires_in), ("WDJB-MJHT", 5, 900));
        assert!(!format!("{d:?}").contains("3584d835"), "the device code isn't printed by Debug");
        assert!(parse_device_code(r#"{"error":"unauthorized_client","error_description":"no"}"#).is_err());
    }

    #[test]
    fn poll_answers() {
        assert_eq!(parse_poll(r#"{"error":"authorization_pending"}"#), Poll::Pending);
        assert_eq!(parse_poll(r#"{"error":"slow_down","interval":10}"#), Poll::SlowDown);
        assert_eq!(parse_poll(r#"{"error":"expired_token"}"#), Poll::Expired);
        assert_eq!(parse_poll(r#"{"error":"access_denied"}"#), Poll::Denied);
        assert_eq!(parse_poll(r#"{"access_token":"gho_x","token_type":"bearer"}"#), Poll::Token("gho_x".into()));
        assert!(matches!(parse_poll("<html>"), Poll::Other(_)));
    }

    #[test]
    fn secrets_go_through_stdin() {
        let dc = DeviceCode { user_code: "A-B".into(), uri: DEVICE_URL.into(), device_code: Secret("dev-SECRET".into()), interval: 5, expires_in: 900 };
        let c = poll_cmd(&dc);
        assert!(c.args.iter().all(|a| !a.contains("dev-SECRET")));
        assert!(String::from_utf8_lossy(c.stdin_bytes().unwrap()).contains("device_code=dev-SECRET"));
        assert!(!c.line().contains("dev-SECRET"));
        let c = login_cmd(&Secret("gho_TOKEN".into()));
        assert!(c.args.iter().all(|a| !a.contains("gho_TOKEN")) && !c.line().contains("gho_TOKEN"));
        assert_eq!(c.stdin_bytes(), Some(&b"gho_TOKEN"[..]));
        assert!(c.args.contains(&"--insecure-storage".to_string()));
    }

    #[test]
    fn encodes_form_values() {
        assert_eq!(form_encode("repo read:org"), "repo%20read%3Aorg");
        assert_eq!(form_encode("abc-_.~"), "abc-_.~");
    }

    #[test]
    fn users_and_emails() {
        let u = parse_user(r#"{"login":"Vainnor","id":42,"name":"Vainnor","email":null}"#).unwrap();
        assert_eq!(u, User { login: "Vainnor".into(), name: Some("Vainnor".into()), email: None, id: Some(42) });
        assert_eq!(parse_user(r#"{"login":"x","name":"  "}"#).unwrap().name, None);
        let emails = r#"[{"email":"old@x.com","primary":false,"verified":true},{"email":"me@x.com","primary":true,"verified":true},
            {"email":"bad@x.com","primary":true,"verified":false}]"#;
        assert_eq!(parse_primary_email(emails).as_deref(), Some("me@x.com"));
        assert_eq!(parse_primary_email(r#"{"message":"Not Found"}"#), None);
    }

    #[test]
    fn keys() {
        let k = parse_keys(r#"[{"id":1,"key":"ssh-ed25519 AAAA1"},{"id":2,"key":"ssh-rsa AAAA2"},{"id":3,"key":"evil \"${x}\""}]"#);
        assert_eq!(k, ["ssh-ed25519 AAAA1", "ssh-rsa AAAA2"]);
        assert_eq!(parse_keys("ssh-ed25519 AAAA1\nsk-ssh-ed25519@openssh.com AAAA3\n\n"), ["ssh-ed25519 AAAA1", "sk-ssh-ed25519@openssh.com AAAA3"]);
        assert!(parse_keys("[]").is_empty());
    }

    #[test]
    fn fork_detection() {
        let fork = r#"{"full_name":"someone/nixos","fork":true,"clone_url":"https://github.com/someone/nixos.git",
            "default_branch":"main","parent":{"full_name":"vainnor/nixos"},"source":{"full_name":"vainnor/nixos"}}"#;
        let f = parse_fork(fork, "Vainnor", "nixos").unwrap();
        assert_eq!(f, Fork { full_name: "someone/nixos".into(), url: "https://github.com/someone/nixos.git".into(), branch: "main".into() });
        // Their own repo called nixos, not a fork.
        assert_eq!(parse_fork(r#"{"full_name":"someone/nixos","fork":false}"#, "vainnor", "nixos"), None);
        // A fork of something else.
        assert_eq!(parse_fork(&fork.replace("vainnor/nixos", "other/nixos"), "vainnor", "nixos"), None);
        // A fork of a fork: source is the upstream.
        let ff = fork.replace(r#""parent":{"full_name":"vainnor/nixos"}"#, r#""parent":{"full_name":"third/nixos"}"#);
        assert!(parse_fork(&ff, "vainnor", "nixos").is_some());
        // Renamed fork found in the upstream's fork list (two pages).
        let pages = r#"[{"full_name":"a/nixos","fork":true,"owner":{"login":"a"}}]
[{"full_name":"Someone/my-nixos","fork":true,"owner":{"login":"Someone"},"clone_url":"https://github.com/Someone/my-nixos.git","default_branch":"trunk"}]"#;
        let f = find_in_forks(pages, "someone", "vainnor", "nixos").unwrap();
        assert_eq!((f.full_name.as_str(), f.branch.as_str()), ("Someone/my-nixos", "trunk"));
        assert_eq!(find_in_forks(pages, "nobody", "vainnor", "nixos"), None);
        assert_eq!(find_in_forks("", "a", "vainnor", "nixos"), None);
    }

    #[test]
    fn dry_run_owner_and_fork() {
        // One test for the shared demo state (tests run in parallel).
        let r = Runner::new(super::super::Mode::DryRun, None);
        set_demo_login("Vainnor");
        let a = account(&r, "vainnor", "nixos").unwrap();
        assert!(a.owner && a.fork.is_none());
        assert_eq!(a.email.as_deref(), Some("vainnor@example.com"));
        assert_eq!(a.keys.len(), 1);
        set_demo_login("someone");
        let a = account(&r, "vainnor", "nixos").unwrap();
        assert!(!a.owner && a.fork.is_none(), "no fork yet");
        let f = fork(&r, "vainnor", "nixos", "someone").unwrap();
        assert_eq!(f.url, "https://github.com/someone/nixos.git");
        assert_eq!(account(&r, "vainnor", "nixos").unwrap().fork, Some(f));
        set_demo_login("Vainnor");
    }
}
