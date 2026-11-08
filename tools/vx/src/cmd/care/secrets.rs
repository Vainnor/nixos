//! `vx secrets ...`: sops-encrypted secrets/secrets.yaml (system/secrets.nix).
//!
//! New machines get their secrets without the Mac: the installer adds the new
//! host to .sops.yaml; every machine that can decrypt runs `rekey --push`
//! (re-encrypts for hosts newly listed there); the new machine runs `wait`
//! (switches once it can decrypt). The services run vx as root with VX_REPO
//! and SOPS_AGE_SSH_PRIVATE_KEY_FILE set, and git/gh wrappers that drop to the
//! user (modules/system/secrets.nix), so nothing here uses sudo as root.

use crate::ctx::{self, Ctx, Host};
use crate::{sops, ui};
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FILE: &str = "secrets/secrets.yaml";
const HOST_KEY: &str = "/persistent/etc/ssh/ssh_host_ed25519_key";
const SSH_KEY_ENV: &str = "SOPS_AGE_SSH_PRIVATE_KEY_FILE";
/// Same file as install::plan::SECRETS_OK (the wait timer stops once it exists).
const SECRETS_OK: &str = "/persistent/install/secrets-ok";

#[derive(Subcommand)]
pub enum Cmd {
    /// Edit the encrypted secrets (on the Mac, which holds the editing key)
    Edit,
    /// Set one secret at a hidden prompt (not saved in shell history)
    Set {
        /// Key path, parts split by / (e.g. restic/password)
        key: String,
    },
    /// Print one decrypted secret
    Get {
        /// Key path, parts split by / (e.g. restic/password)
        key: String,
    },
    /// Print this machine's age recipient (run on a NixOS machine)
    Host,
    /// Add a machine as a recipient, re-encrypt, and turn secrets on (run on the Mac)
    AddHost {
        /// The machine's age key, from `vx secrets host`
        key: String,
        /// Its host name in the flake
        #[arg(long, default_value = ctx::DEFAULT_NIXOS_HOST)]
        name: String,
    },
    /// Pull, re-encrypt secrets.yaml for machines newly listed in .sops.yaml, commit
    Rekey {
        /// Push the commit too
        #[arg(long)]
        push: bool,
    },
    /// Exit 0 when this machine can decrypt secrets.yaml, 1 when it can't yet
    Ready,
    /// Pull; once this machine can decrypt secrets.yaml, switch to it (exit 1: not yet)
    Wait,
}

