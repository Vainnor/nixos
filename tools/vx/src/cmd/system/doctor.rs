//! `vx doctor`: deeper checks than `vx status`, each with the fix to try.
//! Slower (it evaluates the flake and talks to the network); `--full` adds
//! the whole `nix flake check` and a real signing test (1Password asks).

use super::util::{binary_cache, meta_bool, meta_src, meta_str, run_timeout};
use crate::ctx::{self, Ctx, Host, MAC_HOST, NIXOS_HOST};
use crate::ui;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Default)]
struct Report {
    fails: u32,
    warns: u32,
}

impl Report {
    fn ok(&mut self, s: &str) {
        ui::ok(s);
    }
    fn info(&mut self, s: &str) {
        ui::info(s);
    }
    fn warn(&mut self, s: &str, fix: &str) {
        self.warns += 1;
        ui::warn(s);
        hint(fix);
    }
    fn fail(&mut self, s: &str, fix: &str) {
        self.fails += 1;
        // Same stream as ok/warn so the report reads in order.
        use owo_colors::{OwoColorize, Stream::Stdout};
        println!("{} {s}", "[ fail ]".if_supports_color(Stdout, |t| t.red()));
        hint(fix);
    }
}

fn hint(fix: &str) {
    if !fix.is_empty() {
        println!("         {}", format!("fix: {fix}").as_str());
    }
}

const SHORT: Duration = Duration::from_secs(10);

pub fn run(ctx: &Ctx, full: bool) -> Result<()> {
    let repo = ctx.repo()?.to_path_buf();
    let meta = meta_src(&repo);
    let mut r = Report::default();

    ui::heading("nix");
    nix_daemon(ctx, &mut r);
    caches(&meta, &mut r);
    flake(ctx, full, &mut r);

    ui::heading("secrets and keys");
    secrets(ctx, &repo, &meta, &mut r);
    gh(&mut r);
    let sock = agent(ctx, &meta, &mut r);
    signing(ctx, &meta, sock.as_deref(), full, &mut r);

    if ctx.is_arc() {
        ui::heading("desktop");
        keyring(&mut r);
        mango(&mut r);
        vshell(&mut r);
    }

    println!();
    match (r.fails, r.warns) {
        (0, 0) => ui::ok("all good"),
        (0, w) => ui::warn(&format!("{w} warning(s), nothing broken")),
        (f, w) => bail!("{f} problem(s), {w} warning(s)"),
    }
    Ok(())
}

/// Run quietly with a deadline; (ok, stdout, stderr tail). stdout is dropped
/// when `keep_out` is false (e.g. decrypted secrets).
fn check(c: &mut Command, dur: Duration, keep_out: bool) -> Option<(bool, String, String)> {
    if !keep_out {
        c.stdout(Stdio::null());
    } else {
        c.stdout(Stdio::piped());
    }
    let mut child = c.stdin(Stdio::null()).stderr(Stdio::piped()).spawn().ok()?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let read = |p: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut p) = p {
                let _ = p.read_to_string(&mut s);
            }
            s
        })
    };
    let (ho, he) = (read(out.map(|o| Box::new(o) as _)), read(err.map(|e| Box::new(e) as _)));
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < dur => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let stderr = he.join().unwrap_or_default();
    let tail: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = tail[tail.len().saturating_sub(3)..].join("\n         ");
    Some((status.success(), ho.join().unwrap_or_default().trim_end().to_string(), tail))
}

fn nix_daemon(ctx: &Ctx, r: &mut Report) {
    let fix = match ctx.host {
        Host::Arc => "sudo systemctl restart nix-daemon",
        Host::Mac => "sudo launchctl kickstart -k system/org.nixos.nix-daemon",
    };
    for sub in ["info", "ping"] {
        let mut c = ctx.nix();
        c.args(["store", sub]);
        if let Some((true, _, _)) = check(&mut c, SHORT, false) {
            return r.ok("nix daemon answers");
        }
    }
    r.fail("nix daemon doesn't answer (nix store info)", fix);
}

fn reachable(url: &str) -> bool {
    matches!(
        run_timeout(Command::new("curl").args(["-fsS", "-m", "5", "-o", "/dev/null", url]), Duration::from_secs(7)),
        Some((true, _))
    )
}

