//! Off the welcome screen: connect to Wi-Fi. The network is saved, and the
//! new system joins it from its first boot too.

use super::super::super::engine::wifi::{Network, Security};
use super::super::app::{Act, App, Field};
use super::super::form::{Row, button_row, input_row, pad, radio, truncate, wrap};
use super::{mark, para};
use ratatui::text::{Line, Span};

/// At most this many networks are listed (strongest first).
const SHOWN: usize = 12;

pub fn rows(app: &App, w: usize) -> Vec<Row> {
    let t = &app.theme;
    let mut r = vec![Row::heading(t, "connect to wi-fi"), Row::blank()];
    if let Some(ssid) = &app.wifi.connected {
        let online = app.machine.as_ref().is_some_and(|m| m.online);
        r.push(mark(t, 0, format!("connected to {ssid}")));
        r.push(if online { mark(t, 0, "online") } else { mark(t, 1, "no internet yet: try another network, or go back and use the usb's copy") });
        r.push(Row::blank());
        r.extend(para(t, "the new system joins this network on its first boot too.", w));
        r.push(Row::blank());
        r.push(button_row(t, Field::WifiBack, "back to the start"));
        r.push(button_row(t, Field::WifiRescan, "pick another network"));
        return r;
    }
    r.extend(para(t, "pick your network and enter its password. the new system joins it on its first boot too.", w));
    r.push(Row::blank());
    match &app.wifi.networks {
        None => r.push(Row::muted(t, "scanning ...")),
        Some(n) if n.is_empty() => r.push(Row::muted(t, "no networks found: rescan, or plug in ethernet")),
        Some(n) => {
            let name_w = n.iter().take(SHOWN).map(|x| x.ssid.chars().count()).max().unwrap_or(8).clamp(8, w.saturating_sub(26).max(8));
            for (i, x) in n.iter().take(SHOWN).enumerate() {
                r.push(network_row(app, i, x, name_w));
            }
            if n.len() > SHOWN {
                r.push(Row::muted(t, format!("  and {} weaker ones", n.len() - SHOWN)));
            }
        }
    }
    r.push(Row::blank());
    if let Some(n) = app.wifi.selected() {
        match n.security {
            Security::Psk | Security::Sae => {
                r.push(input_row(t, Field::WifiPw, "password", &app.wifi.pw, app.focus == Some(Field::WifiPw), None));
            }
            Security::Unsupported => {
                r.extend(para(t, "this network needs a login the installer can't do: quit (q), run nmtui, then vx-installer.", w));
            }
            Security::Open => r.push(Row::muted(t, "open network: no password")),
        }
    }
    if let Some(e) = &app.wifi.error {
        for l in wrap(e, w.saturating_sub(1)) {
            r.push(Row::colored(t.red, l));
        }
    }
    r.push(Row::blank());
    let can = app.wifi.selected().is_some_and(|n| n.security != Security::Unsupported);
    r.push(button_row(t, Field::WifiConnect, "connect").disabled(!can));
    r.push(button_row(t, Field::WifiRescan, "rescan"));
    r.push(button_row(t, Field::WifiBack, "back"));
    if !app.backend.is_real() {
        r.push(Row::blank());
        r.push(Row::muted(t, format!("{}: made-up networks; any password of 8+ characters works", app.backend.label())));
    }
    r
}

fn network_row(app: &App, i: usize, n: &Network, name_w: usize) -> Row {
    let t = &app.theme;
    let bars = match n.signal {
        75.. => "▂▄▆█",
        50..=74 => "▂▄▆ ",
        25..=49 => "▂▄  ",
        _ => "▂   ",
    };
    let bars = if t.truecolor { bars.to_string() } else { format!("{:>3}%", n.signal) };
    let sec = match n.security {
        Security::Open => "open",
        Security::Psk => "wpa2",
        Security::Sae => "wpa3",
        Security::Unsupported => "enterprise",
    };
    Row::field(
        Field::WifiNet(i),
        Line::from(vec![
            Span::styled(format!("  {}", radio(app.wifi.sel == Some(i))), t.accent()),
            Span::styled(format!(" {}", pad(&truncate(&n.ssid, name_w), name_w)), if n.active { t.accent() } else { t.text() }),
            Span::styled(format!("  {bars}"), t.accent()),
            Span::styled(format!("  {sec}"), t.muted()),
            Span::styled(if n.active { "  connected".to_string() } else { String::new() }, t.fg(t.green)),
        ]),
    )
}

pub fn act(app: &mut App, f: Field, a: Act) {
    if !matches!(a, Act::Select | Act::Toggle) {
        return;
    }
    match f {
        Field::WifiOpen => app.wifi_open(),
        Field::WifiNet(i) => app.wifi_pick(i),
        Field::WifiConnect => app.wifi_connect(),
        Field::WifiRescan => {
            app.wifi.connected = None;
            app.wifi.error = None;
            app.wifi_scan();
        }
        Field::WifiBack => app.back(),
        _ => {}
    }
}
