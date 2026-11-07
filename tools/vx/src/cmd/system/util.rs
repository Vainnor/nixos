//! Small helpers for the system commands: timed probes, reading meta.nix and
//! flake.lock, relative ages, shell quoting and terminal prompts.

use anyhow::{Context, Result, bail};
use regex::Regex;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Run quietly with a deadline. Some((exit ok, stdout)) if it finished in time,
/// None if it couldn't start or was killed for taking too long.
pub fn run_timeout(c: &mut Command, dur: Duration) -> Option<(bool, String)> {
    let mut child = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut out = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let s = reader.join().unwrap_or_default();
                return Some((status.success(), s.trim_end().to_string()));
            }
            Ok(None) if start.elapsed() < dur => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Seconds since the epoch.
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Seconds since the epoch of a file's (or symlink's own) mtime.
pub fn mtime(p: &Path) -> Option<u64> {
    let m = std::fs::symlink_metadata(p).ok()?.modified().ok()?;
    m.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// A short relative age: `40 s`, `12 min`, `5 h`, `3 d`.
pub fn ago(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 2 * 86400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86400),
    }
}

/// The text of modules/flake/meta.nix ("" if missing).
pub fn meta_src(repo: &Path) -> String {
    std::fs::read_to_string(repo.join("modules/flake/meta.nix")).unwrap_or_default()
}

/// `key = true;` / `key = false;` from meta.nix (uncommented lines only).
pub fn meta_bool(src: &str, key: &str) -> Option<bool> {
    let re = Regex::new(&format!(r"(?m)^\s*{}\s*=\s*(true|false)\s*;", regex::escape(key))).ok()?;
    re.captures(src).map(|c| &c[1] == "true")
}

/// `key = "value";` from meta.nix (uncommented lines only).
pub fn meta_str(src: &str, key: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"(?m)^\s*{}\s*=\s*"([^"]*)"\s*;"#, regex::escape(key))).ok()?;
    re.captures(src).map(|c| c[1].to_string())
}

/// meta.binaryCache as (endpoint, cache name); None when it's `null`.
pub fn binary_cache(src: &str) -> Option<(String, String)> {
    let re = Regex::new(r"(?ms)^\s*binaryCache\s*=\s*\{(.*?)\}\s*;").ok()?;
    let body = re.captures(src)?.get(1)?.as_str().to_string();
    let endpoint = meta_field(&body, "endpoint")?;
    let cache = meta_field(&body, "cache").unwrap_or_else(|| "nixos".into());
    Some((endpoint.trim_end_matches('/').to_string(), cache))
}

fn meta_field(body: &str, key: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"\b{}\s*=\s*"([^"]*)""#, regex::escape(key))).ok()?;
    re.captures(body).map(|c| c[1].to_string())
}

/// One root input of flake.lock.
#[derive(Debug, PartialEq, Clone)]
pub struct Input {
    pub name: String,
    pub last_modified: u64,
    pub rev: String,
}

/// The root inputs of a flake.lock with their lastModified and rev.
pub fn lock_inputs(json: &str) -> Vec<Input> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return vec![] };
    let nodes = &v["nodes"];
    let root = v["root"].as_str().unwrap_or("root");
    let Some(inputs) = nodes[root]["inputs"].as_object() else { return vec![] };
    let mut out: Vec<Input> = inputs
        .iter()
        .filter_map(|(name, node)| {
            let locked = &nodes[node.as_str()?]["locked"];
            Some(Input {
                name: name.clone(),
                last_modified: locked["lastModified"].as_u64()?,
                rev: locked["rev"].as_str().or_else(|| locked["narHash"].as_str()).unwrap_or("").to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Quote an argument so both POSIX sh and fish read it back unchanged.
/// Safe words go bare; otherwise single quotes, with `'` and `\` escaped
/// outside the quotes (the two shells treat them differently inside).
pub fn shell_quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./=:@,+%".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        return arg.to_string();
    }
    let mut s = String::from("'");
    for c in arg.chars() {
        match c {
            '\'' | '\\' => {
                s.push('\'');
                s.push('\\');
                s.push(c);
                s.push('\'');
            }
            c => s.push(c),
        }
    }
    s.push('\'');
    s
}

/// Ask for a line on the terminal.
pub fn prompt(q: &str) -> Result<String> {
    print!("{q}");
    std::io::stdout().flush()?;
    let mut s = String::new();
    std::io::stdin().lock().read_line(&mut s)?;
    Ok(s.trim_end_matches(['\n', '\r']).to_string())
}

/// Ask for a line without echoing it (passwords).
pub fn prompt_hidden(q: &str) -> Result<String> {
    let stty = |arg: &str| Command::new("stty").arg(arg).stdin(Stdio::inherit()).status();
    stty("-echo").context("could not turn off echo (stty)")?;
    let r = prompt(q);
    let _ = stty("echo");
    println!();
    r
}

/// Feed bytes to a command's stdin, return its stdout.
pub fn pipe(c: &mut Command, input: &[u8]) -> Result<Vec<u8>> {
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()
        .with_context(|| format!("could not start {}", c.get_program().to_string_lossy()))?;
    child.stdin.take().context("no stdin")?.write_all(input)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{} failed ({})", c.get_program().to_string_lossy(), out.status);
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(ago(5), "5 s");
        assert_eq!(ago(125), "2 min");
        assert_eq!(ago(3 * 3600 + 5), "3 h");
        assert_eq!(ago(47 * 3600), "47 h");
        assert_eq!(ago(10 * 86400), "10 d");
    }

    const META: &str = r#"
    secretsReady = false;
    resticReady = true;
    signingKey = "ssh-ed25519 AAAA";
    #   { endpoint = "https://cache.example"; cache = "nixos";
    binaryCache = null;
    "#;

    #[test]
    fn meta() {
        assert_eq!(meta_bool(META, "secretsReady"), Some(false));
        assert_eq!(meta_bool(META, "resticReady"), Some(true));
        assert_eq!(meta_bool(META, "nope"), None);
        assert_eq!(meta_str(META, "signingKey").as_deref(), Some("ssh-ed25519 AAAA"));
        assert_eq!(binary_cache(META), None);
        let on = r#"
    binaryCache = {
      endpoint = "https://cache.lab.vainnor.com/";
      cache = "main";
      publicKey = "main:AAAA";
    };
    "#;
        assert_eq!(binary_cache(on), Some(("https://cache.lab.vainnor.com".into(), "main".into())));
    }

    #[test]
    fn lock() {
        let j = r#"{"nodes":{
          "root":{"inputs":{"nixpkgs":"nixpkgs","hm":"home-manager","x":["a","b"]}},
          "nixpkgs":{"locked":{"lastModified":100,"rev":"abc"}},
          "home-manager":{"locked":{"lastModified":50,"rev":"def"}}
        },"root":"root","version":7}"#;
        let v = lock_inputs(j);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], Input { name: "hm".into(), last_modified: 50, rev: "def".into() });
        assert_eq!(v[1].name, "nixpkgs");
        assert!(lock_inputs("not json").is_empty());
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("switch"), "switch");
        assert_eq!(shell_quote("--pull"), "--pull");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("fix: a thing"), "'fix: a thing'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(r"a\b"), r"'a'\\'b'");
        assert_eq!(shell_quote("$(rm -rf ~)"), "'$(rm -rf ~)'");
    }
}
