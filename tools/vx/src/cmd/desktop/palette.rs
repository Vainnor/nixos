//! Palettes: reading the theme files (assets/themes/*.nix and
//! assets/wallpapers/*/theme.nix) the way scripts/theme-check.py does, the
//! colour maths (WCAG contrast, OKLCH), and `vx theme check`.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub const SLOTS: [&str; 16] = [
    "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08", "base09", "base0A",
    "base0B", "base0C", "base0D", "base0E", "base0F",
];
pub const EXTRA: [&str; 2] = ["border", "accent2"];

/// The `name = { ... };` attrset (flat, no nesting) as {key: value}.
pub fn block(text: &str, name: &str) -> Option<BTreeMap<String, String>> {
    let re = Regex::new(&format!(r"(?ms)^\s*{}\s*=\s*\{{(.*?)\}};", regex::escape(name))).unwrap();
    let body = re.captures(text)?.get(1)?.as_str();
    let body = Regex::new(r"#[^\n]*").unwrap().replace_all(body, "");
    let kv = Regex::new(r#"(\w+)\s*=\s*"([^"]*)"\s*;"#).unwrap();
    Some(kv.captures_iter(&body).map(|c| (c[1].to_string(), c[2].to_string())).collect())
}

/// A resolved palette: base00..base0F (whichever exist), `border`, `accent2`.
pub struct Palette {
    /// Slots plus `border`, uppercased (what theme-check checks).
    pub p: BTreeMap<String, String>,
    /// `extra.accent2`, else base0F.
    pub accent2: String,
    /// The file's own `overrides` were applied (and non-empty).
    pub has_over: bool,
}

impl Palette {
    pub fn get(&self, k: &str) -> Option<&str> {
        if k == "accent2" { Some(&self.accent2) } else { self.p.get(k).map(String::as_str) }
    }
    pub fn missing(&self) -> Vec<&'static str> {
        SLOTS.iter().copied().filter(|k| !self.p.contains_key(*k)).collect()
    }
}

/// Parse a theme file's text, laying `overrides` and then `global` over it
/// (neither with `raw`).
pub fn parse(text: &str, global: Option<&BTreeMap<String, String>>, raw: bool) -> Palette {
    let mut scheme = block(text, "scheme").unwrap_or_default();
    let mut extra = block(text, "extra").unwrap_or_default();
    let mut layers = vec![];
    if !raw {
        layers.push(block(text, "overrides").unwrap_or_default());
        if let Some(g) = global {
            layers.push(g.clone());
        }
    }
    for over in &layers {
        for (k, v) in over {
            if EXTRA.contains(&k.as_str()) { extra.insert(k.clone(), v.clone()) } else { scheme.insert(k.clone(), v.clone()) };
        }
    }
    let mut p: BTreeMap<String, String> =
        SLOTS.iter().filter_map(|k| scheme.get(*k).map(|v| (k.to_string(), v.to_uppercase()))).collect();
    let border = match extra.get("border").filter(|s| !s.is_empty()) {
        Some(b) => b.to_uppercase(),
        None => p.get("base02").cloned().unwrap_or_default(),
    };
    p.insert("border".into(), border);
    let accent2 = match extra.get("accent2").filter(|s| !s.is_empty()) {
        Some(a) => a.to_uppercase(),
        None => p.get("base0F").cloned().unwrap_or_default(),
    };
    Palette { p, accent2, has_over: layers.first().is_some_and(|l| !l.is_empty()) }
}

pub fn load(path: &Path, global: Option<&BTreeMap<String, String>>, raw: bool) -> Result<Palette> {
    let text = std::fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    Ok(parse(&text, global, raw))
}

