//! Restoring /home from the homelab's restic backup (system/backups.nix) onto
//! a fresh /home, before the first boot. Only /home/<user>: what /persistent
//! needs comes from the carry-over.
//!
//! The login (the repository, its password, the rest-server's user and
//! password) comes from the first of these that works ([`sources`]):
//!   1. secrets.yaml, when the carried-over host key is one of its recipients
//!      (sops with SOPS_AGE_SSH_PRIVATE_KEY_FILE, like `vx secrets get`);
//!   2. the 1Password item meta.restoreItem, fields repository, password,
//!      rest_username and rest_password, read with `op` after signing in on
//!      the installer (address, email, secret key, password; its config in
//!      [`OP_CONFIG`], removed once read);
//!   3. typed on the screen.
//!
//! It stays in memory. restic gets the repository and password as files in
//! [`RESTIC_WORK`] (RAM, 0600, removed right after) and the rest-server login
//! in its environment; none of it is logged. TLS: the repo's pinned
//! certificate, assets/restic/homelab-ca.pem.
//!
//! The server is only reachable over NetBird, so the usb is logged in to it
//! (the netbird screen) and stays up until this step is done; `netbird down`
//! comes later, in restore. A restore that stops is run again (restic skips
//! what's already there, `--overwrite if-changed`) and never fails the
//! install: `vx restore --restic` gets the files later.

use super::super::plan::{CredSource, RESTIC_CA_REL, RESTIC_WORK, OP_CONFIG, ResticCreds, Secret};
use super::runner::{Cmd, Runner};
use anyhow::{Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};

const SECRETS_REL: &str = "secrets/secrets.yaml";
const SSH_KEY_ENV: &str = "SOPS_AGE_SSH_PRIVATE_KEY_FILE";
/// `op account add --shorthand`: the account's name in [`OP_CONFIG`].
pub const OP_ACCOUNT: &str = "vx";
/// The 1Password item's fields, in [`ResticCreds`] order.
pub const OP_FIELDS: [&str; 4] = ["repository", "password", "rest_username", "rest_password"];
pub const OP_ADDRESS: &str = "my.1password.com";
/// Tries of the restore before it's left for later.
pub const ATTEMPTS: u32 = 3;
/// restic's JSON progress every 5 s (not a terminal: it would wait a minute).
const PROGRESS_FPS: &str = "0.2";
const DRY_AGE: &str = "age1dryrunxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

/// Where to get the login, in order: the secrets when the carried host key
/// can read them, then 1Password when an item is set, then typing it.
pub fn sources(sops_key: bool, op_item: Option<&str>) -> Vec<CredSource> {
    let mut v = vec![];
    if sops_key {
        v.push(CredSource::Sops);
    }
    if op_item.is_some_and(|i| !i.trim().is_empty()) {
        v.push(CredSource::OnePassword);
    }
    v.push(CredSource::Typed);
    v
}

/// The pinned certificate in the repo, when it's there (DryRun: assumed).
pub fn ca(r: &Runner, repo: &Path) -> Option<PathBuf> {
    let p = repo.join(RESTIC_CA_REL);
    r.exists(&p, true).then_some(p)
}

// ---------------------------------------------------------------- 1. sops

/// Is `key`'s age recipient one of secrets.yaml's? (The check
/// `vx secrets ready` does for a host without the key.)
pub fn key_decrypts(r: &Runner, repo: &Path, key: &Path) -> Result<bool> {
    let mut pub_key = key.as_os_str().to_owned();
    pub_key.push(".pub");
    let pubkey = r.read(Path::new(&pub_key), "ssh-ed25519 AAAAdryrun root@arc")?;
    let age = r.run(Cmd::new("ssh-to-age").stdin(pubkey.into_bytes()).fake(DRY_AGE))?.trim().to_string();
    let text = r.read(&repo.join(SECRETS_REL), &format!("sops:\n    age:\n        - recipient: {DRY_AGE}\n"))?;
    Ok(crate::sops::file_recipients(&text).contains(&age))
}

/// `sops -d --extract '["restic"]["x"]' secrets.yaml` as `key` (output not logged).
pub fn sops_get_cmd(repo: &Path, key: &Path, path: &str) -> Result<Cmd> {
    Ok(Cmd::new("sops")
        .env(SSH_KEY_ENV, &key.to_string_lossy())
        .args(["-d", "--extract", &crate::cmd::care::secrets::key_path(path)?])
        .arg(repo.join(SECRETS_REL).to_string_lossy())
        .quiet())
}

