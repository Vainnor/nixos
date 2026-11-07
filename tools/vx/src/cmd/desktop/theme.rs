//! `vx theme show` and `vx theme accent`.

use super::palette::{self, Grade};
use super::{swatch, vx_switch};
use crate::ctx::Ctx;
use crate::ui;
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::path::{Path, PathBuf};

const ROLES: [(&str, &str); 18] = [
    ("base00", "background"),
    ("base01", "panels, bar"),
    ("base02", "selection, raised"),
    ("base03", "comments, muted"),
    ("base04", "dim text"),
    ("base05", "text"),
    ("base06", "light"),
    ("base07", "lightest"),
    ("base08", "red"),
    ("base09", "orange"),
    ("base0A", "yellow"),
    ("base0B", "green"),
    ("base0C", "cyan"),
    ("base0D", "accent"),
    ("base0E", "purple"),
    ("base0F", "extra"),
    ("border", "unfocused borders"),
    ("accent2", "second accent"),
];

/// The theme file for a name (or the live one), and whether it's live.
fn resolve(repo: &Path, name: Option<&str>) -> Result<(String, PathBuf, bool)> {
    let live = palette::live(repo);
    let all = palette::files(repo)?;
    let found = match name {
        Some(n) => all.into_iter().find(|(m, _)| m == n),
        None => all.into_iter().find(|(_, f)| live.is(f)),
    };
    match (found, name) {
        (Some((n, f)), _) => {
            let is_live = live.is(&f);
            Ok((n, f, is_live))
        }
        (None, Some(n)) => bail!("no theme '{n}' (assets/themes/{n}.nix or assets/wallpapers/{n}/theme.nix)"),
        (None, None) => bail!(
            "the live theme ({}) has no palette file here; it's a base16 scheme. name one: vx theme show <name>",
            live.theme.as_deref().unwrap_or("unset")
        ),
    }
}

/// `vx theme show [name]`: every slot as a swatch.
pub fn show(ctx: &Ctx, name: Option<&str>, raw: bool) -> Result<()> {
    let repo = ctx.repo()?;
    let (n, f, is_live) = resolve(repo, name)?;
    let live = palette::live(repo);
    let pal = palette::load(&f, if is_live { live.over.as_ref() } else { None }, raw)?;
    let mut flags = vec![];
    if pal.has_over {
        flags.push("overrides");
    }
    if is_live && !raw && live.over.as_ref().is_some_and(|o| !o.is_empty()) {
        flags.push("+meta.themeOverrides");
    }
    if is_live {
        flags.push("live");
    }
    ui::heading(&if flags.is_empty() { n.clone() } else { format!("{n} [{}]", flags.join(", ")) });
    let color = palette::color_out();
    for (k, role) in ROLES {
        let v = pal.get(k).filter(|v| !v.is_empty());
        let sw = match (v, color) {
            (Some(v), true) => swatch(v).repeat(3),
            _ => String::new(),
        };
        println!("  {sw}{}{k:<8} {:<7} {role}", if color { " " } else { "" }, v.unwrap_or("-"));
    }
    if color {
        // The palette in use: text in each colour on the background.
        let bg = palette::rgb(pal.get("base00").unwrap_or("000000"));
        let mut line = format!("\x1b[48;2;{};{};{}m ", bg[0], bg[1], bg[2]);
        for k in ["base05", "base04", "base03", "base08", "base09", "base0A", "base0B", "base0C", "base0D", "base0E"] {
            let c = palette::rgb(pal.get(k).unwrap_or("FFFFFF"));
            line += &format!("\x1b[38;2;{};{};{}m{k} ", c[0], c[1], c[2]);
        }
        println!("\n  {line}\x1b[0m");
    }
    Ok(())
}

/// What `vx theme accent <value>` sets base0D to (None = remove the override).
fn accent_value(repo: &Path, v: &str) -> Result<Option<String>> {
    if v == "reset" {
        return Ok(None);
    }
    if palette::is_hex(v) {
        return Ok(Some(v.trim_start_matches('#').to_uppercase()));
    }
    let slot = Regex::new(r"(?i)^base0([0-9a-f])$").unwrap();
    if let Some(c) = slot.captures(v) {
        let k = format!("base0{}", c[1].to_uppercase());
        let (n, f, _) = resolve(repo, None)?;
        let pal = palette::load(&f, palette::live(repo).over.as_ref(), false)?;
        let hex = pal.get(&k).filter(|h| !h.is_empty()).with_context(|| format!("{n} has no {k}"))?;
        return Ok(Some(hex.to_string()));
    }
    bail!("'{v}' is not a colour: give hex (E0A040), a slot (base0E) or reset")
}

