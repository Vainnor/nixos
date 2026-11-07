//! `vx wallpaper ...` and `vx wallpapers`.

use super::palette::{self, Palette};
use super::{swatch, vx_switch};
use crate::ctx::{self, Ctx, Host};
use crate::ui;
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Slots shown next to each name in lists and the picker.
const SHOW: [&str; 12] =
    ["base00", "base01", "base02", "base03", "base05", "base08", "base09", "base0A", "base0B", "base0C", "base0D", "base0E"];

/// Wallpapers with images (assets/wallpapers/<name>/wide.png).
fn names(repo: &Path) -> Result<Vec<String>> {
    Ok(palette::wallpaper_names(repo)?.into_iter().filter(|n| repo.join("assets/wallpapers").join(n).join("wide.png").is_file()).collect())
}

/// A wallpaper's palette as the shell's picker sees it (own and global overrides).
fn pal(repo: &Path, name: &str, live: &palette::Live) -> Option<Palette> {
    let f = repo.join("assets/wallpapers").join(name).join("theme.nix");
    palette::load(&f, live.over.as_ref(), false).ok()
}

fn strip(pal: &Option<Palette>, color: bool) -> String {
    match pal {
        Some(p) if color => SHOW.iter().map(|k| swatch(p.get(k).unwrap_or(""))).collect(),
        Some(p) => p.get("base0D").map(|a| format!("accent {a}")).unwrap_or_default(),
        None => "(no theme.nix)".into(),
    }
}

/// `vx wallpapers`: list, current one marked.
pub fn list(ctx: &Ctx) -> Result<()> {
    let repo = ctx.repo()?;
    let live = palette::live(repo);
    let color = palette::color_out();
    // Same as the recipe: every folder, even ones without images.
    for n in palette::wallpaper_names(repo)? {
        let mark = if live.wallpaper.as_deref() == Some(n.as_str()) { "*" } else { " " };
        let p = pal(repo, &n, &live);
        println!("{mark} {n:<14} {}", strip(&p, color));
    }
    Ok(())
}

/// Ask which wallpaper; None if the user backs out.
fn pick(ctx: &Ctx, repo: &Path) -> Result<Option<String>> {
    let live = palette::live(repo);
    let all = names(repo)?;
    if all.is_empty() {
        bail!("no wallpapers in assets/wallpapers (add one: vx wallpaper add <name> <wide> <uhd>)");
    }
    let lines: Vec<String> = all
        .iter()
        .map(|n| {
            let mark = if live.wallpaper.as_deref() == Some(n.as_str()) { "*" } else { " " };
            format!("{n:<14} {mark} {}", strip(&pal(repo, n, &live), true))
        })
        .collect();

    if ctx::has("fzf") && std::io::stdin().is_terminal() {
        let mut c = ctx.cmd("fzf");
        c.args(["--ansi", "--prompt", "wallpaper > ", "--height", "~20", "--reverse", "--no-sort"]);
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = c.spawn().context("could not start fzf")?;
        child.stdin.take().unwrap().write_all(lines.join("\n").as_bytes())?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Ok(None);
        }
        let s = String::from_utf8_lossy(&out.stdout);
        return Ok(s.split_whitespace().next().map(str::to_string));
    }

    if !std::io::stdin().is_terminal() {
        bail!("no terminal to pick in; name one: vx wallpaper <name> (see vx wallpapers)");
    }
    for (i, l) in lines.iter().enumerate() {
        println!("  {:>2}  {l}", i + 1);
    }
    print!("wallpaper (number or name, empty to cancel): ");
    std::io::stdout().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if let Ok(i) = s.parse::<usize>() {
        return match all.get(i.wrapping_sub(1)) {
            Some(n) => Ok(Some(n.clone())),
            None => bail!("no wallpaper number {i}"),
        };
    }
    Ok(Some(s.to_string()))
}