/// `wifi/psk` -> `["wifi"]["psk"]` (sops' key path syntax).
pub fn key_path(key: &str) -> Result<String> {
    if key.is_empty() || key.split('/').any(str::is_empty) {
        bail!("bad key {key:?}; use parts split by /, like wifi/psk");
    }
    Ok(key.split('/').map(|p| format!("[{}]", serde_json::Value::from(p))).collect())
}

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    let repo = ctx.repo()?;
    match cmd {
        Cmd::Edit => ctx.run(ctx.cmd("sops").arg(FILE)),
        Cmd::Get { key } => ctx.run(sops_cmd(ctx).args(["-d", "--extract", &key_path(&key)?, FILE])),
        Cmd::Set { key } => {
            let path = key_path(&key)?;
            let v = read_hidden(&format!("{key}: "))?;
            if v.is_empty() {
                bail!("empty, nothing changed");
            }
            let mut c = ctx.cmd("sops");
            c.args(["set", "--value-stdin", FILE, &path]).stdin(Stdio::piped());
            if ctx.verbose {
                ui::dim(&format!("$ sops set --value-stdin {FILE} {path}"));
            }
            let mut child = c.spawn().context("could not start sops")?;
            let json = serde_json::Value::from(v).to_string();
            child.stdin.take().context("no stdin for sops")?.write_all(format!("{json}\n").as_bytes())?;
            let st = child.wait()?;
            if !st.success() {
                bail!("sops set failed ({st}); is this the Mac with the editing key?");
            }
            ui::ok(&format!("set {key}"));
            Ok(())
        }
        Cmd::Host => {
            ctx.need(Host::NixOs)?;
            println!("{}", host_age(ctx)?);
            Ok(())
        }
        Cmd::AddHost { key, name } => {
            if !sops::is_age_key(&key) {
                bail!("that isn't an age key (they start with age1); get it on the machine with: vx secrets host");
            }
            let f = repo.join(".sops.yaml");
            let before = std::fs::read_to_string(&f).context("can't read .sops.yaml")?;
            let after = sops::set_host(&before, &name, &key, &name)?;
            if after != before {
                std::fs::write(&f, after)?;
            }
            ctx.run(sops_cmd(ctx).args(["updatekeys", "--yes", FILE]))?;
            let meta = repo.join("modules/flake/meta.nix");
            let m = std::fs::read_to_string(&meta)?;
            std::fs::write(&meta, m.replace("secretsReady = false;", "secretsReady = true;"))?;
            ui::ok("done; review with git diff, then commit and deploy");
            Ok(())
        }
        Cmd::Rekey { push } => rekey(ctx, push),
        Cmd::Ready => {
            let (ok, why) = ready(ctx)?;
            if ok {
                ui::ok(&why);
                Ok(())
            } else {
                ui::info(&why);
                std::process::exit(1);
            }
        }
        Cmd::Wait => {
            if wait(ctx)? {
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
    }
}

// ---------------------------------------------------------------- who decrypts

fn is_root() -> bool {
    Command::new("id").arg("-u").output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
}

/// The age key file sops should use on the Mac (env first, then the usual places).
pub fn mac_age_key() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SOPS_AGE_KEY_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let mut places = vec![];
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        places.push(PathBuf::from(x).join("sops/age/keys.txt"));
    }
    places.push(home.join(".config/sops/age/keys.txt"));
    places.push(home.join("Library/Application Support/sops/age/keys.txt"));
    places.into_iter().find(|p| p.is_file())
}

/// This machine can decrypt as itself: an age key (Mac), or the host key given
/// in the env, or root (who can read the host key).
fn has_key(ctx: &Ctx) -> bool {
    match ctx.host {
        Host::Mac => std::env::var_os("SOPS_AGE_KEY").is_some() || mac_age_key().is_some(),
        Host::NixOs => std::env::var_os(SSH_KEY_ENV).is_some() || is_root(),
    }
}

/// `sops` with the key it needs: the Mac's age key file; on NixOS the host key
/// (as given in the env, else read as root, through sudo when not root).
fn sops_cmd(ctx: &Ctx) -> Command {
    match ctx.host {
        Host::Mac => {
            let mut c = ctx.cmd("sops");
            if std::env::var_os("SOPS_AGE_KEY_FILE").is_none()
                && let Some(k) = mac_age_key()
            {
                c.env("SOPS_AGE_KEY_FILE", k);
            }
            c
        }
        Host::NixOs if std::env::var_os(SSH_KEY_ENV).is_some() => ctx.cmd("sops"),
        Host::NixOs if is_root() => {
            let mut c = ctx.cmd("sops");
            c.env(SSH_KEY_ENV, HOST_KEY);
            c
        }
        Host::NixOs => {
            let mut c = ctx.sudo("env");
            c.arg(format!("{SSH_KEY_ENV}={HOST_KEY}")).arg("sops");
            c
        }
    }
}

/// This machine's age recipient, from its SSH host key.
fn host_age(ctx: &Ctx) -> Result<String> {
    let key = std::env::var(SSH_KEY_ENV).unwrap_or_else(|_| HOST_KEY.to_string());
    let pub_ = format!("{key}.pub");
    let out = if ctx::has("ssh-to-age") {
        ctx.output(Command::new("ssh-to-age").args(["-i", &pub_]))?
    } else {
        ctx.output(ctx.nix().args(["run", "nixpkgs#ssh-to-age", "--", "-i", &pub_]))?
    };
    let age = out.trim().to_string();
    if !sops::is_age_key(&age) {
        bail!("ssh-to-age printed no age key for {pub_}");
    }
    Ok(age)
}