/// meta.nix with `key` set (or removed) inside `themeOverrides = { ... };`.
pub fn set_override(text: &str, key: &str, value: Option<&str>) -> Result<String> {
    let re = Regex::new(r"(?ms)^([ \t]*)themeOverrides\s*=\s*\{(.*?)\};").unwrap();
    let caps = re.captures(text).context("no `themeOverrides = { ... };` in modules/flake/meta.nix")?;
    let indent = caps.get(1).unwrap().as_str();
    let body = caps.get(2).unwrap();
    let entry_re = Regex::new(&format!(r#"^([^#\n]*?)\b{key}\s*=\s*"[^"]*"\s*;([ \t]*)"#)).unwrap();
    let entry = value.map(|v| format!("{key} = \"{v}\";"));

    let mut found = false;
    let mut lines = vec![];
    for line in body.as_str().split('\n') {
        match entry_re.captures(line) {
            Some(c) if !found => {
                found = true;
                let m = c.get(0).unwrap();
                let rest = &line[m.end()..];
                match &entry {
                    Some(e) => lines.push(format!("{}{e}{}{rest}", &c[1], &c[2])),
                    None => {
                        let l = format!("{}{rest}", &c[1]);
                        if !l.trim().is_empty() || line.trim().is_empty() {
                            lines.push(l);
                        }
                    }
                }
            }
            _ => lines.push(line.to_string()),
        }
    }
    let mut new = lines.join("\n");
    if !found && let Some(e) = &entry {
        new = if new.trim().is_empty() {
            format!(" {e} ")
        } else if new.contains('\n') {
            format!("\n{indent}  {e}{new}")
        } else {
            format!(" {e}{new}")
        };
    }
    if new.trim().is_empty() {
        new = " ".into();
    }
    let mut out = text.to_string();
    out.replace_range(body.range(), &new);
    Ok(out)
}

/// `vx theme accent <hex|slot|reset>`.
pub fn accent(ctx: &Ctx, v: &str, yes: bool) -> Result<()> {
    let repo = ctx.repo()?;
    let value = accent_value(repo, v)?;
    let meta = palette::meta_path(repo);
    let text = std::fs::read_to_string(&meta).with_context(|| format!("could not read {}", meta.display()))?;
    let new = set_override(&text, "base0D", value.as_deref())?;
    if new == text {
        ui::info("nothing to change");
        return Ok(());
    }
    std::fs::write(&meta, &new).with_context(|| format!("could not write {}", meta.display()))?;
    match &value {
        Some(h) => ui::ok(&format!("accent is {h} (meta.themeOverrides.base0D)")),
        None => ui::ok("accent override removed; the theme's own accent is back"),
    }

    // How the new accent reads on the live palette.
    if let Ok((_, f, _)) = resolve(repo, None)
        && let Ok(pal) = palette::load(&f, palette::live(repo).over.as_ref(), false)
        && pal.missing().is_empty()
    {
        for r in palette::rows(&pal.p).iter().filter(|r| r.label.starts_with("accent")) {
            let line = format!("{:<14}{}  {:<8} {}", r.label, r.value.trim(), r.threshold, r.note);
            match r.grade {
                Grade::Pass => ui::ok(&line),
                Grade::Warn => ui::warn(&line),
                Grade::Fail => ui::warn(&format!("{line} (too low; vx theme check)")),
            }
        }
    }

    if yes || ui::confirm("switch now?") {
        vx_switch(ctx)?;
    } else {
        ui::dim("apply it later with: vx switch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = "  config.meta = {\n    theme = \"match\";\n    # e.g. { base0D = \"E0A040\"; }\n    themeOverrides = { };\n    audioOutput = null;\n  };\n";

    #[test]
    fn set_on_empty() {
        let out = set_override(META, "base0D", Some("E0A040")).unwrap();
        assert!(out.contains("    themeOverrides = { base0D = \"E0A040\"; };\n"), "{out}");
        assert!(out.contains("# e.g. { base0D = \"E0A040\"; }"));
        let back = set_override(&out, "base0D", None).unwrap();
        assert_eq!(back, META);
    }

    #[test]
    fn replace_and_keep_others() {
        let m = "themeOverrides = { border = \"111111\"; base0D = \"222222\"; };";
        let out = set_override(m, "base0D", Some("333333")).unwrap();
        assert_eq!(out, "themeOverrides = { border = \"111111\"; base0D = \"333333\"; };");
        let out = set_override(m, "base0D", None).unwrap();
        assert_eq!(out, "themeOverrides = { border = \"111111\"; };");
        let m = "themeOverrides = { border = \"111111\"; };";
        let out = set_override(m, "base0D", Some("333333")).unwrap();
        assert_eq!(out, "themeOverrides = { base0D = \"333333\"; border = \"111111\"; };");
    }

    #[test]
    fn multiline() {
        let m = "    themeOverrides = {\n      border = \"111111\"; # edge\n      base0D = \"222222\";\n    };\n";
        let out = set_override(m, "base0D", None).unwrap();
        assert_eq!(out, "    themeOverrides = {\n      border = \"111111\"; # edge\n    };\n");
        let out2 = set_override(&out, "base0D", Some("444444")).unwrap();
        assert_eq!(out2, "    themeOverrides = {\n      base0D = \"444444\";\n      border = \"111111\"; # edge\n    };\n");
        // commented-out entries are left alone
        let m = "themeOverrides = {\n  # base0D = \"999999\";\n};";
        let out = set_override(m, "base0D", Some("AAAAAA")).unwrap();
        assert_eq!(out, "themeOverrides = {\n  base0D = \"AAAAAA\";\n  # base0D = \"999999\";\n};");
    }

    #[test]
    fn missing_block_errors() {
        assert!(set_override("nothing here", "base0D", Some("000000")).is_err());
    }
}