/// Names go into meta.nix and paths: keep them plain.
fn check_name(n: &str) -> Result<()> {
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') || n.starts_with('.') {
        bail!("'{n}' is not a wallpaper name (letters, digits, - _ .)");
    }
    Ok(())
}

/// meta.nix with `wallpaper = "<n>";` and `theme = "match";` (every such line, like the sed).
pub fn set_meta(text: &str, n: &str) -> String {
    let w = Regex::new(r"(?m)^( *)wallpaper = .*;").unwrap();
    let t = Regex::new(r"(?m)^( *)theme = .*;").unwrap();
    let text = w.replace_all(text, format!("${{1}}wallpaper = \"{n}\";").as_str());
    t.replace_all(&text, "${1}theme = \"match\";").into_owned()
}

/// `vx wallpaper [name]` / `vx wallpaper set <name>`.
pub fn set(ctx: &Ctx, name: Option<String>) -> Result<()> {
    let repo = ctx.repo()?;
    let n = match name {
        Some(n) => n,
        None => match pick(ctx, repo)? {
            Some(n) => n,
            None => return Ok(()),
        },
    };
    check_name(&n)?;
    if !repo.join("assets/wallpapers").join(&n).join("wide.png").is_file() {
        bail!("no wallpaper '{n}'. see: vx wallpapers");
    }
    let meta = palette::meta_path(repo);
    let text = std::fs::read_to_string(&meta).with_context(|| format!("could not read {}", meta.display()))?;
    std::fs::write(&meta, set_meta(&text, &n)).with_context(|| format!("could not write {}", meta.display()))?;
    ctx.run(ctx.cmd("git").args(["add", "-A", "assets/wallpapers", "modules/flake/meta.nix"]))?;
    vx_switch(ctx)?;
    ui::ok(&format!("now: {n}. commit it with: vx commit 'style: wallpaper {n}'"));
    Ok(())
}

const DEMO: &str = r#"printf '\033[1m%s\033[0m\n\n' "$1"
for i in 0 1 2 3 4 5 6 7; do printf '\033[4%sm    \033[0m' "$i"; done; echo
for i in 0 1 2 3 4 5 6 7; do printf '\033[10%sm    \033[0m' "$i"; done; echo; echo
printf '\033[34maccent\033[0m  \033[31mred\033[0m  \033[32mgreen\033[0m  \033[33myellow\033[0m  \033[90mcomment\033[0m\n\n'
exec /bin/zsh -f
"#;

/// The Ghostty config the try window uses.
pub fn ghostty_config(p: &Palette) -> String {
    let ansi = [
        "base00", "base08", "base0B", "base0A", "base0D", "base0E", "base0C", "base05", "base03", "base08", "base0B",
        "base0A", "base0D", "base0E", "base0C", "base07",
    ];
    let c = |k: &str| p.get(k).unwrap_or("000000").to_string();
    let mut s = format!(
        "background = #{}\nforeground = #{}\ncursor-color = #{}\nselection-background = #{}\n",
        c("base00"),
        c("base05"),
        c("base05"),
        c("base02")
    );
    for (i, k) in ansi.iter().enumerate() {
        s += &format!("palette = {i}=#{}\n", c(k));
    }
    s += "font-family = JetBrainsMono Nerd Font\nwindow-padding-x = 8\nwindow-padding-y = 8\n";
    s
}

