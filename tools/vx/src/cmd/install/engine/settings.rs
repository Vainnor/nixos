//! Writing the installer's choices into the repo copy: meta.nix, a new host's
//! host.json and disks.json. Same approach as `vx wallpaper`
//! (desktop/wallpaper.rs): targeted replacements of whole `name = ...;`
//! statements, never a rewrite of the file.

use super::super::plan::{DisksJson, HostJson, NewHost, Plan, Settings};
use anyhow::{Result, bail};
use regex::Regex;

/// A Nix string literal's contents.
fn nix_str(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', "\\\"").replace("${", "\\${")
}

/// Replace the one `<key> = ...;` line (indented, not a comment). Errors when
/// there isn't exactly one, so a reworded file fails loudly instead of being skipped.
fn set_line(text: &str, key: &str, value: &str, file: &str) -> Result<String> {
    let re = Regex::new(&format!(r"(?m)^([ \t]*){} = [^;\n]*;", regex::escape(key))).unwrap();
    let n = re.find_iter(text).count();
    if n != 1 {
        bail!("expected one `{key} = ...;` line in {file}, found {n}: update the installer (engine/settings.rs)");
    }
    let rep = format!("${{1}}{key} = \"{}\";", nix_str(value));
    Ok(re.replace(text, rep.as_str()).into_owned())
}

/// A `<key> = "<value>";` string from a Nix file (first match).
pub fn get_string(text: &str, key: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"(?m)^[ \t]*{} = "([^"]*)";"#, regex::escape(key))).unwrap();
    re.captures(text).map(|c| c[1].to_string())
}

/// meta.nix with the user's settings.
pub fn edit_meta(text: &str, s: &Settings) -> Result<String> {
    const F: &str = "modules/flake/meta.nix";
    let t = set_line(text, "username", &s.username, F)?;
    let t = set_line(&t, "flakePath", &format!("/home/{}/nixos", s.username), F)?;
    let t = set_line(&t, "timeZone", &s.time_zone, F)?;
    let t = set_line(&t, "wallpaper", &s.wallpaper, F)?;
    set_line(&t, "theme", &s.theme, F)
}

/// Where the one top-level `<key> = <value>;` statement is: (start of `key`,
/// end just past its `;`). The value may span lines (lists, attrsets);
/// strings and comments are skipped while looking for the `;`.
fn statement(text: &str, key: &str, file: &str) -> Result<(usize, usize)> {
    let re = Regex::new(&format!(r"(?m)^[ \t]*({}) = ", regex::escape(key))).unwrap();
    let found: Vec<_> = re.captures_iter(text).collect();
    if found.len() != 1 {
        bail!("expected one `{key} = ...;` in {file}, found {}: update the installer (engine/settings.rs)", found.len());
    }
    let m = &found[0];
    let start = m.get(1).unwrap().start();
    let b = text.as_bytes();
    let (mut i, mut depth) = (m.get(0).unwrap().end(), 0i32);
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'\'' if b.get(i + 1) == Some(&b'\'') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'\'' && b[i + 1] == b'\'') {
                    i += 1;
                }
                i += 1;
            }
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => depth -= 1,
            b';' if depth == 0 => return Ok((start, i + 1)),
            _ => {}
        }
        i += 1;
    }
    bail!("`{key} = ...` in {file} has no closing `;`")
}

/// Replace the whole `<key> = ...;` statement (any number of lines) with
/// `<key> = <raw>;`, plus ` # <comment>` unless the line already has a comment.
fn set_raw(text: &str, key: &str, raw: &str, comment: Option<&str>, file: &str) -> Result<String> {
    let (s, e) = statement(text, key, file)?;
    let mut new = format!("{key} = {raw};");
    let rest_of_line = text[e..].split('\n').next().unwrap_or("");
    if let Some(c) = comment
        && !rest_of_line.trim_start().starts_with('#')
    {
        new.push_str(&format!(" # {c}"));
    }
    Ok(format!("{}{new}{}", &text[..s], &text[e..]))
}

/// The indentation of the line `key = ` is on.
fn indent_of(text: &str, key: &str) -> String {
    let re = Regex::new(&format!(r"(?m)^([ \t]*){} = ", regex::escape(key))).unwrap();
    re.captures(text).map(|c| c[1].to_string()).unwrap_or_else(|| "    ".into())
}

/// A Nix list of strings, one per line, indented under `indent`.
fn nix_list(items: &[String], indent: &str) -> String {
    if items.is_empty() {
        return "[ ]".into();
    }
    let mut s = "[\n".to_string();
    for i in items {
        s.push_str(&format!("{indent}  \"{}\"\n", nix_str(i)));
    }
    s.push_str(&format!("{indent}]"));
    s
}

