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
    /// The installer enrolled the keys (the firmware was in Setup Mode).
    pub sb_enrolled: bool,
    /// Encryption + TPM unlock.
    pub tpm: bool,
    /// Secrets are on and this new host waits for another machine to re-encrypt them.
    pub secrets_pending: bool,
    /// The home restore was chosen but didn't finish: why.
    pub restore_failed: Option<String>,
    /// The username (for the restore's next step).
    pub user: String,
}

/// Only what needs a person: the usb and reboot, Secure Boot in the BIOS, a
/// home restore that didn't finish, and (as a note) the secrets that arrive by
/// themselves. GitHub, NetBird and the push are done by the installer; a push
/// that didn't happen is in [`not_pushed`].
pub fn next_steps(i: &NextInput) -> Vec<String> {
    let mut v = vec!["remove the usb stick and reboot into nixos".to_string()];
    let tpm = if i.tpm { "; the disk then unlocks itself" } else { "" };
    if i.sb_enrolled {
        v.push(format!("turn secure boot on in the bios (your keys are enrolled){tpm}"));
    } else if i.sb_carried {
        v.push(format!("turn secure boot back on in the bios: your keys are still enrolled{tpm}"));
    } else {
        v.push(format!(
            "secure boot: boot nixos once, then in the bios set secure boot to setup mode (delete keys) and boot again: \
             it enrolls your keys; then turn secure boot on{tpm}"
        ));
    }
    if let Some(why) = &i.restore_failed {
        v.push(format!(
            "/home wasn't restored from the homelab ({why}): on the new system, vx restore --restic /home/{} \
             puts the backup in /tmp/restore",
            i.user
        ));
    }
    if i.secrets_pending {
        v.push(
            "secrets: your other machines add this one automatically (or run vx secrets rekey on the mac); \
             it switches by itself once they have"
                .into(),
        );
    }
    v
}

/// Why the install commit isn't on GitHub, and what to do about it (None when pushed).
pub fn not_pushed(push: &PushState, snapshot: bool) -> Option<String> {
    let s = match push {
        PushState::Pushed(_) => return None,
        _ if snapshot => "the repo came from the usb (its own history): in ~/nixos, fetch origin and rebase the install \
                          commit onto origin/main, then push"
            .to_string(),
        PushState::Offline => "offline: committed in ~/nixos only; push later with vx sync".into(),
        PushState::Off => "you chose not to push: push from ~/nixos when ready".into(),
        PushState::Failed(e) => format!("the push failed ({e}): push from ~/nixos later"),
        PushState::NotPushed(why) => format!("{why}: push from ~/nixos later"),
    };
    Some(s)
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
    fn only_what_needs_a_person() {
        let v = next_steps(&NextInput { sb_enrolled: true, tpm: true, secrets_pending: true, ..Default::default() });
        assert_eq!(
            v,
            [
                "remove the usb stick and reboot into nixos",
                "turn secure boot on in the bios (your keys are enrolled); the disk then unlocks itself",
                "secrets: your other machines add this one automatically (or run vx secrets rekey on the mac); it switches by itself once they have",
            ]
        );
        let all = v.join(" ");
        assert!(!all.contains("netbird") && !all.contains("gh auth") && !all.contains("add-host") && !all.contains("push"));
    }

    #[test]
    fn restore_left_for_later() {
        let v = next_steps(&NextInput { restore_failed: Some("can't reach the backup server".into()), user: "vainnor".into(), ..Default::default() });
        assert_eq!(v.len(), 3);
        assert!(v[2].starts_with("/home wasn't restored from the homelab (can't reach the backup server)"), "{}", v[2]);
        assert!(v[2].contains("vx restore --restic /home/vainnor"));
        assert_eq!(next_steps(&NextInput::default()).len(), 2, "restored, or not chosen: nothing said");
    }

    #[test]
    fn secure_boot_cases() {
        let v = next_steps(&NextInput { sb_carried: true, ..Default::default() });
        assert_eq!(v[1], "turn secure boot back on in the bios: your keys are still enrolled");
        assert_eq!(v.len(), 2, "secrets off or carried: nothing said");
        let v = next_steps(&NextInput::default());
        assert!(v[1].starts_with("secure boot: boot nixos once, then in the bios set secure boot to setup mode"));
        // Carried keys enrolled again because the firmware was in setup mode.
        let v = next_steps(&NextInput { sb_carried: true, sb_enrolled: true, ..Default::default() });
        assert_eq!(v[1], "turn secure boot on in the bios (your keys are enrolled)");
    }

    #[test]
    fn why_not_pushed() {
        assert_eq!(not_pushed(&PushState::Pushed("vainnor/nixos main".into()), false), None);
        assert!(not_pushed(&PushState::Offline, false).unwrap().contains("vx sync"));
        assert!(not_pushed(&PushState::Failed("rejected".into()), false).unwrap().contains("push from ~/nixos"));
        assert!(not_pushed(&PushState::NotPushed("x".into()), true).unwrap().contains("its own history"));
    }
}
