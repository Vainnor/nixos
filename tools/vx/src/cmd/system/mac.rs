//! The Mac's one-off chores: hand apps to Homebrew, take nix-darwin back out,
//! and the Homebrew upgrade `vx switch` doesn't do.

use super::util::prompt;
use crate::ctx::{Ctx, MAC_HOST};
use crate::ui;
use anyhow::{Result, bail};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// First time only: `brew install --cask --adopt` every cask the config lists
/// that Homebrew doesn't own yet.
pub fn adopt(ctx: &Ctx) -> Result<()> {
    let have = ctx.output(Command::new("brew").args(["list", "--cask", "-1"]))?;
    let have: BTreeSet<&str> = have.lines().collect();
    let attr = format!("{}#darwinConfigurations.{MAC_HOST}.config.homebrew.casks", ctx.repo()?.display());
    let casks = ctx.output(ctx.nix().args(["eval", "--raw", &attr, "--apply", r#"cs: builtins.concatStringsSep " " (map (c: c.name) cs)"#]))?;
    let mut failed = vec![];
    for c in casks.split_whitespace() {
        let short = c.rsplit('/').next().unwrap_or(c);
        if have.contains(short) {
            continue;
        }
        ui::heading(c);
        let ok = Command::new("brew")
            .args(["install", "--cask", "--adopt", c])
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            failed.push(c.to_string());
        }
    }
    if failed.is_empty() {
        ui::ok("all casks are Homebrew's now. next: vx switch");
        Ok(())
    } else {
        bail!("could not adopt: {}. usually a version mismatch: update that app (or delete it) and run this again", failed.join(" "))
    }
}

/// Update Homebrew and upgrade every brew and cask.
pub fn upgrade(ctx: &Ctx) -> Result<()> {
    ctx.run(Command::new("brew").arg("update"))?;
    ctx.run(Command::new("brew").arg("upgrade"))?;
    ctx.run(Command::new("brew").args(["upgrade", "--cask", "--greedy"]))
}

/// `(domain, key)` of every `defaults write` in nix-darwin's activate script.
/// nix-darwin quotes the key (`defaults write com.apple.dock 'autohide' ...`),
/// which the old recipe's grep missed; quoted and bare keys both match here.
pub fn defaults_keys(activate: &str) -> BTreeSet<(String, String)> {
    let re = Regex::new(r"defaults write +'?(-g|[^ ']+)'? +'?([^ ']+)'?").expect("regex");
    re.captures_iter(activate).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

struct Undo {
    dry: bool,
}

impl Undo {
    /// Run (ignoring failure, like the recipe's `|| true`) or, dry, just say it.
    fn run(&self, prog: &str, args: &[&str]) {
        if self.dry {
            println!("  would: {prog} {}", args.join(" "));
        } else {
            let _ = Command::new(prog).args(args).stderr(std::process::Stdio::null()).status();
        }
    }
    fn sudo(&self, args: &[&str]) {
        self.run("sudo", args)
    }
}

/// Remove nix-darwin and put the Mac back as it was. `dry` only shows what it would do.
pub fn undo(ctx: &Ctx, dry: bool) -> Result<()> {
    let sys = match std::fs::canonicalize("/run/current-system") {
        Ok(s) if s.join("activate").exists() => s,
        _ => {
            ui::ok("nix-darwin isn't active on this Mac; nothing to undo");
            return Ok(());
        }
    };
    let u = Undo { dry };
    let refs = ctx.output(Command::new("nix-store").arg("-qR").arg(&sys)).unwrap_or_default();
    let hmf = refs.lines().find(|l| l.ends_with("home-manager-files")).map(PathBuf::from);

    if !dry {
        println!("This removes nix-darwin from this Mac:");
        println!("  - macOS settings it set go back to Apple's defaults, firewall off");
        println!("  - dotfiles it manages are removed, and your *.hm-backup originals put back");
        println!("  - nix-darwin itself is uninstalled, /etc files it replaced are restored");
        println!("Homebrew, your apps, Nix itself and the wallpaper stay as they are.");
        if prompt("Type UNDO to continue: ")? != "UNDO" {
            bail!("nothing changed");
        }
    }

    ui::heading("macos settings");
    let activate = std::fs::read_to_string(sys.join("activate")).unwrap_or_default();
    for (dom, key) in defaults_keys(&activate) {
        if dom.starts_with("/Library/") {
            u.sudo(&["defaults", "delete", &dom, &key]);
        } else {
            u.run("defaults", &["delete", &dom, &key]);
        }
    }
    let fw = "/usr/libexec/ApplicationFirewall/socketfilterfw";
    u.sudo(&[fw, "--setstealthmode", "off"]);
    u.sudo(&[fw, "--setglobalstate", "off"]);
    u.run("killall", &["Dock", "Finder", "SystemUIServer"]);

    ui::heading("dotfiles");
    if let Some(hmf) = hmf {
        let home = PathBuf::from(std::env::var("HOME")?);
        for e in walkdir::WalkDir::new(&hmf).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            let ft = e.file_type();
            if !(ft.is_symlink() || ft.is_file()) {
                continue;
            }
            let Ok(rel) = e.path().strip_prefix(&hmf) else { continue };
            let t = home.join(rel);
            let ts = t.to_string_lossy().into_owned();
            if std::fs::read_link(&t).map(|l| l.starts_with("/nix/store")).unwrap_or(false) {
                u.run("rm", &[&ts]);
            }
            let backup = PathBuf::from(format!("{ts}.hm-backup"));
            if backup.exists() && (dry || !t.exists()) {
                u.run("mv", &[&backup.to_string_lossy(), &ts]);
            }
        }
    }

    ui::heading("nix-darwin");
    u.sudo(&[&sys.join("sw/bin/darwin-uninstaller").to_string_lossy()]);
    for dir in ["/etc", "/etc/nix"] {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.filter_map(|e| e.ok()) {
            let b = e.path();
            let bs = b.to_string_lossy().into_owned();
            let Some(o) = bs.strip_suffix(".before-nix-darwin") else { continue };
            if !Path::new(&bs).exists() {
                continue;
            }
            u.sudo(&["rm", "-f", o]);
            u.sudo(&["mv", &bs, o]);
        }
    }
    u.sudo(&["launchctl", "kickstart", "-k", "system/org.nixos.nix-daemon"]);

    if dry {
        ui::info("dry run: nothing changed");
    } else {
        ui::ok("done. log out and back in so every app sees the old settings");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let a = "defaults write -g AppleShowAllExtensions -bool true\n\
                 defaults write com.apple.dock 'autohide' -bool true\n\
                 defaults write com.apple.dock tilesize -int 48\n\
                 sudo defaults write /Library/Preferences/com.apple.loginwindow GuestEnabled -bool false\n\
                 defaults write -g AppleShowAllExtensions -bool true\n";
        let k = defaults_keys(a);
        assert!(k.contains(&("-g".into(), "AppleShowAllExtensions".into())));
        assert!(k.contains(&("com.apple.dock".into(), "tilesize".into())));
        assert!(k.contains(&("/Library/Preferences/com.apple.loginwindow".into(), "GuestEnabled".into())));
        assert!(k.contains(&("com.apple.dock".into(), "autohide".into())));
        assert_eq!(k.len(), 4);
    }
}