fn caches(meta: &str, r: &mut Report) {
    if reachable("https://cache.nixos.org/nix-cache-info") {
        r.ok("cache.nixos.org reachable");
    } else {
        r.fail("cache.nixos.org unreachable", "check the network / DNS (resolvectl status)");
    }
    match binary_cache(meta) {
        None => r.info("homelab cache is off (meta.binaryCache = null)"),
        Some((endpoint, cache)) => {
            let url = format!("{endpoint}/{cache}/nix-cache-info");
            if reachable(&url) {
                r.ok(&format!("homelab cache {endpoint}/{cache} reachable"));
            } else {
                r.warn(
                    &format!("homelab cache {endpoint}/{cache} unreachable (nix falls back to building)"),
                    "is NetBird up (netbird status)? is the homelab on?",
                );
            }
        }
    }
}

fn flake(ctx: &Ctx, full: bool, r: &mut Report) {
    let repo = ctx.repo.display().to_string();
    let attr = match ctx.host {
        Host::Arc => format!("{repo}#nixosConfigurations.{NIXOS_HOST}.config.system.build.toplevel.drvPath"),
        Host::Mac => format!("{repo}#darwinConfigurations.{MAC_HOST}.system.drvPath"),
    };
    ui::dim("evaluating this machine's config...");
    let mut c = ctx.nix();
    c.args(["eval", "--raw", &attr]);
    match check(&mut c, Duration::from_secs(600), false) {
        Some((true, _, _)) => r.ok("this machine's config evaluates"),
        Some((false, _, tail)) => r.fail(&format!("config doesn't evaluate:\n         {tail}"), "vx check for the full trace"),
        None => r.fail("evaluating took over 10 minutes", "try again with -v; a fetch may be hanging"),
    }
    if full {
        ui::dim("nix flake check --no-build --all-systems...");
        let mut c = ctx.nix();
        c.args(["flake", "check", "--no-build", "--all-systems"]);
        match check(&mut c, Duration::from_secs(1800), false) {
            Some((true, _, _)) => r.ok("nix flake check passes (all systems)"),
            Some((false, _, tail)) => r.fail(&format!("nix flake check fails:\n         {tail}"), "vx check for the full trace"),
            None => r.fail("nix flake check took over 30 minutes", "run vx check to watch it"),
        }
    }
}

fn secrets(ctx: &Ctx, repo: &Path, meta: &str, r: &mut Report) {
    let file = repo.join("secrets/secrets.yaml");
    let enc = std::fs::read_to_string(&file).unwrap_or_default();
    match ctx.host {
        Host::Mac => {
            if !ctx::has("sops") {
                return r.fail("sops isn't installed", "vx switch (nix-darwin installs it) or brew install sops");
            }
            let mut c = ctx.cmd("sops");
            c.args(["-d"]).arg(&file);
            match check(&mut c, SHORT, false) {
                Some((true, _, _)) => r.ok("sops decrypts secrets/secrets.yaml"),
                _ => r.fail(
                    "sops can't decrypt secrets/secrets.yaml",
                    "put the Mac's age key in ~/Library/Application Support/sops/age/keys.txt (or set SOPS_AGE_KEY_FILE)",
                ),
            }
        }
        Host::Arc => {
            if meta_bool(meta, "secretsReady") != Some(true) {
                return r.info("secrets are off (meta.secretsReady = false; README \"Secrets\")");
            }
            let pubkey = "/persistent/etc/ssh/ssh_host_ed25519_key.pub";
            let mut c = if ctx::has("ssh-to-age") {
                ctx.cmd("ssh-to-age")
            } else {
                let mut c = ctx.nix();
                c.args(["run", "nixpkgs#ssh-to-age", "--"]);
                c
            };
            c.args(["-i", pubkey]);
            match check(&mut c, Duration::from_secs(120), true) {
                Some((true, age, _)) if enc.contains(age.trim()) => r.ok("the PC's host key is a recipient of secrets.yaml"),
                Some((true, age, _)) => r.fail(
                    &format!("the PC ({}) isn't a recipient of secrets.yaml", age.trim()),
                    "on the Mac: add it to .sops.yaml, then sops updatekeys secrets/secrets.yaml",
                ),
                _ => r.warn(&format!("could not read the host key {pubkey}"), "is /persistent mounted?"),
            }
            if Path::new("/run/secrets").exists() {
                r.ok("sops-nix installed the secrets (/run/secrets)");
            } else {
                r.fail("/run/secrets is missing: sops-nix couldn't decrypt at activation", "journalctl -b | grep -i sops");
            }
        }
    }
}