pub fn from_sops(r: &Runner, repo: &Path, key: &Path) -> Result<ResticCreds> {
    if !key_decrypts(r, repo, key)? {
        bail!("the carried host key isn't a recipient of secrets.yaml");
    }
    let get = |path: &str, fake: &str| -> Result<String> {
        r.run(sops_get_cmd(repo, key, path)?.fake(fake))
            .map_err(|e| anyhow::anyhow!("no {path} in secrets.yaml ({})", first_line(&format!("{e:#}"))))
    };
    let repository = get("restic/repository", "rest:https://restic.dryrun:8000/arc/")?.trim().to_string();
    let password = get("restic/password", "dry-run-repo-password")?;
    let env = get("restic/env", "RESTIC_REST_USERNAME=arc\nRESTIC_REST_PASSWORD=dry-run\nRESTIC_CACERT=/etc/restic/homelab-ca.pem\n")?;
    creds(repository, password.trim_end_matches('\n').to_string(), parse_env(&env))
}

/// `KEY=value` lines of restic/env (an EnvironmentFile). RESTIC_CACERT is
/// dropped (that path is the installed system's; the repo's cert is used) and
/// so are the repository and password ones (they go in as files).
pub fn parse_env(text: &str) -> Vec<(String, Secret)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("export ").unwrap_or(l).split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string()))
        .filter(|(k, _)| {
            k.chars().next().is_some_and(|c| c.is_ascii_uppercase() || c == '_')
                && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && k != "RESTIC_CACERT"
                && !k.starts_with("RESTIC_REPOSITORY")
                && !k.starts_with("RESTIC_PASSWORD")
        })
        .map(|(k, v)| (k, Secret(v)))
        .collect()
}

// ---------------------------------------------------------------- 2. 1password

/// The account to sign in to (typed on the screen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpLogin {
    pub address: String,
    pub email: String,
    pub secret_key: Secret,
    pub password: Secret,
}

fn op() -> Cmd {
    Cmd::new("op").env("OP_CONFIG_DIR", OP_CONFIG).env("OP_BIOMETRIC_UNLOCK_ENABLED", "false")
}

/// `op account add ... --signin --raw`: prints a session token. The password
/// goes in on stdin; the secret key has no other way in than argv (`***` in the log).
pub fn op_signin_cmd(l: &OpLogin) -> Cmd {
    op().args(["account", "add", "--address", l.address.trim(), "--email", l.email.trim(), "--secret-key"])
        .secret_arg(l.secret_key.0.trim())
        .args(["--shorthand", OP_ACCOUNT, "--signin", "--raw"])
        .stdin(format!("{}\n", l.password.0).into_bytes())
        .quiet()
        .fake("dry-run-session")
}

/// `op read op://<vault>/<item>/<field>` with the session (`***` in the log).
pub fn op_read_cmd(session: &Secret, item: &str, field: &str) -> Cmd {
    op().args(["read", "--no-newline", "--account", OP_ACCOUNT, "--session"])
        .secret_arg(&session.0)
        .arg(format!("{}/{field}", item.trim().trim_end_matches('/')))
        .quiet()
}

/// `op://<vault>/<item>`, nothing more.
pub fn check_item(item: &str) -> Result<()> {
    let rest = item.trim().strip_prefix("op://").unwrap_or("");
    let parts: Vec<&str> = rest.trim_end_matches('/').split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.trim().is_empty()) {
        bail!("meta.restoreItem should look like op://<vault>/<item>, not {item:?}");
    }
    Ok(())
}

/// Sign in, read the item's four fields, forget the sign-in.
pub fn from_op(r: &Runner, l: &OpLogin, item: &str) -> Result<ResticCreds> {
    check_item(item)?;
    if l.email.trim().is_empty() || l.secret_key.0.trim().is_empty() || l.password.0.is_empty() {
        bail!("type the email, secret key and password of your 1password account");
    }
    let dir = Path::new(OP_CONFIG);
    r.remove(dir)?;
    r.mkdir(dir, 0o700)?;
    let res = (|| -> Result<ResticCreds> {
        let session = r
            .run(op_signin_cmd(l))
            .map_err(|e| anyhow::anyhow!("1password sign-in failed: {}", last_line(&format!("{e:#}"))))?;
        let session = Secret(session.trim().to_string());
        if session.0.is_empty() {
            bail!("1password sign-in printed no session");
        }
        let mut got = vec![];
        for (f, fake) in OP_FIELDS.iter().zip(["rest:https://restic.dryrun:8000/arc/", "dry-run-repo-password", "arc", "dry-run"]) {
            let v = r
                .run(op_read_cmd(&session, item, f).fake(fake))
                .map_err(|e| anyhow::anyhow!("couldn't read {f} from {item}: {}", last_line(&format!("{e:#}"))))?;
            got.push(v);
        }
        let [repository, password, user, pass] = <[String; 4]>::try_from(got).expect("four fields");
        creds(repository.trim().to_string(), password, rest_env(&user, &pass))
    })();
    let _ = r.remove(dir);
    res
}

