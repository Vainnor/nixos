//! Colours: the default wallpaper's palette from install-data, or ANSI 16
//! colours on the raw Linux console (TERM=linux has no truecolour).

use super::super::plan::InstallData;
use ratatui::style::{Color, Modifier, Style};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub border: Color,
    pub muted: Color,
    pub text: Color,
    pub accent: Color,
    /// Text drawn on the accent fill.
    pub on_accent: Color,
    pub red: Color,
    pub yellow: Color,
    pub green: Color,
    pub truecolor: bool,
}

pub fn hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

/// The raw console (no truecolour, a small font).
pub fn console_is_ansi() -> bool {
    std::env::var("TERM").is_ok_and(|t| t == "linux")
}

const ANSI: [(Color, (u8, u8, u8)); 8] = [
    (Color::Black, (0, 0, 0)),
    (Color::Red, (170, 0, 0)),
    (Color::Green, (0, 170, 0)),
    (Color::Yellow, (170, 85, 0)),
    (Color::Blue, (0, 0, 170)),
    (Color::Magenta, (170, 0, 170)),
    (Color::Cyan, (0, 170, 170)),
    (Color::Gray, (170, 170, 170)),
];

impl Theme {
    pub fn from_colors(c: &BTreeMap<String, String>) -> Option<Theme> {
        let get = |k: &str| c.get(k).and_then(|v| hex(v)).map(|(r, g, b)| Color::Rgb(r, g, b));
        let bg = get("base00")?;
        Some(Theme {
            bg,
            surface: get("base01").unwrap_or(bg),
            border: get("border").or_else(|| get("base02")).unwrap_or(bg),
            muted: get("base03")?,
            text: get("base05")?,
            accent: get("base0D")?,
            on_accent: bg,
            red: get("base08")?,
            yellow: get("base0A")?,
            green: get("base0B")?,
            truecolor: true,
        })
    }

    /// Vainnor slate: used until install-data is loaded.
    pub fn slate() -> Theme {
        Theme {
            bg: Color::Rgb(0x10, 0x14, 0x1B),
            surface: Color::Rgb(0x16, 0x1B, 0x24),
            border: Color::Rgb(0x35, 0x46, 0x5C),
            muted: Color::Rgb(0x6C, 0x7A, 0x8F),
            text: Color::Rgb(0xD3, 0xD9, 0xE2),
            accent: Color::Rgb(0xB5, 0xA8, 0xF5),
            on_accent: Color::Rgb(0x10, 0x14, 0x1B),
            red: Color::Rgb(0xE5, 0x67, 0x5F),
            yellow: Color::Rgb(0xE6, 0xA5, 0x2E),
            green: Color::Rgb(0x94, 0xC4, 0x7D),
            truecolor: true,
        }
    }

    pub fn ansi() -> Theme {
        Theme {
            bg: Color::Reset,
            surface: Color::Black,
            border: Color::DarkGray,
            muted: Color::DarkGray,
            text: Color::Gray,
            accent: Color::Cyan,
            on_accent: Color::Black,
            red: Color::LightRed,
            yellow: Color::Yellow,
            green: Color::Green,
            truecolor: false,
        }
    }

    /// The default wallpaper's palette (falling back to the first wallpaper, then slate).
    pub fn pick(data: Option<&InstallData>, ansi: bool) -> Theme {
        if ansi {
            return Theme::ansi();
        }
        let Some(d) = data else { return Theme::slate() };
        d.wallpapers
            .iter()
            .find(|w| w.name == d.defaults.wallpaper)
            .or(d.wallpapers.first())
            .and_then(|w| Theme::from_colors(&w.colors))
            .unwrap_or_else(Theme::slate)
    }

    /// A palette colour for a swatch (nearest ANSI background colour on the console).
    pub fn swatch(&self, hexs: &str) -> Color {
        let Some((r, g, b)) = hex(hexs) else { return self.muted };
        if self.truecolor {
            return Color::Rgb(r, g, b);
        }
        let d = |(x, y, z): (u8, u8, u8)| {
            let s = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
            s(r, x) + s(g, y) + s(b, z)
        };
        ANSI.iter().min_by_key(|(_, c)| d(*c)).map(|(c, _)| *c).unwrap_or(Color::Black)
    }

    pub fn base(&self) -> Style {
        Style::new().fg(self.text).bg(self.bg)
    }
    pub fn fg(&self, c: Color) -> Style {
        Style::new().fg(c)
    }
    pub fn muted(&self) -> Style {
        Style::new().fg(self.muted)
    }
    pub fn text(&self) -> Style {
        Style::new().fg(self.text)
    }
    pub fn accent(&self) -> Style {
        Style::new().fg(self.accent)
    }
    pub fn bold(&self) -> Style {
        Style::new().fg(self.text).add_modifier(Modifier::BOLD)
    }
    pub fn selected(&self) -> Style {
        Style::new().fg(self.on_accent).bg(self.accent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_mapping() {
        let mut c = BTreeMap::new();
        for (k, v) in [
            ("base00", "0A0A0C"),
            ("base01", "17171C"),
            ("base02", "202024"),
            ("base03", "6E6E77"),
            ("base05", "D8D8DA"),
            ("base08", "D47D7D"),
            ("base0A", "D1B675"),
            ("base0B", "91C577"),
            ("base0D", "5B9BEA"),
        ] {
            c.insert(k.to_string(), v.to_string());
        }
        let t = Theme::from_colors(&c).unwrap();
        assert_eq!(t.bg, Color::Rgb(0x0A, 0x0A, 0x0C));
        assert_eq!(t.border, Color::Rgb(0x20, 0x20, 0x24), "base02 without a border key");
        c.insert("border".into(), "38383F".into());
        assert_eq!(Theme::from_colors(&c).unwrap().border, Color::Rgb(0x38, 0x38, 0x3F));
        assert_eq!(Theme::ansi().swatch("D47D7D"), Color::Gray);
        assert_eq!(Theme::ansi().swatch("0A0A0C"), Color::Black);
    }
}