fn gh(r: &mut Report) {
    if !ctx::has("gh") {
        return r.warn("gh isn't installed", "it comes with the dev tools; vx switch");
    }
    match run_timeout(Command::new("gh").args(["auth", "status"]), SHORT) {
        Some((true, _)) => r.ok("gh is logged in"),
        Some((false, _)) => r.fail("gh isn't logged in (git push over https needs it)", "gh auth login"),
        None => r.warn("gh auth status timed out", "check the network"),
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// The 1Password agent socket, from modules/apps/1password.nix on the PC and
/// the app's fixed location on the Mac.
fn agent_sock(host: Host) -> PathBuf {
    match host {
        Host::Arc => home().join(".1password/agent.sock"),
        Host::Mac => home().join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"),
    }
}

fn is_socket(p: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(p).map(|m| m.file_type().is_socket()).unwrap_or(false)
}

/// The key part of `ssh-ed25519 AAAA... comment`.
fn key_body(k: &str) -> Option<&str> {
    k.split_whitespace().nth(1)
}

fn agent(ctx: &Ctx, meta: &str, r: &mut Report) -> Option<PathBuf> {
    let sock = agent_sock(ctx.host);
    if !is_socket(&sock) {
        r.fail(
            &format!("1password ssh agent socket missing ({})", sock.display()),
            "open 1Password: Settings > Developer > Use the SSH agent",
        );
        return None;
    }
    r.ok("1password ssh agent socket is there");

    let env = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();
    let env_ok = !env.is_empty() && std::fs::canonicalize(&env).ok() == std::fs::canonicalize(&sock).ok();
    let ssh_config = std::fs::read_to_string(home().join(".ssh/config")).unwrap_or_default();
    if env_ok {
        r.ok("SSH_AUTH_SOCK points at it");
    } else if std::env::var_os("SSH_CONNECTION").is_some() {
        r.info("SSH_AUTH_SOCK is the ssh session's (you're logged in over ssh)");
    } else if ctx.host == Host::Mac && ssh_config.to_lowercase().contains("1password") {
        r.ok("~/.ssh/config uses the 1password agent");
    } else {
        r.warn(
            &format!("SSH_AUTH_SOCK is {} instead of the 1password agent", if env.is_empty() { "unset" } else { &env }),
            match ctx.host {
                Host::Arc => "log out and back in (home-manager sets it for new sessions)",
                Host::Mac => "add IdentityAgent \"~/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock\" to ~/.ssh/config",
            },
        );
    }

    let mut c = Command::new("ssh-add");
    c.arg("-L").env("SSH_AUTH_SOCK", &sock);
    let want = meta_str(meta, "signingKey");
    match run_timeout(&mut c, SHORT) {
        Some((true, keys)) => match want.as_deref().and_then(key_body) {
            Some(body) if keys.contains(body) => r.ok("the signing key is in the agent"),
            Some(_) => r.warn("the agent doesn't offer meta.signingKey", "in 1Password, allow that key for the SSH agent"),
            None => r.ok(&format!("the agent offers {} key(s)", keys.lines().count())),
        },
        _ => r.warn("the agent offers no keys (1Password locked?)", "unlock 1Password"),
    }
    Some(sock)
}

fn git_config(ctx: &Ctx, key: &str) -> Option<String> {
    ctx.probe(ctx.cmd("git").args(["config", "--get", key])).filter(|s| !s.is_empty())
}

fn signing(ctx: &Ctx, meta: &str, sock: Option<&Path>, full: bool, r: &mut Report) {
    if git_config(ctx, "commit.gpgsign").as_deref() != Some("true") {
        return r.info("git commit signing is off");
    }
    if git_config(ctx, "gpg.format").as_deref() != Some("ssh") {
        return r.warn("commits are signed, but not with ssh (gpg.format)", "vx switch so home-manager sets it");
    }
    let program = git_config(ctx, "gpg.ssh.program").unwrap_or_else(|| "ssh-keygen".into());
    let found = if program.contains('/') { Path::new(&program).exists() } else { ctx::has(&program) };
    if !found {
        return r.fail(&format!("git's signer {program} isn't there"), "install 1Password (it ships op-ssh-sign), or vx switch");
    }
    let Some(key) = git_config(ctx, "user.signingkey").or_else(|| meta_str(meta, "signingKey")) else {
        return r.fail("no user.signingkey", "vx switch so home-manager sets it");
    };
    // A key file with its private half on disk signs without any agent.
    let key_path = key.strip_prefix('~').map(|k| format!("{}{k}", home().display())).unwrap_or(key.clone());
    let private_on_disk = !key.starts_with("ssh-") && !key.starts_with("key::") && Path::new(key_path.trim_end_matches(".pub")).is_file();
    let name = Path::new(&program).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| program.clone());
    // ssh-keygen only finds the agent through SSH_AUTH_SOCK (it ignores ~/.ssh/config).
    let env_sock = std::env::var("SSH_AUTH_SOCK").ok().and_then(|s| std::fs::canonicalize(s).ok());
    let reaches = sock.is_some_and(|s| std::fs::canonicalize(s).ok() == env_sock);
    if private_on_disk {
        r.ok(&format!("git signs with {key_path} (key on disk, not 1password)"));
    } else if name == "ssh-keygen" && !reaches && std::env::var_os("SSH_CONNECTION").is_none() {
        r.warn(
            "git signs with ssh-keygen, which only finds 1Password through SSH_AUTH_SOCK",
            "set gpg.ssh.program to 1Password's op-ssh-sign (/Applications/1Password.app/Contents/MacOS/op-ssh-sign on the Mac)",
        );
    } else {
        r.ok(&format!("git signs with ssh via {name}"));
    }
    if !full {
        return;
    }

    // A real signature the way git makes one (same signer, same environment):
    // a throwaway commit object nothing points at. 1Password asks to approve.
    ui::dim("signing a test commit (approve it in 1Password)...");
    let mut c = ctx.cmd("git");
    c.args(["commit-tree", "-S", "-m", "vx doctor signing test", "HEAD^{tree}"]);
    match c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output() {
        Ok(o) if o.status.success() => r.ok("git signing works (signed a test commit)"),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let last = err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            r.fail(&format!("git couldn't sign: {last}"), "unlock 1Password and allow the key for the SSH agent")
        }
        Err(e) => r.fail(&format!("could not run git: {e}"), ""),
    }
}