// ---------------------------------------------------------------- 3. typed

pub fn typed(repository: &str, password: &str, user: &str, pass: &str) -> Result<ResticCreds> {
    creds(repository.trim().to_string(), password.to_string(), rest_env(user, pass))
}

fn rest_env(user: &str, pass: &str) -> Vec<(String, Secret)> {
    let mut env = vec![];
    if !user.trim().is_empty() {
        env.push(("RESTIC_REST_USERNAME".to_string(), Secret(user.trim().to_string())));
    }
    if !pass.is_empty() {
        env.push(("RESTIC_REST_PASSWORD".to_string(), Secret(pass.to_string())));
    }
    env
}

fn creds(repository: String, password: String, env: Vec<(String, Secret)>) -> Result<ResticCreds> {
    if repository.is_empty() {
        bail!("no repository (like rest:https://<server>:8000/<host>/)");
    }
    if password.is_empty() {
        bail!("no repository password");
    }
    Ok(ResticCreds { repository: Secret(repository), password: Secret(password), env })
}

// ---------------------------------------------------------------- restic

pub fn repository_file() -> PathBuf {
    Path::new(RESTIC_WORK).join("repository")
}
pub fn password_file() -> PathBuf {
    Path::new(RESTIC_WORK).join("password")
}

/// The repository and its password as files restic reads (RAM, 0600).
pub fn write_creds(r: &Runner, c: &ResticCreds) -> Result<()> {
    r.mkdir(Path::new(RESTIC_WORK), 0o700)?;
    r.write_secret(&repository_file(), c.repository.0.as_bytes(), 0o600)?;
    r.write_secret(&password_file(), c.password.0.as_bytes(), 0o600)
}

pub fn remove_creds(r: &Runner) {
    let _ = r.remove(Path::new(RESTIC_WORK));
}

/// `restic` with the login: files for the repository and password, the
/// rest-server login in the environment, the pinned certificate.
pub fn restic(c: &ResticCreds, ca: Option<&Path>) -> Cmd {
    let mut cmd = Cmd::new("restic")
        .arg("--repository-file")
        .arg(repository_file().to_string_lossy())
        .arg("--password-file")
        .arg(password_file().to_string_lossy());
    if let Some(ca) = ca {
        cmd = cmd.arg("--cacert").arg(ca.to_string_lossy());
    }
    for (k, v) in &c.env {
        cmd = cmd.env(k, &v.0);
    }
    cmd.env("RESTIC_PROGRESS_FPS", PROGRESS_FPS)
}

pub fn snapshots_cmd(c: &ResticCreds, ca: Option<&Path>) -> Cmd {
    restic(c, ca).args(["snapshots", "--latest", "1", "--json"]).fake(
        r#"[{"time":"2026-11-09T03:41:12.5-06:00","paths":["/home","/persistent"],"hostname":"arc","short_id":"4f2a91c0"}]"#,
    )
}

/// The latest snapshot's /home/<user> into /mnt/home/<user>, owners, modes
/// and xattrs as they were. Run again, it only fetches what isn't there yet.
pub fn restore_cmd(c: &ResticCreds, ca: Option<&Path>, user: &str) -> Cmd {
    restic(c, ca)
        .args(["--json", "restore", "latest", "--target", "/mnt", "--include"])
        .arg(format!("/home/{user}"))
        .args(["--overwrite", "if-changed"])
        .format(progress_line)
}

/// Can the backup be read? The latest snapshot, described ("2026-11-09 03:41 from arc").
pub fn check(r: &Runner, c: &ResticCreds, repo: &Path) -> Result<String> {
    let ca = ca(r, repo);
    write_creds(r, c)?;
    let res = r.output(&snapshots_cmd(c, ca.as_deref()));
    remove_creds(r);
    let (ok, out, tail) = res?;
    if !ok {
        let mut why = explain(&tail);
        if ca.is_none() {
            why.push_str(&format!(" ({RESTIC_CA_REL} isn't in the repo, so the server's certificate can't be checked)"));
        }
        bail!(why);
    }
    parse_latest(&out).ok_or_else(|| anyhow::anyhow!("the backup has no snapshots yet"))
}

