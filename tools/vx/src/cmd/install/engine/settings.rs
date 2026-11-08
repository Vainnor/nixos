//! Writing the installer's choices into the repo copy: meta.nix, the host
//! file and disks.json. Same approach as `vx wallpaper` (desktop/wallpaper.rs):
//! targeted regex replacements of whole `name = ...;` lines.

use super::super::plan::{DisksJson, Plan, Settings};
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

/// The host file with `networking.hostName` set.
pub fn edit_host(text: &str, hostname: &str, file: &str) -> Result<String> {
    set_line(text, "networking.hostName", hostname, file)
}

/// disks.json: pretty, with a trailing newline.
pub fn disks_json(plan: &Plan) -> String {
    let d = DisksJson { system: plan.system.clone(), data: plan.data.clone() };
    serde_json::to_string_pretty(&d).expect("plain data serializes") + "\n"
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
  };
}
"#
            .to_string()
        })
    }

    /// A copy of modules/hosts/arc/default.nix (the part that matters).
    const HOST: &str = r#"{ inputs, config, ... }:
{
  flake.modules.nixos.arc = {
    imports = [ ./_disko.nix ];

    networking.hostName = "arc";

    # Set once at install time
    system.stateVersion = "26.11";
  };
}
"#;

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

    #[test]
    fn host_edit() {
        let out = edit_host(HOST, "forge", "default.nix").unwrap();
        assert!(out.contains("    networking.hostName = \"forge\";\n"));
        assert_eq!(out.replace("\"forge\"", "\"arc\""), HOST);
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
