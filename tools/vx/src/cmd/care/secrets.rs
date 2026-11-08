//! `vx secrets ...`: sops-encrypted secrets/secrets.yaml (system/secrets.nix).

use crate::ctx::{Ctx, Host};
use crate::ui;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use regex::Regex;
use std::io::Write;
use std::process::Stdio;

const FILE: &str = "secrets/secrets.yaml";
const HOST_KEY: &str = "/persistent/etc/ssh/ssh_host_ed25519_key.pub";

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
    /// Print this PC's age recipient (run on the PC)
    Host,
    /// Add the PC as a recipient, re-encrypt, and turn secrets on (run on the Mac)
    AddHost {
        /// The PC's age key, from `vx secrets host`
        key: String,
    },
}

/// `wifi/psk` -> `["wifi"]["psk"]` (sops' key path syntax).
pub fn key_path(key: &str) -> Result<String> {
    if key.is_empty() || key.split('/').any(str::is_empty) {
        bail!("bad key {key:?}; use parts split by /, like wifi/psk");
    }
    Ok(key
        .split('/')
        .map(|p| format!("[{}]", serde_json::Value::from(p)))
        .collect())
}

/// Enable the commented-out `&arc` recipient in .sops.yaml.
pub fn add_recipient(sops_yaml: &str, key: &str) -> String {
    if sops_yaml.contains(key) {
        return sops_yaml.to_string();
    }
    let anchor = Regex::new(r"(?m)^  # - &arc age1.*$").unwrap();
    let alias = Regex::new(r"(?m)^          # - \*arc").unwrap();
    let s = anchor.replace(sops_yaml, regex::NoExpand(&format!("  - &arc {key} # the PC (arc)")));
    alias.replace(&s, "          - *arc").into_owned()
}

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    let repo = ctx.repo()?;
    match cmd {
        Cmd::Edit => ctx.run(ctx.cmd("sops").arg(FILE)),
        Cmd::Get { key } => ctx.run(ctx.cmd("sops").args(["-d", "--extract", &key_path(&key)?, FILE])),
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
            child
                .stdin
                .take()
                .context("no stdin for sops")?
                .write_all(format!("{json}\n").as_bytes())?;
            let st = child.wait()?;
            if !st.success() {
                bail!("sops set failed ({st}); is this the Mac with the editing key?");
            }
            ui::ok(&format!("set {key}"));
            Ok(())
        }
        Cmd::Host => {
            ctx.need(Host::NixOs)?;
            ctx.run(ctx.nix().args(["run", "nixpkgs#ssh-to-age", "--", "-i", HOST_KEY]))
        }
        Cmd::AddHost { key } => {
            if !key.starts_with("age1") {
                bail!("that isn't an age key (they start with age1); get it on the PC with: vx secrets host");
            }
            let f = repo.join(".sops.yaml");
            let before = std::fs::read_to_string(&f).context("can't read .sops.yaml")?;
            let after = add_recipient(&before, &key);
            if !after.contains(&key) {
                bail!("couldn't find the commented `# - &arc age1...` line in .sops.yaml; add the key by hand");
            }
            if after != before {
                std::fs::write(&f, after)?;
            }
            ctx.run(ctx.cmd("sops").args(["updatekeys", "--yes", FILE]))?;
            let meta = repo.join("modules/flake/meta.nix");
            let m = std::fs::read_to_string(&meta)?;
            std::fs::write(&meta, m.replace("secretsReady = false;", "secretsReady = true;"))?;
            ui::ok("done; review with git diff, then commit and deploy to the PC");
            Ok(())
        }
    }
}

/// Read a line with echo off (`stty -echo` on the terminal).
fn read_hidden(prompt: &str) -> Result<String> {
    let tty = || std::fs::File::open("/dev/tty");
    print!("{prompt}");
    std::io::stdout().flush()?;
    let hid = tty()
        .ok()
        .and_then(|t| std::process::Command::new("stty").arg("-echo").stdin(t).status().ok())
        .is_some_and(|s| s.success());
    let mut s = String::new();
    let r = std::io::stdin().read_line(&mut s);
    if hid && let Ok(t) = tty() {
        let _ = std::process::Command::new("stty").arg("echo").stdin(t).status();
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
    fn recipient() {
        let y = "keys:\n  - &mac age1mac # MacBook\n  # - &arc age1...   # the PC\ncreation_rules:\n  - path_regex: x\n    key_groups:\n      - age:\n          - *mac\n          # - *arc\n";
        let out = add_recipient(y, "age1pc");
        assert!(out.contains("  - &arc age1pc # the PC (arc)\n"));
        assert!(out.contains("          - *arc\n"));
        assert!(!out.contains("# - *arc"));
        assert_eq!(add_recipient(&out, "age1pc"), out);
    }
}
