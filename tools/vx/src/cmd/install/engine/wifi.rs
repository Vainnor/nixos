//! Wi-Fi on the installer: list the networks around, and connect to one.
//!
//! The installer ISO runs NetworkManager (as the installed system does). To
//! connect, we write a NetworkManager keyfile to [`CONNECTIONS`] (0600, the
//! password never on a command line), `nmcli connection reload`, then
//! `nmcli connection up uuid <uuid>`. That same file is copied into the new
//! system's /persistent at the restore step, so the PC is on the same Wi-Fi
//! from its first boot.
//!
//! `vx iso --wifi` writes a keyfile the same way ([`keyfile`]) and bakes it
//! into the ISO, so the stick connects by itself on boot.

use super::super::plan::Secret;
use super::runner::{Cmd, Runner};
use anyhow::{Result, bail};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// NetworkManager's saved connections, on the ISO and on the installed system.
pub const CONNECTIONS: &str = "/etc/NetworkManager/system-connections";

/// How a network is secured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    Open,
    /// WPA/WPA2 personal (also WPA2/WPA3 transition networks).
    Psk,
    /// WPA3 personal only.
    Sae,
    /// 802.1X / WEP: not handled here (use nmtui).
    Unsupported,
}

impl Security {
    /// From nmcli's SECURITY column ("WPA2", "WPA1 WPA2", "WPA3", "WPA2 802.1X", "" ...).
    pub fn from_nmcli(s: &str) -> Security {
        let s = s.trim();
        if s.is_empty() || s == "--" {
            Security::Open
        } else if s.contains("802.1X") || s.contains("WEP") {
            Security::Unsupported
        } else if s.contains("WPA1") || s.contains("WPA2") {
            Security::Psk
        } else if s.contains("WPA3") {
            Security::Sae
        } else if s.contains("WPA") {
            Security::Psk
        } else {
            Security::Unsupported
        }
    }

    /// The keyfile's `key-mgmt`, None for an open network.
    fn key_mgmt(self) -> Option<&'static str> {
        match self {
            Security::Psk => Some("wpa-psk"),
            Security::Sae => Some("sae"),
            Security::Open | Security::Unsupported => None,
        }
    }

    pub fn needs_password(self) -> bool {
        matches!(self, Security::Psk | Security::Sae)
    }

    /// nmcli `802-11-wireless-security.key-mgmt` of a saved connection -> Security.
    pub fn from_key_mgmt(k: &str) -> Security {
        match k.trim() {
            "" => Security::Open,
            "wpa-psk" => Security::Psk,
            "sae" => Security::Sae,
            _ => Security::Unsupported,
        }
    }
}

/// A network from the scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub ssid: String,
    /// 0-100.
    pub signal: u8,
    pub security: Security,
    /// Connected to it right now.
    pub active: bool,
}

/// The networks around, strongest first (one entry per name; hidden ones left out).
pub fn scan(r: &Runner) -> Result<Vec<Network>> {
    let out = r.query(
        Cmd::new("nmcli")
            .args(["-t", "-f", "IN-USE,SIGNAL,SECURITY,SSID", "device", "wifi", "list", "--rescan", "yes"])
            .fake(":82:WPA2:home\n:61:WPA2 WPA3:home-5g\n:40::cafe guest\n:22:WPA2 802.1X:work\n"),
    )?;
    Ok(parse_scan(&out))
}

/// `nmcli -t -f IN-USE,SIGNAL,SECURITY,SSID device wifi list` -> networks.
pub fn parse_scan(out: &str) -> Vec<Network> {
    let mut v: Vec<Network> = vec![];
    for l in out.lines() {
        let f = split_terse(l);
        let [in_use, signal, security, ssid] = f.as_slice() else { continue };
        if ssid.is_empty() {
            continue;
        }
        let n = Network {
            ssid: ssid.clone(),
            signal: signal.parse().unwrap_or(0),
            security: Security::from_nmcli(security),
            active: in_use.trim() == "*",
        };
        match v.iter_mut().find(|x| x.ssid == n.ssid) {
            Some(x) => {
                x.active |= n.active;
                if n.signal > x.signal {
                    x.signal = n.signal;
                    x.security = n.security;
                }
            }
            None => v.push(n),
        }
    }
    v.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    v
}