/// Can this machine decrypt secrets.yaml? (answer, a short sentence)
pub fn ready(ctx: &Ctx) -> Result<(bool, String)> {
    let repo = ctx.repo()?;
    let file = repo.join(FILE);
    let text = std::fs::read_to_string(&file).with_context(|| format!("can't read {}", file.display()))?;
    if has_key(ctx) {
        let ok = sops_cmd(ctx).args(["-d", FILE]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        return Ok(if ok {
            (true, "this machine can decrypt secrets.yaml".into())
        } else {
            (false, "this machine can't decrypt secrets.yaml yet (not re-encrypted for it)".into())
        });
    }
    // A NixOS user without the key: is this host a recipient?
    let age = host_age(ctx)?;
    Ok(if sops::file_recipients(&text).contains(&age) {
        (true, "this machine is a recipient of secrets.yaml".into())
    } else {
        (false, "this machine isn't a recipient of secrets.yaml yet".into())
    })
}

// ---------------------------------------------------------------- rekey / wait

/// Network trouble, as git tells it.
pub fn offline_text(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    [
        "could not resolve host",
        "unable to access",
        "failed to connect",
        "network is unreachable",
        "connection timed out",
        "temporary failure in name resolution",
        "could not read from remote repository",
    ]
    .iter()
    .any(|p| s.contains(p))
}

enum Net {
    Ok,
    Offline,
}

/// `git <args>` in the repo; Offline when it failed for lack of network.
fn git_net(ctx: &Ctx, args: &[&str]) -> Result<Net> {
    let mut c = ctx.cmd("git");
    c.args(args);
    if ctx.verbose {
        ui::dim(&format!("$ git {}", args.join(" ")));
    }
    let out = c.output().context("could not start git")?;
    if out.status.success() {
        return Ok(Net::Ok);
    }
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    if offline_text(&err) {
        return Ok(Net::Offline);
    }
    bail!("git {} failed: {}", args.join(" "), err.trim());
}

/// The commit message for re-encrypting for these hosts.
pub fn rekey_message(hosts: &[String]) -> String {
    format!("chore(secrets): re-encrypt for {}", hosts.join(", "))
}

/// Pull; when .sops.yaml lists recipients secrets.yaml lacks, `sops updatekeys`
/// and commit; with `push`, push that commit (and earlier ones of ours).
pub fn rekey(ctx: &Ctx, push: bool) -> Result<()> {
    let repo = ctx.repo()?;
    if let Net::Offline = git_net(ctx, &["pull", "--ff-only", "-q"])? {
        ui::info("offline, will retry");
        return Ok(());
    }
    let sops_yaml = std::fs::read_to_string(repo.join(".sops.yaml")).context("can't read .sops.yaml")?;
    let secrets = std::fs::read_to_string(repo.join(FILE)).with_context(|| format!("can't read {FILE}"))?;
    let missing = sops::missing(&sops_yaml, &secrets);
    if missing.is_empty() {
        ui::ok("nothing to re-encrypt: secrets.yaml is encrypted for everyone in .sops.yaml");
    } else {
        let hosts: Vec<String> = missing.iter().map(|(n, _)| n.clone()).collect();
        ui::info(&format!("re-encrypting secrets.yaml for {}", hosts.join(", ")));
        ctx.run(sops_cmd(ctx).args(["updatekeys", "-y", FILE]))?;
        let unchanged = ctx.cmd("git").args(["diff", "--quiet", "--", FILE]).status().is_ok_and(|s| s.success());
        if unchanged {
            ui::warn("sops changed nothing: check .sops.yaml");
            return Ok(());
        }
        ctx.run(ctx.cmd("git").args(["commit", "-q", "-m", &rekey_message(&hosts), "--", FILE]))?;
        ui::ok(&rekey_message(&hosts));
    }
    if push {
        push_ours(ctx)?;
    }
    Ok(())
}

/// Push when every commit ahead of upstream is one of rekey's (never your own work).
fn push_ours(ctx: &Ctx) -> Result<()> {
    let Some(log) = ctx.probe(ctx.cmd("git").args(["log", "--format=%s", "@{u}..HEAD"])) else { return Ok(()) };
    let ahead: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    if ahead.is_empty() {
        return Ok(());
    }
    if !ahead.iter().all(|s| s.starts_with("chore(secrets): re-encrypt")) {
        ui::warn("not pushing: other commits are waiting to be pushed too (push them yourself)");
        return Ok(());
    }
    match git_net(ctx, &["push", "-q"])? {
        Net::Ok => ui::ok("pushed"),
        Net::Offline => ui::info("offline, will retry"),
    }
    Ok(())
}

/// Pull; once this machine can decrypt, switch to the flake (as root:
/// nixos-rebuild) and note it in SECRETS_OK. Ok(false): not yet.
pub fn wait(ctx: &Ctx) -> Result<bool> {
    let repo = ctx.repo()?;
    if let Net::Offline = git_net(ctx, &["pull", "--ff-only", "-q"])? {
        ui::info("offline, will retry");
        return Ok(false);
    }
    let (ok, why) = ready(ctx)?;
    if !ok {
        ui::info(&why);
        return Ok(false);
    }
    let target = format!("{}#{}", repo.display(), ctx.nixos_host());
    let mut c = if is_root() { ctx.cmd("nixos-rebuild") } else { ctx.sudo("nixos-rebuild") };
    ctx.run(c.args(["switch", "--flake", &target]))?;
    if is_root() {
        let ok = Path::new(SECRETS_OK);
        if let Some(d) = ok.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(ok, b"");
    }
    ui::ok("switched with the secrets");
    Ok(true)
}

/// Read a line with echo off (`stty -echo` on the terminal).
fn read_hidden(prompt: &str) -> Result<String> {
    let tty = || std::fs::File::open("/dev/tty");
    print!("{prompt}");
    std::io::stdout().flush()?;
    let hid = tty()
        .ok()
        .and_then(|t| Command::new("stty").arg("-echo").stdin(t).status().ok())
        .is_some_and(|s| s.success());
    let mut s = String::new();
    let r = std::io::stdin().read_line(&mut s);
    if hid && let Ok(t) = tty() {
        let _ = Command::new("stty").arg("echo").stdin(t).status();
    }
    println!();
    r?;
    Ok(s.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_paths() {
        assert_eq!(key_path("wifi/psk").unwrap(), r#"["wifi"]["psk"]"#);
        assert_eq!(key_path(r#"a"b"#).unwrap(), r#"["a\"b"]"#);
        assert!(key_path("a//b").is_err());
        assert!(key_path("").is_err());
    }

    #[test]
    fn rekey_commit_message() {
        let m = rekey_message(&["forge".into(), "arc".into()]);
        assert_eq!(m, "chore(secrets): re-encrypt for forge, arc");
        assert!(!m.contains("Co-Authored"));
    }

    #[test]
    fn offline_or_not() {
        assert!(offline_text("fatal: unable to access 'https://github.com/vainnor/nixos/': Could not resolve host: github.com"));
        assert!(!offline_text("fatal: Not possible to fast-forward, aborting."));
    }

    #[test]
    fn sops_gets_the_key_without_argv() {
        // The key's path may be in argv (sudo env), never a key itself.
        let ctx = Ctx::new(Some(PathBuf::from("/r")), false).unwrap();
        let c = sops_cmd(&ctx);
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.iter().all(|a| !a.contains("AGE-SECRET-KEY")));
    }
}