/// `vx wallpaper try <name>`: a throwaway Ghostty window in its colours + the 4K image.
pub fn try_(ctx: &Ctx, name: &str) -> Result<()> {
    ctx.need(Host::Mac)?;
    let repo = ctx.repo()?;
    check_name(name)?;
    let d = repo.join("assets/wallpapers").join(name);
    let theme = d.join("theme.nix");
    if !theme.is_file() {
        bail!("no wallpaper '{name}'. see: vx wallpapers");
    }
    // The recipe used the bare `scheme`; this includes the file's `overrides`,
    // which is what the theme would really look like.
    let p = palette::load(&theme, None, false)?;
    let tmp = std::env::temp_dir().join(format!("vx-wallpaper-try-{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    let demo = tmp.join("demo.sh");
    std::fs::write(&demo, DEMO)?;
    let conf = tmp.join("ghostty");
    std::fs::write(&conf, ghostty_config(&p) + &format!("command = /bin/sh {} {name}\n", demo.display()))?;
    let quiet = |c: &mut std::process::Command| {
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    };
    let mut g = ctx.cmd("/Applications/Ghostty.app/Contents/MacOS/ghostty");
    g.arg("--config-default-files=false").arg(format!("--config-file={}", conf.display()));
    quiet(&mut g);
    g.spawn().context("could not start Ghostty (/Applications/Ghostty.app)")?;
    let mut q = ctx.cmd("qlmanage");
    q.arg("-p").arg(d.join("4k.png"));
    quiet(&mut q);
    q.spawn().context("could not start qlmanage")?;
    ui::info("close both windows when done. nothing on the mac was changed.");
    Ok(())
}

/// Resolve a path the user typed against where they typed it (the recipe ran in the repo).
fn abs(p: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(p).with_context(|| format!("no such file: {}", p.display()))
}

/// `vx wallpaper add <name> <wide> <uhd>`: copy the images in as PNG and make a palette.
pub fn add(ctx: &Ctx, name: &str, wide: &Path, uhd: &Path) -> Result<()> {
    let repo = ctx.repo()?;
    check_name(name)?;
    let (wide, uhd) = (abs(wide)?, abs(uhd)?);
    let rel = format!("assets/wallpapers/{name}");
    let d = repo.join(&rel);
    std::fs::create_dir_all(&d)?;
    let py = r#"with import (builtins.getFlake "nixpkgs") {}; python3.withPackages (p: [ p.pillow ])"#;
    let shell = |args: &[&std::ffi::OsStr]| {
        let mut c = ctx.nix();
        c.args(["shell", "--impure", "--expr", py, "-c", "python3", "-I"]).args(args);
        c
    };
    let convert = "import sys; from PIL import Image; Image.open(sys.argv[1]).convert('RGB').save(sys.argv[2])";
    for (src, dst) in [(&wide, "wide.png"), (&uhd, "4k.png")] {
        ui::info(&format!("{} -> {rel}/{dst}", src.display()));
        ctx.run(&mut shell(&["-c".as_ref(), convert.as_ref(), src.as_os_str(), d.join(dst).as_os_str()]))?;
    }
    let script = repo.join("scripts/wallpaper-palette.py");
    let out = ctx.output(&mut shell(&[script.as_os_str(), d.join("wide.png").as_os_str(), "--name".as_ref(), name.as_ref()]))?;
    std::fs::write(d.join("theme.nix"), out + "\n")?;
    ctx.run(ctx.cmd("git").args(["add", &rel]))?;
    ui::ok(&format!("added {rel}. tweak {rel}/theme.nix if a colour feels off, then: vx wallpaper {name}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_meta_edits_both_lines() {
        let m = "  x = 1;\n    wallpaper = \"eye\";\n    theme = \"caelus\";\n    # wallpaper: a folder\n    themeOverrides = { };\n";
        let out = set_meta(m, "noir");
        assert!(out.contains("    wallpaper = \"noir\";\n"));
        assert!(out.contains("    theme = \"match\";\n"));
        assert!(out.contains("    # wallpaper: a folder\n"));
        assert!(out.contains("themeOverrides = { };"));
    }

    #[test]
    fn names_are_checked() {
        assert!(check_name("racing-green").is_ok());
        for bad in ["", "../x", "a\"b", ".hidden", "a b"] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ghostty_palette_order() {
        let p = palette::parse(r#"scheme = { base00 = "000001"; base08 = "ff0000"; base07 = "ffffff"; };"#, None, false);
        let c = ghostty_config(&p);
        assert!(c.starts_with("background = #000001\n"));
        assert!(c.contains("palette = 1=#FF0000\n"));
        assert!(c.contains("palette = 15=#FFFFFF\n"));
    }
}