/// One line of nmcli's terse output: fields split on unescaped ':' (`\:` and `\\` unescaped).
pub fn split_terse(l: &str) -> Vec<String> {
    let mut v = vec![String::new()];
    let mut it = l.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                if let Some(n) = it.next() {
                    v.last_mut().unwrap().push(n);
                }
            }
            ':' => v.push(String::new()),
            c => v.last_mut().unwrap().push(c),
        }
    }
    v
}

/// Connect to `n` (password for a secured one). On success the keyfile stays
/// in [`CONNECTIONS`]; on failure it's removed again. Waits until the
/// connection is up (not for the internet: the caller checks that).
pub fn connect(r: &Runner, n: &Network, password: Option<&Secret>) -> Result<()> {
    if n.security == Security::Unsupported {
        bail!("{} needs a login this installer can't do: quit, connect with nmtui, then run vx-installer", n.ssid);
    }
    let psk = password.map(|p| p.0.as_str()).filter(|p| !p.is_empty());
    if n.security.needs_password() {
        match psk {
            None => bail!("enter the wi-fi password"),
            Some(p) if p.len() < 8 || p.len() > 63 => bail!("a wi-fi password is 8 to 63 characters"),
            _ => {}
        }
    }
    let uuid = new_uuid();
    let file = keyfile_path(&n.ssid);
    r.mkdir(Path::new(CONNECTIONS), 0o700)?;
    r.write_secret(&file, keyfile(&n.ssid, n.security, psk, &uuid).as_bytes(), 0o600)?;
    let up = (|| -> Result<()> {
        r.run(Cmd::new("nmcli").args(["connection", "reload"]))?;
        let (ok, _, tail) = r.output(&Cmd::new("nmcli").args(["--wait", "45", "connection", "up", "uuid", &uuid]).fake(""))?;
        if !ok {
            bail!("{}", explain(&tail));
        }
        Ok(())
    })();
    if up.is_err() {
        let _ = r.remove(&file);
        let _ = r.run(Cmd::new("nmcli").args(["connection", "reload"]).quiet());
    }
    up
}

/// nmcli's failure in a few words.
fn explain(tail: &str) -> String {
    let t = tail.to_lowercase();
    if t.contains("secrets were required") || t.contains("802-1x supplicant") || t.contains("wrong password") {
        "wrong password".into()
    } else if t.contains("no network with ssid") || t.contains("not found") {
        "network not found: move closer, or rescan".into()
    } else if t.contains("timeout") || t.contains("timed out") {
        "timed out: the network didn't answer".into()
    } else {
        let last = tail.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("nmcli failed").trim();
        last.trim_start_matches("Error: ").to_string()
    }
}

/// Wait up to `secs` for the internet (DHCP and DNS after the link comes up).
pub fn wait_online(r: &Runner, secs: u64, online: impl Fn() -> bool) -> bool {
    if r.dry() {
        return true;
    }
    let until = Instant::now() + Duration::from_secs(secs);
    loop {
        if online() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(700));
    }
}

/// Where the keyfile for `ssid` goes.
pub fn keyfile_path(ssid: &str) -> PathBuf {
    Path::new(CONNECTIONS).join(format!("{}.nmconnection", file_stem(ssid)))
}

/// `wifi-<ssid>` with anything but [A-Za-z0-9._-] made '_'.
pub fn file_stem(ssid: &str) -> String {
    let s: String = ssid.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    format!("wifi-{s}")
}

/// A NetworkManager keyfile for a Wi-Fi network that connects by itself.
pub fn keyfile(ssid: &str, security: Security, psk: Option<&str>, uuid: &str) -> String {
    let mut s = format!(
        "[connection]\nid={}\nuuid={uuid}\ntype=wifi\nautoconnect=true\n\n[wifi]\nmode=infrastructure\nssid={}\n",
        escape(ssid),
        ssid_value(ssid)
    );
    if let Some(k) = security.key_mgmt() {
        s.push_str(&format!("\n[wifi-security]\nkey-mgmt={k}\n"));
        if let Some(p) = psk {
            s.push_str(&format!("psk={}\n", escape(p)));
        }
    }
    s.push_str("\n[ipv4]\nmethod=auto\n\n[ipv6]\naddr-gen-mode=default\nmethod=auto\n");
    s
}