/// Every palette, named: assets/themes/*.nix (not default.nix), then each
/// wallpaper with a theme.nix. Sorted like the Python script (by name).
pub fn files(repo: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = vec![];
    let d = repo.join("assets/themes");
    let mut names: Vec<String> = std::fs::read_dir(&d)
        .with_context(|| format!("could not read {}", d.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    for f in names {
        if let Some(n) = f.strip_suffix(".nix").filter(|_| f != "default.nix") {
            out.push((n.to_string(), d.join(&f)));
        }
    }
    let d = repo.join("assets/wallpapers");
    for n in wallpaper_names(repo)? {
        let f = d.join(&n).join("theme.nix");
        if f.is_file() {
            out.push((n, f));
        }
    }
    Ok(out)
}

/// Every folder in assets/wallpapers, sorted.
pub fn wallpaper_names(repo: &Path) -> Result<Vec<String>> {
    let d = repo.join("assets/wallpapers");
    let mut names: Vec<String> = std::fs::read_dir(&d)
        .with_context(|| format!("could not read {}", d.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

/// What meta.nix says is live.
pub struct Live {
    pub wallpaper: Option<String>,
    pub theme: Option<String>,
    /// The theme file in use (canonical path), if it's a local palette.
    pub file: Option<PathBuf>,
    /// `meta.themeOverrides`.
    pub over: Option<BTreeMap<String, String>>,
}

pub fn meta_path(repo: &Path) -> PathBuf {
    repo.join("modules/flake/meta.nix")
}

pub fn meta_string(text: &str, key: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"(?m)^\s*{key}\s*=\s*"([^"]+)"\s*;"#)).unwrap();
    re.captures(text).map(|c| c[1].to_string())
}

pub fn live(repo: &Path) -> Live {
    let Ok(text) = std::fs::read_to_string(meta_path(repo)) else {
        return Live { wallpaper: None, theme: None, file: None, over: None };
    };
    let wallpaper = meta_string(&text, "wallpaper");
    let theme = meta_string(&text, "theme");
    let over = block(&text, "themeOverrides");
    let file = theme.as_deref().map(|t| {
        let f = if t == "match" {
            repo.join("assets/wallpapers").join(wallpaper.as_deref().unwrap_or("")).join("theme.nix")
        } else {
            repo.join("assets/themes").join(format!("{t}.nix"))
        };
        std::fs::canonicalize(&f).unwrap_or(f)
    });
    Live { wallpaper, theme, file, over }
}

impl Live {
    pub fn is(&self, path: &Path) -> bool {
        let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.file.as_deref() == Some(p.as_path())
    }
}

// ── colour maths (same formulas as scripts/theme-check.py) ──────────────

fn lin(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// "RRGGBB" (optional '#') as bytes; black if malformed.
pub fn rgb(h: &str) -> [u8; 3] {
    let h = h.trim_start_matches('#');
    let at = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).unwrap_or(0);
    [at(0), at(2), at(4)]
}

pub fn is_hex(h: &str) -> bool {
    let h = h.trim_start_matches('#');
    h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn lum(h: &str) -> f64 {
    let [r, g, b] = rgb(h).map(lin);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio (symmetric).
pub fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (lum(a), lum(b));
    let (hi, lo) = if x >= y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// OKLCH: (lightness, chroma, hue in degrees 0..360).
pub fn oklch(h: &str) -> (f64, f64, f64) {
    let [r, g, b] = rgb(h).map(lin);
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let [l, m, s] = [l, m, s].map(|x| x.powf(1.0 / 3.0));
    let ll = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let bb = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    (ll, a.hypot(bb), bb.atan2(a).to_degrees().rem_euclid(360.0))
}

// ── vx theme check ──────────────────────────────────────────────────────

/// name, fg, bg, pass, warn floor (pass threshold as printed by the script).
const CONTRAST: [(&str, &str, &str, f64, f64, &str); 11] = [
    ("text/bg", "base05", "base00", 7.0, 6.0, "7.0"),
    ("dim/bg", "base04", "base00", 4.5, 4.0, "4.5"),
    ("dim/surface", "base04", "base01", 4.5, 4.0, "4.5"),
    ("muted/bg", "base03", "base00", 3.0, 2.6, "3.0"),
    ("muted/surface", "base03", "base01", 3.0, 2.6, "3.0"),
    ("accent/bg", "base0D", "base00", 4.5, 4.0, "4.5"),
    ("accent/surf", "base0D", "base01", 4.5, 4.0, "4.5"),
    ("red/bg", "base08", "base00", 4.0, 3.5, "4.0"),
    ("border/bg", "border", "base00", 1.4, 1.25, "1.4"),
    ("surface step", "base01", "base00", 1.08, 1.04, "1.08"),
    ("raised step", "base02", "base01", 1.08, 1.04, "1.08"),
];
const CHROMA: (f64, f64, &str) = (0.06, 0.035, "0.06");
const RED: (f64, f64, &str) = (0.09, 0.06, "0.09");
const ANSI_T: (f64, f64, &str) = (0.07, 0.045, "0.07");
const ANSI: [&str; 5] = ["base09", "base0A", "base0B", "base0C", "base0E"];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Grade {
    Pass,
    Warn,
    Fail,
}

impl Grade {
    fn of(v: f64, ok: f64, floor: f64) -> Self {
        if v >= ok { Grade::Pass } else if v >= floor { Grade::Warn } else { Grade::Fail }
    }
    fn word(self) -> &'static str {
        match self {
            Grade::Pass => "PASS",
            Grade::Warn => "WARN",
            Grade::Fail => "FAIL",
        }
    }
}

pub struct Row {
    pub label: &'static str,
    pub value: String,
    pub threshold: String,
    pub grade: Grade,
    pub note: String,
}

/// Every check for one palette (all 16 slots must be present).
pub fn rows(p: &BTreeMap<String, String>) -> Vec<Row> {
    let mut rows = vec![];
    for (label, fg, bg, ok, floor, th) in CONTRAST {
        let v = contrast(&p[fg], &p[bg]);
        rows.push(Row {
            label,
            value: format!("{v:5.2}"),
            threshold: format!(">= {th}"),
            grade: Grade::of(v, ok, floor),
            note: format!("{} on {}", p[fg], p[bg]),
        });
    }
    let (_, c, hue) = oklch(&p["base0D"]);
    let g = Grade::of(c, CHROMA.0, CHROMA.1);
    let note = format!("hue {hue:3.0}") + if g != Grade::Pass { "  accent has no colour" } else { "" };
    rows.push(Row { label: "accent chroma", value: format!("{c:5.3}"), threshold: format!(">= {}", CHROMA.2), grade: g, note });
    let c = oklch(&p["base08"]).1;
    rows.push(Row {
        label: "red chroma",
        value: format!("{c:5.3}"),
        threshold: format!(">= {}", RED.2),
        grade: Grade::of(c, RED.0, RED.1),
        note: p["base08"].clone(),
    });
    let (mut c, mut k) = (f64::INFINITY, ANSI[0]);
    for s in ANSI {
        let x = oklch(&p[s]).1;
        if x < c {
            (c, k) = (x, s);
        }
    }
    rows.push(Row {
        label: "colours",
        value: format!("{c:5.3}"),
        threshold: format!(">= {}", ANSI_T.2),
        grade: Grade::of(c, ANSI_T.0, ANSI_T.1),
        note: format!("lowest: {k} {}", p[k]),
    });
    rows
}

pub fn worst(rows: &[Row]) -> Grade {
    rows.iter().map(|r| r.grade).max().unwrap_or(Grade::Pass)
}

/// Should we colour stdout?
pub fn color_out() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// Port of scripts/theme-check.py: same checks, output and exit status.
/// Returns true when anything FAILed.
pub fn check(repo: &Path, names: &[String], quiet: bool, raw: bool) -> Result<bool> {
    let live = live(repo);
    let tty = color_out();
    let tag = |g: Grade| {
        let code = match g {
            Grade::Pass => "32",
            Grade::Warn => "33",
            Grade::Fail => "31",
        };
        if tty { format!("\x1b[{code}m{}\x1b[0m", g.word()) } else { g.word().to_string() }
    };
    let mut summary = vec![];
    let mut failed = false;
    for (name, path) in files(repo)? {
        if !names.is_empty() && !names.contains(&name) {
            continue;
        }
        let is_live = live.is(&path);
        let pal = load(&path, if is_live { live.over.as_ref() } else { None }, raw)?;
        let missing = pal.missing();
        if !missing.is_empty() {
            println!("{name}: missing {}", missing.join(", "));
            failed = true;
            continue;
        }
        let rows = rows(&pal.p);
        let w = worst(&rows);
        failed |= w == Grade::Fail;
        let flags = format!(
            "{}{}{}",
            if pal.has_over { " overrides" } else { "" },
            if is_live && live.over.as_ref().is_some_and(|o| !o.is_empty()) { " +meta.themeOverrides (live)" } else { "" },
            if is_live { " [live]" } else { "" }
        );
        let bad: Vec<&str> = rows.iter().filter(|r| r.grade != Grade::Pass).map(|r| r.label).collect();
        summary.push((name.clone(), w, bad.join(", ")));
        if quiet && w == Grade::Pass {
            continue;
        }
        println!("{}  {name}{flags}", tag(w));
        for r in &rows {
            if quiet && r.grade == Grade::Pass {
                continue;
            }
            println!("    {}  {:<14}{}  {:<8} {}", tag(r.grade), r.label, r.value, r.threshold, r.note);
        }
    }
    println!();
    for (name, w, bad) in summary {
        println!("  {}  {name:<14} {bad}", tag(w));
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    // Reference values printed by scripts/theme-check.py's contrast()/oklch().
    #[test]
    fn contrast_matches_python() {
        assert!(close(contrast("D8D8DA", "0A0A0C"), 13.897973037294797));
        assert!(close(contrast("FFFFFF", "000000"), 21.0));
        assert!(close(contrast("5B9BEA", "17171C"), 6.227019895318941));
        assert!(close(contrast("161B24", "10141B"), 1.0689347359766757));
    }

    #[test]
    fn oklch_matches_python() {
        let cases = [
            ("5B9BEA", 0.681504696650731, 0.13487408388100852, 254.6333972666144),
            ("B5A8F5", 0.7710561285659467, 0.10925661156555987, 291.35872771231374),
            ("D47D7D", 0.6834089939493865, 0.10841685986797456, 20.259654027332402),
            ("FF0000", 0.6279553606145516, 0.2576833077361567, 29.233885192342633),
        ];
        for (h, l, c, hue) in cases {
            let (a, b, d) = oklch(h);
            assert!(close(a, l) && close(b, c) && (d - hue).abs() < 1e-9, "{h}: {a} {b} {d}");
        }
        let (l, c, _) = oklch("808080");
        assert!(close(l, 0.5998708017071177) && c < 1e-7);
        assert_eq!(oklch("000000").0, 0.0);
    }

    #[test]
    fn grades() {
        assert_eq!(Grade::of(7.0, 7.0, 6.0), Grade::Pass);
        assert_eq!(Grade::of(6.5, 7.0, 6.0), Grade::Warn);
        assert_eq!(Grade::of(5.9, 7.0, 6.0), Grade::Fail);
    }

    const EYE: &str = r#"{
  scheme = {
    scheme = "eye";
    base00 = "0a0a0c";
    base01 = "121216"; # a comment = "XXXXXX";
    base02 = "1C1C20";
  };
  extra = {
    border = "313137";
  };
  overrides = {
    base01 = "17171C";
    border = "38383F";
  };
}"#;

    #[test]
    fn parse_applies_overrides_in_order() {
        let p = parse(EYE, None, false);
        assert_eq!(p.p["base00"], "0A0A0C");
        assert_eq!(p.p["base01"], "17171C");
        assert_eq!(p.p["border"], "38383F");
        assert_eq!(p.accent2, "");
        assert!(p.has_over);
        let g: BTreeMap<_, _> = [("base0D".to_string(), "e0a040".to_string())].into();
        let p = parse(EYE, Some(&g), false);
        assert_eq!(p.p["base0D"], "E0A040");
        let p = parse(EYE, Some(&g), true);
        assert_eq!(p.p["base01"], "121216");
        assert_eq!(p.p["border"], "313137");
        assert!(!p.p.contains_key("base0D"));
        assert!(!p.has_over);
    }

    #[test]
    fn border_falls_back_to_base02() {
        let p = parse(r#"scheme = { base02 = "abcdef"; };"#, None, false);
        assert_eq!(p.p["border"], "ABCDEF");
        assert!(!p.has_over);
    }

    #[test]
    fn empty_block() {
        assert_eq!(block("  themeOverrides = { };", "themeOverrides"), Some(BTreeMap::new()));
        assert_eq!(block("nothing", "themeOverrides"), None);
    }
}