/// The newest snapshot in `restic snapshots --json`.
pub fn parse_latest(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json.trim()).ok()?;
    let s = v.as_array()?.iter().max_by_key(|s| s.get("time").and_then(Value::as_str).unwrap_or(""))?;
    let time = s.get("time")?.as_str()?;
    let when = if time.len() >= 16 { format!("{} {}", &time[..10], &time[11..16]) } else { time.to_string() };
    Some(match s.get("hostname").and_then(Value::as_str) {
        Some(h) if !h.is_empty() => format!("{when} from {h}"),
        _ => when,
    })
}

/// restic's error, in a sentence that says what to do.
pub fn explain(tail: &str) -> String {
    let l = tail.to_lowercase();
    let hint = if l.contains("wrong password") || l.contains("no key found") {
        "wrong repository password"
    } else if l.contains("401") || l.contains("unauthorized") {
        "the rest-server refused the login (rest_username / rest_password)"
    } else if l.contains("x509") || l.contains("certificate") {
        "the server's certificate doesn't match the pinned one"
    } else if l.contains("no such host")
        || l.contains("connection refused")
        || l.contains("network is unreachable")
        || l.contains("i/o timeout")
        || l.contains("timed out")
        || l.contains("no route to host")
    {
        "can't reach the backup server: it's only reachable over netbird; is this usb logged in?"
    } else if l.contains("path") && l.contains("not found") || l.contains("no matching") {
        "that folder isn't in the latest snapshot"
    } else {
        ""
    };
    let last = last_line(tail);
    if hint.is_empty() { last } else { format!("{hint} ({last})") }
}

/// restic's `--json` restore output, one log line per update.
pub fn progress_line(line: &str) -> Option<String> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return (!line.trim().is_empty()).then(|| line.to_string());
    };
    let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    match v.get("message_type").and_then(Value::as_str) {
        Some("status") => {
            let pct = v.get("percent_done").and_then(Value::as_f64).unwrap_or(0.0) * 100.0;
            Some(format!(
                "restored {} of {} ({pct:.0}%), {} of {} files",
                gb(n("bytes_restored") + n("bytes_skipped")),
                gb(n("total_bytes")),
                n("files_restored") + n("files_skipped"),
                n("total_files")
            ))
        }
        Some("summary") => Some(format!(
            "restored {}, {} files ({} were already there)",
            gb(n("total_bytes")),
            n("total_files"),
            n("files_skipped")
        )),
        _ => None,
    }
}

/// Run `f` up to `attempts` times; between tries `wait(n, error)` is called.
/// The last error when every try failed.
pub fn retry(attempts: u32, mut f: impl FnMut(u32) -> Result<()>, mut wait: impl FnMut(u32, &str)) -> Option<String> {
    let mut last = None;
    for n in 1..=attempts {
        match f(n) {
            Ok(()) => return None,
            Err(e) => {
                let msg = explain(&format!("{e:#}"));
                if n < attempts {
                    wait(n, &msg);
                }
                last = Some(msg);
            }
        }
    }
    last
}

fn gb(b: u64) -> String {
    if b >= 1_000_000_000 {
        format!("{:.1} GB", b as f64 / 1e9)
    } else {
        format!("{:.0} MB", b as f64 / 1e6)
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).to_string()
}