/// A keyfile string value (GKeyFile escapes).
fn escape(v: &str) -> String {
    let mut o = String::new();
    for (i, c) in v.chars().enumerate() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            ' ' if i == 0 => o.push_str("\\s"),
            c => o.push(c),
        }
    }
    o
}

/// The ssid: as text, or (when it has ';' or isn't plain printable) as NetworkManager's byte list.
fn ssid_value(ssid: &str) -> String {
    let plain = ssid.chars().all(|c| !c.is_control() && c != ';' && c != '\\') && !ssid.starts_with(' ');
    if plain {
        ssid.to_string()
    } else {
        ssid.bytes().map(|b| format!("{b};")).collect()
    }
}

/// A random (v4) UUID for a new connection.
pub fn new_uuid() -> String {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        b = (n ^ (std::process::id() as u128) << 64).to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The Wi-Fi keyfiles on this machine (to copy into the new system).
pub fn saved_keyfiles() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(CONNECTIONS) else { return vec![] };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "nmconnection"))
        .filter(|p| std::fs::read_to_string(p).is_ok_and(|s| is_wifi_keyfile(&s)))
        .collect();
    v.sort();
    v
}

pub fn is_wifi_keyfile(s: &str) -> bool {
    s.lines().any(|l| matches!(l.trim(), "type=wifi" | "type=802-11-wireless"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_parses_and_merges() {
        let out = "*:70:WPA2:home\n:82:WPA2:home\n:40::cafe\\: guest\n:30:WPA2 802.1X:work\n:20:WPA2:\n:55:WPA3:new\n";
        let n = parse_scan(out);
        assert_eq!(n.iter().map(|x| x.ssid.as_str()).collect::<Vec<_>>(), ["home", "new", "cafe: guest", "work"]);
        assert!(n[0].active && n[0].signal == 82);
        assert_eq!(n[1].security, Security::Sae);
        assert_eq!(n[2].security, Security::Open);
        assert_eq!(n[3].security, Security::Unsupported);
        assert_eq!(Security::from_nmcli("WPA2 WPA3"), Security::Psk);
    }

    #[test]
    fn keyfile_shape() {
        let k = keyfile("home", Security::Psk, Some("pa\\ss word"), "u-1");
        assert!(k.contains("id=home\nuuid=u-1\ntype=wifi\nautoconnect=true\n"), "{k}");
        assert!(k.contains("[wifi]\nmode=infrastructure\nssid=home\n"));
        assert!(k.contains("[wifi-security]\nkey-mgmt=wpa-psk\npsk=pa\\\\ss word\n"));
        let open = keyfile("cafe", Security::Open, None, "u");
        assert!(!open.contains("wifi-security"));
        assert!(keyfile("a;b", Security::Sae, Some("x"), "u").contains("ssid=97;59;98;\n"));
        assert!(is_wifi_keyfile(&k));
        assert_eq!(file_stem("my net/5G"), "wifi-my_net_5G");
    }

    #[test]
    fn uuid_shape() {
        let u = new_uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_ne!(u, new_uuid());
    }

    #[test]
    fn password_stays_off_the_command_line() {
        let (tx, rx) = std::sync::mpsc::channel();
        let r = Runner::new(super::super::Mode::DryRun, Some(tx));
        let n = Network { ssid: "home".into(), signal: 80, security: Security::Psk, active: false };
        connect(&r, &n, Some(&Secret("hunter2hunter2".into()))).unwrap();
        drop(r);
        let logs: Vec<String> = rx.try_iter().filter_map(|e| if let super::super::Event::Log(l) = e { Some(l) } else { None }).collect();
        assert!(logs.iter().all(|l| !l.contains("hunter2")), "{logs:?}");
        assert!(logs.iter().any(|l| l.contains("connection up uuid")), "{logs:?}");
        let r = Runner::new(super::super::Mode::DryRun, None);
        assert!(connect(&r, &n, Some(&Secret("short".into()))).is_err());
    }
}
