//! Checks for what the user types. Each returns a short lowercase message
//! that says what to change.

use std::path::Path;

pub fn hostname(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 63 {
        return Err("hostname: 1 to 63 characters".into());
    }
    if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("hostname: only a-z, 0-9 and -".into());
    }
    if s.starts_with('-') || s.ends_with('-') {
        return Err("hostname: can't start or end with -".into());
    }
    Ok(())
}

const RESERVED_USERS: [&str; 12] =
    ["root", "nobody", "nixbld", "daemon", "bin", "sys", "messagebus", "sshd", "systemd-network", "users", "wheel", "admin"];

pub fn username(s: &str) -> Result<(), String> {
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    if !first_ok || s.len() > 32 || !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err("username: start with a-z, then a-z 0-9 _ - (32 at most)".into());
    }
    if RESERVED_USERS.contains(&s) || s.starts_with("nixbld") {
        return Err(format!("username: {s} is a system account, pick another"));
    }
    Ok(())
}

/// A data drive's name: label, LUKS name and key file name.
pub fn data_name(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 24 {
        return Err("name: 1 to 24 characters".into());
    }
    if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("name: only a-z, 0-9 and -".into());
    }
    if matches!(s, "main" | "nixos" | "cryptroot" | "root" | "boot") {
        return Err(format!("name: {s} is used by the system disk"));
    }
    Ok(())
}

/// Where a data drive is mounted. `user` is the login name (its home itself is off limits).
pub fn mount_path(s: &str, user: &str) -> Result<(), String> {
    if !s.starts_with('/') || s.len() < 2 {
        return Err("mount path: an absolute path like /mnt/d".into());
    }
    if s.ends_with('/') || s.contains("//") {
        return Err("mount path: no trailing or double /".into());
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.')) {
        return Err("mount path: only letters, digits and / - _ .".into());
    }
    if s.split('/').any(|p| p == "." || p == "..") {
        return Err("mount path: no . or .. parts".into());
    }
    let home = format!("/home/{user}");
    let reserved = ["/home", home.as_str(), "/persistent", "/nix", "/boot", "/var", "/var/log", "/etc", "/usr", "/tmp", "/run", "/proc", "/sys", "/dev", "/root"];
    if reserved.contains(&s) {
        return Err(format!("mount path: {s} is used by the system"));
    }
    for under in ["/nix/", "/persistent/", "/boot/", "/proc/", "/sys/", "/dev/", "/run/", "/etc/", "/tmp/"] {
        if s.starts_with(under) {
            return Err(format!("mount path: not inside {}", under.trim_end_matches('/')));
        }
    }
    Ok(())
}

/// A tz database name; checked against `zoneinfo` (/etc/zoneinfo) when it exists.
pub fn time_zone(s: &str, zoneinfo: Option<&Path>) -> Result<(), String> {
    let shape = !s.is_empty()
        && !s.starts_with('/')
        && !s.ends_with('/')
        && s.split('/').all(|p| !p.is_empty() && p != "." && p != "..")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'));
    if !shape {
        return Err("time zone: like America/Chicago or UTC".into());
    }
    if let Some(dir) = zoneinfo.filter(|d| d.is_dir())
        && !dir.join(s).is_file()
    {
        return Err(format!("time zone: {s} isn't a known zone (like America/Chicago)"));
    }
    Ok(())
}

/// Sizes the way disks are sold: decimal.
pub fn human_bytes(n: u64) -> String {
    let f = n as f64;
    if f >= 1e12 {
        format!("{:.1} tb", f / 1e12)
    } else if f >= 1e9 {
        format!("{:.0} gb", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.0} mb", f / 1e6)
    } else {
        format!("{n} b")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames() {
        assert!(hostname("arc").is_ok());
        assert!(hostname("arc-2").is_ok());
        for bad in ["", "Arc", "-arc", "arc-", "a.b", "a b", &"a".repeat(64)] {
            assert!(hostname(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn usernames() {
        assert!(username("vainnor").is_ok());
        assert!(username("_x-1").is_ok());
        for bad in ["", "Vainnor", "1abc", "root", "nixbld3", "a b", &"a".repeat(33)] {
            assert!(username(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn data_names() {
        assert!(data_name("games").is_ok());
        assert!(data_name("d").is_ok());
        for bad in ["", "Games", "a_b", "nixos", "a/b"] {
            assert!(data_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn mount_paths() {
        assert!(mount_path("/mnt/d", "vainnor").is_ok());
        assert!(mount_path("/home/vainnor/games", "vainnor").is_ok());
        for bad in ["mnt/d", "/", "/mnt/d/", "/home", "/home/vainnor", "/nix/x", "/persistent", "/mnt/../etc", "/mnt/a b", "/mnt//d"] {
            assert!(mount_path(bad, "vainnor").is_err(), "{bad}");
        }
    }

    #[test]
    fn time_zones() {
        assert!(time_zone("America/Chicago", None).is_ok());
        assert!(time_zone("UTC", None).is_ok());
        assert!(time_zone("../etc/passwd", None).is_err());
        assert!(time_zone("/UTC", None).is_err());
        let dir = std::env::temp_dir().join(format!("vx-zoneinfo-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("America")).unwrap();
        std::fs::write(dir.join("America/Chicago"), b"TZif").unwrap();
        assert!(time_zone("America/Chicago", Some(&dir)).is_ok());
        assert!(time_zone("America/Chicgo", Some(&dir)).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sizes() {
        assert_eq!(human_bytes(1000204886016), "1.0 tb");
        assert_eq!(human_bytes(2000398934016), "2.0 tb");
        assert_eq!(human_bytes(30752636928), "31 gb");
    }
}