fn last_line(s: &str) -> String {
    s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::super::Mode;
    use super::*;
    use std::sync::mpsc;

    fn creds_ab() -> ResticCreds {
        typed("rest:https://restic-1.example:8000/arc/", "repo-pw-Zq", "arc", "rest-pw-Xk").unwrap()
    }

    #[test]
    fn source_order() {
        use CredSource::*;
        assert_eq!(sources(true, Some("op://Private/arc restic")), [Sops, OnePassword, Typed]);
        assert_eq!(sources(false, Some("op://Private/arc restic")), [OnePassword, Typed]);
        assert_eq!(sources(true, None), [Sops, Typed]);
        assert_eq!(sources(false, Some("  ")), [Typed], "an empty item is off");
        assert_eq!(sources(false, None), [Typed], "typing always works");
    }

    #[test]
    fn env_file() {
        let e = parse_env(
            "# restic/env\nRESTIC_REST_USERNAME=arc\nexport RESTIC_REST_PASSWORD=\"p=w\"\n\nRESTIC_CACERT=/etc/restic/homelab-ca.pem\nRESTIC_PASSWORD=x\nlower=no\n",
        );
        let kv: Vec<(&str, &str)> = e.iter().map(|(k, v)| (k.as_str(), v.0.as_str())).collect();
        assert_eq!(kv, [("RESTIC_REST_USERNAME", "arc"), ("RESTIC_REST_PASSWORD", "p=w")]);
    }

    #[test]
    fn restic_commands() {
        let c = creds_ab();
        let ca = PathBuf::from("/tmp/vx-install/nixos/assets/restic/homelab-ca.pem");
        let s = snapshots_cmd(&c, Some(&ca));
        assert_eq!(
            s.line(),
            "restic --repository-file /run/vx-install/restic/repository --password-file /run/vx-install/restic/password \
             --cacert /tmp/vx-install/nixos/assets/restic/homelab-ca.pem snapshots --latest 1 --json"
        );
        let r = restore_cmd(&c, Some(&ca), "vainnor");
        assert!(
            r.line().ends_with(" --json restore latest --target /mnt --include /home/vainnor --overwrite if-changed"),
            "{}",
            r.line()
        );
        assert_eq!(restore_cmd(&c, None, "carson").line().matches("--cacert").count(), 0);
        // The login is in the environment, never in argv or the line.
        for cmd in [&s, &r] {
            assert!(cmd.env.contains(&("RESTIC_REST_PASSWORD".into(), "rest-pw-Xk".into())));
            assert!(cmd.env.contains(&("RESTIC_PROGRESS_FPS".into(), "0.2".into())));
            assert!(cmd.args.iter().all(|a| !a.contains("repo-pw") && !a.contains("rest-pw") && !a.contains("restic-1")));
        }
    }

    #[test]
    fn op_commands_hide_secrets() {
        let l = OpLogin {
            address: OP_ADDRESS.into(),
            email: "me@example.com".into(),
            secret_key: Secret("A3-SECRETKEY".into()),
            password: Secret("master-pw".into()),
        };
        let c = op_signin_cmd(&l);
        assert_eq!(
            c.line(),
            "op account add --address my.1password.com --email me@example.com --secret-key *** --shorthand vx --signin --raw < (stdin)"
        );
        assert_eq!(c.stdin_bytes(), Some("master-pw\n".as_bytes()));
        assert!(c.args.iter().all(|a| !a.contains("master-pw")));
        assert!(c.env.contains(&("OP_CONFIG_DIR".into(), "/run/vx-install/op".into())));
        let c = op_read_cmd(&Secret("tok-123".into()), "op://Private/arc restic/", "rest_password");
        assert_eq!(c.line(), "op read --no-newline --account vx --session *** 'op://Private/arc restic/rest_password'");
        assert!(check_item("op://Private/arc restic").is_ok());
        assert!(check_item("Private/arc restic").is_err());
        assert!(check_item("op://Private/arc restic/password").is_err(), "the item, not a field");
    }

    #[test]
    fn sops_commands() {
        let c = sops_get_cmd(Path::new("/tmp/vx-install/nixos"), Path::new("/run/vx-install/old/persistent/etc/ssh/k"), "restic/env").unwrap();
        assert_eq!(c.line(), r#"sops -d --extract '["restic"]["env"]' /tmp/vx-install/nixos/secrets/secrets.yaml"#);
        assert!(c.env.contains(&(SSH_KEY_ENV.into(), "/run/vx-install/old/persistent/etc/ssh/k".into())));
    }

    fn dry<T>(f: impl FnOnce(&Runner) -> Result<T>) -> (Result<T>, Vec<String>) {
        let (tx, rx) = mpsc::channel();
        let r = Runner::new(Mode::DryRun, Some(tx));
        let res = f(&r);
        drop(r);
        let logs = rx.try_iter().filter_map(|e| if let super::super::super::plan::Event::Log(l) = e { Some(l) } else { None }).collect();
        (res, logs)
    }

    #[test]
    fn dry_sources() {
        let repo = Path::new("/nonexistent/vx-install/nixos");
        let key = Path::new("/run/vx-install/old/persistent/etc/ssh/ssh_host_ed25519_key");
        let (c, logs) = dry(|r| from_sops(r, repo, key));
        let c = c.unwrap();
        assert_eq!(c.repository.0, "rest:https://restic.dryrun:8000/arc/");
        assert_eq!(c.env.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["RESTIC_REST_USERNAME", "RESTIC_REST_PASSWORD"]);
        assert!(logs.iter().any(|l| l.starts_with("$ sops -d --extract")));
        assert!(logs.iter().all(|l| !l.contains("dry-run-repo-password")), "{logs:?}");

        let l = OpLogin { address: OP_ADDRESS.into(), email: "me@example.com".into(), secret_key: Secret("A3-K".into()), password: Secret("pw".into()) };
        let (c, logs) = dry(|r| from_op(r, &l, "op://Private/arc restic"));
        assert_eq!(c.unwrap().env[0].1.0, "arc");
        let cmds: Vec<&String> = logs.iter().filter(|l| l.starts_with("$ ")).collect();
        assert_eq!(cmds.len(), 5, "{cmds:?}");
        assert!(logs.contains(&"mkdir /run/vx-install/op (700)".to_string()));
        assert_eq!(logs.last().unwrap(), "remove /run/vx-install/op", "the sign-in is forgotten");
        assert!(logs.iter().all(|l| !l.contains("A3-K") && !l.contains("dry-run-session")));
        let bad = OpLogin { email: String::new(), ..l };
        assert!(dry(|r| from_op(r, &bad, "op://Private/arc restic")).0.is_err());

        let (latest, logs) = dry(|r| check(r, &creds_ab(), repo));
        assert_eq!(latest.unwrap(), "2026-11-09 03:41 from arc");
        assert!(logs.contains(&"write /run/vx-install/restic/password (secret, 600)".to_string()));
        assert_eq!(logs.last().unwrap(), "remove /run/vx-install/restic");
        assert!(logs.iter().all(|l| !l.contains("repo-pw") && !l.contains("rest-pw")));
    }

    #[test]
    fn typed_needs_the_repository_and_password() {
        assert!(typed("", "pw", "", "").unwrap_err().to_string().contains("repository"));
        assert!(typed("rest:x", "", "", "").unwrap_err().to_string().contains("password"));
        assert!(typed("rest:x", "pw", "", "").unwrap().env.is_empty(), "no rest-server login: none set");
    }

    #[test]
    fn snapshots_and_errors() {
        let j = r#"[{"time":"2026-11-08T03:40:00Z","hostname":"arc"},{"time":"2026-11-09T03:41:12.5-06:00","hostname":"arc"}]"#;
        assert_eq!(parse_latest(j).as_deref(), Some("2026-11-09 03:41 from arc"));
        assert_eq!(parse_latest("[]"), None);
        assert_eq!(parse_latest("Fatal: x"), None);
        assert!(explain("Fatal: wrong password or no key found").starts_with("wrong repository password"));
        assert!(explain("Fatal: unable to open config file: Get \"https://x\": dial tcp: lookup x: no such host").contains("netbird"));
        assert!(explain("unexpected HTTP response (401): 401 Unauthorized").contains("rest_username"));
        assert!(explain("x509: certificate signed by unknown authority").contains("certificate"));
        assert_eq!(explain("something else\n"), "something else");
    }

    #[test]
    fn progress_lines() {
        let s = r#"{"message_type":"status","seconds_elapsed":12,"percent_done":0.25,"total_files":9000,"files_restored":2000,"files_skipped":250,"total_bytes":80000000000,"bytes_restored":19000000000,"bytes_skipped":1000000000}"#;
        assert_eq!(progress_line(s).unwrap(), "restored 20.0 GB of 80.0 GB (25%), 2250 of 9000 files");
        let s = r#"{"message_type":"summary","total_files":9000,"files_restored":8000,"files_skipped":1000,"total_bytes":80000000000}"#;
        assert_eq!(progress_line(s).unwrap(), "restored 80.0 GB, 9000 files (1000 were already there)");
        assert_eq!(progress_line(r#"{"message_type":"verbose_status"}"#), None);
        assert_eq!(progress_line("plain text").as_deref(), Some("plain text"));
    }

    #[test]
    fn retries_then_gives_up() {
        let mut waits = vec![];
        let mut tries = 0;
        let r = retry(3, |n| { tries = n; if n < 3 { bail!("dial tcp: i/o timeout") } else { Ok(()) } }, |n, _| waits.push(n));
        assert_eq!((r, tries, waits.clone()), (None, 3, vec![1, 2]), "the third try worked");
        waits.clear();
        let r = retry(3, |_| bail!("dial tcp: i/o timeout"), |n, _| waits.push(n));
        assert!(r.unwrap().contains("netbird"));
        assert_eq!(waits, [1, 2], "no wait after the last try");
    }
}