/// Someone else's install from their fork: their identity in meta.nix, and
/// nothing of the owner's that would let the owner in or needs the owner's secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Personal {
    pub full_name: String,
    pub email: String,
    /// Their public keys from GitHub: the only keys that can ssh in.
    pub ssh_keys: Vec<String>,
    /// The fork (https).
    pub repo_url: String,
    pub github_user: String,
}

/// meta.nix personalised for a fork (after [`edit_meta`]).
pub fn personalise_meta(text: &str, p: &Personal) -> Result<String> {
    const F: &str = "modules/flake/meta.nix";
    let t = set_line(text, "fullName", &p.full_name, F)?;
    let t = set_line(&t, "email", &p.email, F)?;
    let t = set_line(&t, "repoUrl", &p.repo_url, F)?;
    let t = match statement(&t, "githubUser", F) {
        Ok(_) => set_line(&t, "githubUser", &p.github_user, F)?,
        Err(_) => {
            // Older meta.nix without it: add it under repoUrl.
            let (_, e) = statement(&t, "repoUrl", F)?;
            let ind = indent_of(&t, "repoUrl");
            format!("{}\n{ind}githubUser = \"{}\";{}", &t[..e], nix_str(&p.github_user), &t[e..])
        }
    };
    let ind = indent_of(&t, "sshKeys");
    let t = set_raw(&t, "sshKeys", &nix_list(&p.ssh_keys, &ind), None, F)?;
    let t = set_raw(&t, "signingKey", "null", Some("your ssh public key here to sign commits (from your ssh agent / 1password)"), F)?;
    let t = set_raw(&t, "secretsReady", "false", None, F)?;
    let t = set_raw(&t, "resticReady", "false", None, F)?;
    set_raw(&t, "binaryCache", "null", None, F)
}

/// disks.json: pretty, with a trailing newline.
pub fn disks_json(plan: &Plan) -> String {
    let d = DisksJson { system: plan.system.clone(), data: plan.data.clone() };
    serde_json::to_string_pretty(&d).expect("plain data serializes") + "\n"
}

/// The Nix system of this machine (the installer runs on the machine it installs).
pub fn nix_system() -> &'static str {
    if cfg!(target_arch = "aarch64") { "aarch64-linux" } else { "x86_64-linux" }
}

/// host.json for a new host: pretty, with a trailing newline. `base` first, always.
pub fn host_json(h: &NewHost) -> String {
    let mut aspects = vec!["base".to_string()];
    aspects.extend(h.aspects.iter().filter(|a| a.as_str() != "base").cloned());
    let j = HostJson {
        name: h.name.clone(),
        system: nix_system().into(),
        state_version: h.state_version.clone(),
        aspects,
        gpu: h.gpu,
        primary_monitor: None,
        audio_output: None,
        replay_monitor: "screen".into(),
    };
    serde_json::to_string_pretty(&j).expect("plain data serializes") + "\n"
}

/// A host name the flake can use: `[a-z][a-z0-9-]{0,30}`, and not taken by something else.
/// A new host's name (its folder and hostname); `aspects` are the flake's aspect names, which a host can't share.
pub fn host_name(s: &str, aspects: &[&str]) -> Result<(), String> {
    let re = Regex::new(r"^[a-z][a-z0-9-]{0,30}$").unwrap();
    if !re.is_match(s) || s.ends_with('-') {
        return Err("name: start with a-z, then a-z 0-9 - (31 at most)".into());
    }
    if matches!(s, "mac" | "installer" | "default" | "base" | "nixos" | "localhost") {
        return Err(format!("name: {s} is taken by the flake, pick another"));
    }
    if aspects.contains(&s) {
        return Err(format!("name: {s} is a feature's name, pick another"));
    }
    Ok(())
}

