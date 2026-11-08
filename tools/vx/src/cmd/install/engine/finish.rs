//! The summary's next steps, and small helpers (dates, firmware state).

/// What the next steps depend on.
#[derive(Clone, Debug, Default)]
pub struct NextInput {
    /// The old Secure Boot keys were carried over.
    pub sb_carried: bool,
    /// The firmware is in Setup Mode right now.
    pub setup_mode: bool,
    /// Encryption + TPM unlock.
    pub tpm: bool,
    /// A new host key was made (secrets need the new recipient).
    pub host_key_new: bool,
    pub age: String,
    pub netbird_carried: bool,
    /// ~/nixos is a snapshot of the usb's copy, with its own history.
    pub snapshot: bool,
}

pub fn next_steps(i: &NextInput) -> Vec<String> {
    let mut v = vec!["remove the usb stick and reboot into nixos".to_string()];
    if i.sb_carried && !i.setup_mode {
        v.push("turn secure boot back on in the bios: your keys are still enrolled".into());
    } else {
        v.push(
            "boot nixos once, then in the bios set secure boot to setup mode (delete keys) and boot again: \
             it enrolls your keys; then turn secure boot on"
                .into(),
        );
    }
    if i.tpm {
        v.push("the disk unlocks itself once secure boot is on".into());
    }
    if i.host_key_new {
        v.push(format!("on the mac: vx secrets add-host {}, push, then vx switch on the pc", i.age));
    }
    if !i.netbird_carried {
        v.push("sudo netbird up".into());
    }
    v.push("gh auth login".into());
    if i.snapshot {
        v.push(
            "~/nixos is the usb's copy with its own history: git fetch origin, then rebase the install settings \
             commit onto origin/main"
                .into(),
        );
    }
    v.push("commit and push ~/nixos (install settings commit)".into());
    v
}

/// Firmware Setup Mode, from efivars (byte 4 after the attribute word).
pub fn setup_mode() -> bool {
    std::fs::read("/sys/firmware/efi/efivars/SetupMode-8be4df61-93ca-11d2-aa0d-00e098032b8c")
        .is_ok_and(|b| b.get(4) == Some(&1))
}

/// Today as YYYY-MM-DD (UTC).
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    date_from_days((secs / 86_400) as i64)
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
pub fn date_from_days(z: i64) -> String {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert_eq!(date_from_days(11_016), "2000-02-29");
        assert_eq!(date_from_days(20_399), "2025-11-07");
        assert_eq!(date_from_days(20_764), "2026-11-07");
    }

    #[test]
    fn fresh_install_steps() {
        let v = next_steps(&NextInput { host_key_new: true, age: "age1xyz".into(), tpm: true, ..Default::default() });
        assert!(v[0].contains("usb"));
        assert!(v[1].starts_with("boot nixos once, then in the bios set secure boot to setup mode"));
        assert!(v.contains(&"the disk unlocks itself once secure boot is on".to_string()));
        assert!(v.contains(&"on the mac: vx secrets add-host age1xyz, push, then vx switch on the pc".to_string()));
        assert!(v.contains(&"sudo netbird up".to_string()));
        assert!(v.contains(&"gh auth login".to_string()));
        assert_eq!(v.last().unwrap(), "commit and push ~/nixos (install settings commit)");
    }

    #[test]
    fn carried_everything() {
        let v = next_steps(&NextInput { sb_carried: true, netbird_carried: true, ..Default::default() });
        assert_eq!(v[1], "turn secure boot back on in the bios: your keys are still enrolled");
        assert!(!v.iter().any(|s| s.contains("netbird") || s.contains("add-host") || s.contains("tpm") || s.contains("unlocks")));
    }

    #[test]
    fn carried_keys_but_setup_mode() {
        let v = next_steps(&NextInput { sb_carried: true, setup_mode: true, ..Default::default() });
        assert!(v[1].starts_with("boot nixos once"));
    }

    #[test]
    fn snapshot_note() {
        let v = next_steps(&NextInput { snapshot: true, ..Default::default() });
        assert!(v.iter().any(|s| s.contains("its own history")));
    }
}