fn keyring(r: &mut Report) {
    let user = std::env::var("USER").unwrap_or_default();
    match run_timeout(Command::new("pgrep").args(["-u", &user, "-f", "gnome-keyring-daemon"]), SHORT) {
        Some((true, _)) => r.ok("gnome-keyring is running"),
        _ => r.warn("gnome-keyring isn't running (apps can't save logins)", "log out and back in through the greeter (PAM starts it)"),
    }
}

fn mango(r: &mut Report) {
    let env = run_timeout(Command::new("systemctl").args(["--user", "show-environment"]), SHORT).map(|(_, s)| s).unwrap_or_default();
    let val = env.lines().find_map(|l| l.strip_prefix("XDG_CURRENT_DESKTOP=")).map(str::to_string);
    match val.as_deref() {
        Some("mango") => r.ok("user services see XDG_CURRENT_DESKTOP=mango"),
        Some(other) => r.fail(
            &format!("user services see XDG_CURRENT_DESKTOP={other}, not mango (portals pick the wrong backend)"),
            "log in with the Mango session from the greeter",
        ),
        None => r.warn("XDG_CURRENT_DESKTOP isn't set for user services (no Mango session?)", "log in with the Mango session from the greeter"),
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        let here = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if here != "mango" {
            r.warn(&format!("this shell has XDG_CURRENT_DESKTOP={here}"), "open a new terminal, or log out and back in");
        }
    }
}

fn vshell(r: &mut Report) {
    match run_timeout(Command::new("systemctl").args(["--user", "is-active", "vshell"]), SHORT) {
        Some((true, _)) => r.ok("vshell is running"),
        Some((false, s)) => r.fail(&format!("vshell is {}", if s.is_empty() { "not running" } else { &s }), "systemctl --user restart vshell, then vx shell log"),
        None => r.warn("systemctl --user timed out", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body() {
        assert_eq!(key_body("ssh-ed25519 AAAAC3 me@host"), Some("AAAAC3"));
        assert_eq!(key_body("ssh-ed25519"), None);
    }
}