/// Values that end up in Nix files, paths and shell lines: keep them plain.
pub fn validate(s: &Settings) -> Result<()> {
    let user = Regex::new(r"^[a-z_][a-z0-9_-]{0,31}$").unwrap();
    if !user.is_match(&s.username) || s.username == "root" {
        bail!("'{}' can't be a username: lowercase letters, digits, - and _, starting with a letter", s.username);
    }
    let host = Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$").unwrap();
    if !host.is_match(&s.hostname) {
        bail!("'{}' can't be a hostname: letters, digits and -, up to 63", s.hostname);
    }
    let tz = Regex::new(r"^[A-Za-z0-9_+./-]+$").unwrap();
    if !tz.is_match(&s.time_zone) || s.time_zone.contains("..") {
        bail!("'{}' isn't a time zone like America/Chicago", s.time_zone);
    }
    let name = Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9_.-]*$").unwrap();
    if !name.is_match(&s.wallpaper) {
        bail!("'{}' isn't a wallpaper name", s.wallpaper);
    }
    if !name.is_match(&s.theme) {
        bail!("'{}' isn't a theme name", s.theme);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::install::engine::tests::plan_fresh;

    /// The real meta.nix when the whole repo is there; the Nix build only has
    /// tools/vx, so it falls back to a copy of the lines the edits touch.
    fn meta_text() -> String {
        let real = concat!(env!("CARGO_MANIFEST_DIR"), "/../../modules/flake/meta.nix");
        std::fs::read_to_string(real).unwrap_or_else(|_| {
            r#"{ lib, ... }:
{
  config.meta = {
    username = "vainnor";
    fullName = "Vainnor";
    email = "me@example.com";

    timeZone = "America/Chicago";

    repoUrl = "https://github.com/vainnor/nixos";

    # Where the repo is cloned on the PC (nh uses this).
    flakePath = "/home/vainnor/nixos";

    # Wallpaper and colours.
    #   wallpaper: a folder in assets/wallpapers (`vx wallpapers` lists them),
    wallpaper = "eye";
    theme = "match";
    themeOverrides = { };

    secretsReady = false;

    sshKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores vainnor@Carsons-MBP.home"
    ];

    # Git commits are signed with this key.
    signingKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores";

    resticReady = false;

    binaryCache = {
      endpoint = "https://cache.example.com"; # the homelab
      cache = "nixos";
    };
  };
}
"#
            .to_string()
        })
    }

    fn settings() -> Settings {
        Settings {
            hostname: "forge".into(),
            username: "carson".into(),
            time_zone: "Europe/Oslo".into(),
            wallpaper: "noir".into(),
            theme: "vainnor-slate".into(),
        }
    }

    #[test]
    fn meta_edits_only_those_lines() {
        let meta = &meta_text();
        let out = edit_meta(meta, &settings()).unwrap();
        assert!(out.contains("    username = \"carson\";\n"));
        assert!(out.contains("    flakePath = \"/home/carson/nixos\";\n"));
        assert!(out.contains("    timeZone = \"Europe/Oslo\";\n"));
        assert!(out.contains("    wallpaper = \"noir\";\n"));
        assert!(out.contains("    theme = \"vainnor-slate\";\n"));
        // Comments and look-alike keys stay.
        assert!(out.contains("#   wallpaper: a folder in assets/wallpapers"));
        assert!(out.contains("themeOverrides = { };"));
        assert_eq!(get_string(&out, "email"), get_string(meta, "email"));
        // Exactly five lines changed.
        let changed = meta.lines().zip(out.lines()).filter(|(a, b)| a != b).count();
        assert_eq!(changed, 5);
        assert_eq!(meta.lines().count(), out.lines().count());
    }

    #[test]
    fn meta_edit_is_idempotent() {
        let meta = &meta_text();
        let once = edit_meta(meta, &settings()).unwrap();
        assert_eq!(edit_meta(&once, &settings()).unwrap(), once);
    }

    #[test]
    fn meta_without_a_key_fails() {
        let meta = &meta_text();
        let broken = meta.replace("timeZone = ", "tz = ");
        let e = edit_meta(&broken, &settings()).unwrap_err().to_string();
        assert!(e.contains("timeZone"), "{e}");
    }

    #[test]
    fn reads_the_default_username() {
        let meta = &meta_text();
        assert_eq!(get_string(meta, "username").as_deref(), Some("vainnor"));
    }

    fn personal() -> Personal {
        Personal {
            full_name: "Some One".into(),
            email: "someone@example.com".into(),
            ssh_keys: vec!["ssh-ed25519 AAAAsomeone1".into(), "ssh-rsa AAAAsomeone2".into()],
            repo_url: "https://github.com/someone/nixos.git".into(),
            github_user: "someone".into(),
        }
    }

    #[test]
    fn personalised_meta() {
        let meta = &meta_text();
        let owner_key = get_string(meta, "signingKey");
        let out = personalise_meta(&edit_meta(meta, &settings()).unwrap(), &personal()).unwrap();
        assert!(out.contains("    fullName = \"Some One\";\n"));
        assert!(out.contains("    email = \"someone@example.com\";\n"));
        assert!(out.contains("    repoUrl = \"https://github.com/someone/nixos.git\";\n"));
        assert!(out.contains("    githubUser = \"someone\";\n"));
        assert!(out.contains("    sshKeys = [\n      \"ssh-ed25519 AAAAsomeone1\"\n      \"ssh-rsa AAAAsomeone2\"\n    ];\n"), "{out}");
        assert!(out.contains("    signingKey = null; # your ssh public key here"));
        assert!(out.contains("    secretsReady = false;\n"));
        assert!(out.contains("    resticReady = false;\n"));
        assert!(out.contains("    binaryCache = null;\n"));
        // Nothing of the owner's keys is left.
        if let Some(k) = owner_key {
            let body = k.split_whitespace().nth(1).unwrap();
            assert!(!out.contains(body), "owner key still there");
        }
        assert!(!out.contains("AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz"), "owner ssh key still there");
        // Comments and the rest stay; it's still one statement per key.
        assert!(out.contains("#   wallpaper: a folder in assets/wallpapers"));
        assert_eq!(statement(&out, "binaryCache", "m").map(|_| ()).ok(), Some(()));
        // Idempotent (a second run, or a fork that was personalised before).
        assert_eq!(personalise_meta(&out, &personal()).unwrap(), out);
        // No keys on GitHub: an empty list.
        let none = personalise_meta(meta, &Personal { ssh_keys: vec![], ..personal() }).unwrap();
        assert!(none.contains("    sshKeys = [ ];\n"));
    }

    #[test]
    fn statements_span_lines() {
        let t = "{\n  a = {\n    endpoint = \"x;y\"; # c;\n    keys = [ \"k\" ];\n  };\n  b = 1;\n  s = ''\n    q; ''\n  ;\n}\n";
        let (s, e) = statement(t, "a", "t").unwrap();
        assert_eq!(&t[s..e], "a = {\n    endpoint = \"x;y\"; # c;\n    keys = [ \"k\" ];\n  };");
        assert_eq!(set_raw(t, "a", "null", None, "t").unwrap(), t.replace(&t[s..e], "a = null;"));
        let (s, e) = statement(t, "s", "t").unwrap();
        assert_eq!(&t[s..e], "s = ''\n    q; ''\n  ;");
        assert!(statement(t, "missing", "t").is_err());
    }

    #[test]
    fn new_host_json() {
        let h = NewHost { name: "forge".into(), aspects: vec!["desktop".into(), "base".into(), "dev".into()], gpu: super::super::super::plan::Gpu::Nvidia, state_version: "26.11".into() };
        let j = host_json(&h);
        assert!(j.ends_with("}\n"));
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["name"], "forge");
        assert_eq!(v["system"], nix_system());
        assert_eq!(v["stateVersion"], "26.11");
        assert_eq!(v["aspects"], serde_json::json!(["base", "desktop", "dev"]));
        assert_eq!(v["gpu"], "nvidia");
        assert!(v["primaryMonitor"].is_null() && v["audioOutput"].is_null());
        assert_eq!(v["replayMonitor"], "screen");
    }

    #[test]
    fn host_names() {
        assert!(host_name("forge", &["desktop"]).is_ok());
        assert!(host_name("pc-2", &[]).is_ok());
        assert!(host_name("desktop", &["desktop", "dev"]).is_err());
        for bad in ["", "Forge", "2pc", "mac", "installer", "a_b", "pc-", &"a".repeat(32)] {
            assert!(host_name(bad, &[]).is_err(), "{bad}");
        }
    }

    #[test]
    fn nix_strings_are_escaped() {
        assert_eq!(nix_str(r#"a"b${c}\"#), r#"a\"b\${c}\\"#);
    }

    #[test]
    fn disks_json_content() {
        let j = disks_json(&plan_fresh());
        assert!(j.ends_with("}\n"));
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["system"]["device"], "/dev/disk/by-id/nvme-SYS");
        assert_eq!(v["system"]["encrypt"], true);
        assert_eq!(v["data"][0]["mode"], "format");
        assert_eq!(v["data"][0]["device"], "/dev/disk/by-id/nvme-GAMES");
        assert_eq!(v["data"][1]["mode"], "mount");
        assert_eq!(v["data"][1]["fsType"], "ntfs3");
        assert!(j.contains("\n  \"system\": {\n"), "pretty: {j}");
    }

    #[test]
    fn validation() {
        assert!(validate(&settings()).is_ok());
        for bad in ["Root", "root", "a b", "", "1x"] {
            let mut s = settings();
            s.username = bad.into();
            assert!(validate(&s).is_err(), "{bad}");
        }
        let mut s = settings();
        s.hostname = "-x".into();
        assert!(validate(&s).is_err());
        s = settings();
        s.time_zone = "../etc".into();
        assert!(validate(&s).is_err());
    }
}
