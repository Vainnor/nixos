//! The summary's next steps, and small helpers (dates, firmware state).

/// What happened to the install commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PushState {
    /// No GitHub (offline, or skipped): committed locally only.
    #[default]
    Offline,
    /// The owner turned pushing off.
    Off,
    /// Pushed to "owner/repo branch".
    Pushed(String),
    /// Tried and failed (the reason).
    Failed(String),
    /// Not tried (the reason), e.g. the repo is the usb's copy.
    NotPushed(String),
}

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
    /// gh's login was copied into the new home.
    pub gh_copied: bool,
    /// Installed from the user's fork (their own secrets, not the owner's).
    pub fork: bool,
    pub push: PushState,
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
    if i.fork {
        v.push(
            "secrets are off (secretsReady = false): your fork can't read the owner's secrets.yaml. \
             make your own with sops when you need them"
                .into(),
        );
    } else if i.host_key_new {
        v.push(format!("on the mac: vx secrets add-host {}, push, then vx switch on the pc", i.age));
    }
    if !i.netbird_carried {
        v.push("sudo netbird up".into());
    }
    if !i.gh_copied {
        v.push("gh auth login".into());
    }
    if i.snapshot {
        v.push(
            "~/nixos is the usb's copy with its own history: git fetch origin, then rebase the install settings \
             commit onto origin/main"
                .into(),
        );
    }
    match &i.push {
        PushState::Pushed(_) => {}
        PushState::Offline => v.push("push later with vx sync: the install commit is only in ~/nixos".into()),
        PushState::Off => v.push("push when ready: cd ~/nixos && git push".into()),
        PushState::Failed(_) | PushState::NotPushed(_) if i.snapshot => {}
        PushState::Failed(_) | PushState::NotPushed(_) => v.push("push later: cd ~/nixos && git push".into()),
    }
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
        assert_eq!(v.last().unwrap(), "push later with vx sync: the install commit is only in ~/nixos");
    }

    #[test]
    fn pushed_and_logged_in() {
        let v = next_steps(&NextInput { gh_copied: true, push: PushState::Pushed("vainnor/nixos main".into()), ..Default::default() });
        assert!(!v.iter().any(|s| s.contains("gh auth") || s.contains("push")), "{v:?}");
        let v = next_steps(&NextInput { push: PushState::Failed("rejected".into()), ..Default::default() });
        assert_eq!(v.last().unwrap(), "push later: cd ~/nixos && git push");
        let v = next_steps(&NextInput { fork: true, host_key_new: true, age: "age1x".into(), ..Default::default() });
        assert!(v.iter().any(|s| s.starts_with("secrets are off")));
        assert!(!v.iter().any(|s| s.contains("add-host")), "no owner secrets for a fork");
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
