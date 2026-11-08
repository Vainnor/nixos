//! The desktop: wallpaper and colours, the shell (vshell), audio and OBS.

mod palette;
mod theme;
mod wallpaper;

use crate::ctx::{Ctx, Host};
use crate::ui;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Cmd {
    /// List the wallpapers (the current one is marked)
    Wallpapers,
    /// Switch wallpaper and colours: `vx wallpaper pinewood` (no name: pick from a list)
    Wallpaper(WallpaperArgs),
    /// Palettes: check contrast, show swatches, change the accent
    #[command(subcommand)]
    Theme(ThemeCmd),
    /// The desktop shell (vshell): live-reload dev mode, logs, restart
    #[command(subcommand)]
    Shell(ShellCmd),
    /// Audio devices
    #[command(subcommand)]
    Audio(AudioCmd),
    /// OBS settings
    #[command(subcommand)]
    Obs(ObsCmd),
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct WallpaperArgs {
    /// Wallpaper to switch to (same as `vx wallpaper set <name>`)
    name: Option<String>,
    #[command(subcommand)]
    cmd: Option<WallpaperCmd>,
}

#[derive(Subcommand)]
pub enum WallpaperCmd {
    /// Switch wallpaper and colours on this machine, then rebuild (no name: pick)
    Set {
        /// A folder in assets/wallpapers
        name: Option<String>,
        /// Only record it in meta.nix (the shell's picker already changed it live); the next switch builds it in
        #[arg(long)]
        no_switch: bool,
    },
    /// List the wallpapers (the current one is marked)
    List,
    /// Try a wallpaper's colours on the Mac without changing anything (Ghostty window + the 4K image)
    Try {
        /// A folder in assets/wallpapers
        name: String,
    },
    /// Add a wallpaper from two images and make a matching palette
    Add {
        /// Folder name for it in assets/wallpapers
        name: String,
        /// The ultrawide image
        wide: PathBuf,
        /// The 4K (16:9) image
        uhd: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum ThemeCmd {
    /// Check every palette's contrast and colour (or some: `vx theme check noir eye`)
    Check {
        /// Palettes to check (default: all)
        names: Vec<String>,
        /// Show only problems
        #[arg(short, long)]
        quiet: bool,
        /// Ignore each file's `overrides` (the generated scheme on its own)
        #[arg(long)]
        raw: bool,
    },
    /// Show a palette as colour swatches (default: the live one)
    Show {
        /// A theme in assets/themes or a wallpaper name
        name: Option<String>,
        /// Ignore `overrides` and meta.themeOverrides
        #[arg(long)]
        raw: bool,
    },
    /// Set the accent (base0D) in meta.themeOverrides: hex, another slot (base0E), or reset
    Accent {
        /// E0A040, #E0A040, base0E, or reset
        value: String,
        /// Switch afterwards without asking
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum ShellCmd {
    /// Run the shell from the repo's shell/ with live reload (save a .qml file, it updates). Ctrl+C to stop
    Dev,
    /// Shell logs (errors from the QML show up here)
    Log {
        /// Lines of history to show
        #[arg(short = 'n', long, default_value_t = 100)]
        lines: u32,
        /// Print and exit instead of following
        #[arg(long)]
        no_follow: bool,
    },
    /// Restart the shell
    Restart,
    /// Pick up a new build: reload the user units, then restart the shell
    Reload,
}

#[derive(Subcommand)]
pub enum AudioCmd {
    /// List audio outputs; put the real one's name in audioOutput in modules/hosts/<host>/host.json
    Devices,
}

#[derive(Subcommand)]
pub enum ObsCmd {
    /// Back up OBS settings and re-seed the Vainnor profile and scene collections
    Reset {
        /// Don't ask first
        #[arg(short, long)]
        yes: bool,
    },
}

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Wallpapers => wallpaper::list(ctx),
        Cmd::Wallpaper(a) => match a.cmd {
            None => wallpaper::set(ctx, a.name, true),
            Some(WallpaperCmd::Set { name, no_switch }) => wallpaper::set(ctx, name, !no_switch),
            Some(WallpaperCmd::List) => wallpaper::list(ctx),
            Some(WallpaperCmd::Try { name }) => wallpaper::try_(ctx, &name),
            Some(WallpaperCmd::Add { name, wide, uhd }) => wallpaper::add(ctx, &name, &wide, &uhd),
        },
        Cmd::Theme(ThemeCmd::Check { names, quiet, raw }) => {
            let failed = palette::check(ctx.repo()?, &names, quiet, raw)?;
            if failed {
                // Like the script: exit 1, the report already says why.
                use std::io::Write;
                let _ = std::io::stdout().flush();
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Theme(ThemeCmd::Show { name, raw }) => theme::show(ctx, name.as_deref(), raw),
        Cmd::Theme(ThemeCmd::Accent { value, yes }) => theme::accent(ctx, &value, yes),
        Cmd::Shell(c) => shell(ctx, c),
        Cmd::Audio(AudioCmd::Devices) => audio_devices(ctx),
        Cmd::Obs(ObsCmd::Reset { yes }) => obs_reset(ctx, yes),
    }
}

/// Two spaces of a 24-bit background colour.
fn swatch(hex: &str) -> String {
    if !palette::is_hex(hex) {
        return "  ".into();
    }
    let [r, g, b] = palette::rgb(hex);
    format!("\x1b[48;2;{r};{g};{b}m  \x1b[0m")
}

/// Rebuild via `vx switch` (the system area's), so this file doesn't depend on its code.
fn vx_switch(ctx: &Ctx) -> Result<()> {
    let exe = std::env::current_exe().context("could not find the vx binary")?;
    let mut c = ctx.cmd(exe.to_str().unwrap_or("vx"));
    c.arg("--repo").arg(&ctx.repo);
    if ctx.verbose {
        c.arg("-v");
    }
    c.arg("switch");
    ctx.run(&mut c)
}

const UNIT: &str = "vshell";

/// VSHELL_* variables from `systemctl --user show vshell -p Environment --value`.
fn shell_env(out: &str) -> Vec<(String, String)> {
    out.split_whitespace()
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| k.starts_with("VSHELL_"))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn shell(ctx: &Ctx, c: ShellCmd) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let sysd = |args: &[&str]| {
        let mut c = ctx.cmd("systemctl");
        c.arg("--user").args(args);
        c
    };
    match c {
        ShellCmd::Dev => {
            let dir = ctx.repo()?.join("shell");
            let env = ctx.output(&mut sysd(&["show", UNIT, "-p", "Environment", "--value"]))?;
            // The recipe passed THEME, WALLPAPER and AUDIO_OUTPUT; all VSHELL_*
            // (WALLPAPER_NAME, WALLPAPERS too) so the wallpaper picker works in dev.
            let env = shell_env(&env);
            ui::info("shell running from the repo; save a .qml file and it reloads. ctrl+c to stop.");
            // bash owns the stop/start so ctrl+c (which also ends vx) still brings the service back.
            let mut c = ctx.cmd("bash");
            c.args([
                "-c",
                "systemctl --user stop vshell; trap 'systemctl --user start vshell' EXIT; qs -p \"$1\"",
                "bash",
            ])
            .arg(&dir)
            .envs(env);
            let _ = ctx.run(&mut c);
            Ok(())
        }
        ShellCmd::Log { lines, no_follow } => {
            let mut c = ctx.cmd("journalctl");
            c.args(["--user", "-u", UNIT, "-n", &lines.to_string()]);
            if !no_follow {
                c.arg("-f");
            }
            ctx.run(&mut c)
        }
        ShellCmd::Restart => {
            ctx.run(&mut sysd(&["restart", UNIT]))?;
            ui::ok("shell restarted");
            Ok(())
        }
        ShellCmd::Reload => {
            ctx.run(&mut sysd(&["daemon-reload"]))?;
            ctx.run(&mut sysd(&["restart", UNIT]))?;
            ui::ok("shell reloaded");
            Ok(())
        }
    }
}

/// Sink names from `pactl list short sinks`, without the virtual channel_* ones.
fn sinks(out: &str) -> Vec<&str> {
    out.lines().filter_map(|l| l.split_whitespace().nth(1)).filter(|n| !n.starts_with("channel_")).collect()
}

fn audio_devices(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let out = ctx.output(ctx.cmd("pactl").args(["list", "short", "sinks"]))?;
    for s in sinks(&out) {
        println!("{s}");
    }
    Ok(())
}

fn obs_reset(ctx: &Ctx, yes: bool) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
    let obs = home.join(".config/obs-studio");
    let overlay = home.join(".local/share/vainnor-overlays/js/config.js");
    if !yes && !ui::confirm("back up OBS settings and re-seed them, then rebuild?") {
        return Ok(());
    }
    let stamp = ctx.output(ctx.cmd("date").arg("+%F_%H-%M-%S"))?;
    for p in [&obs, &overlay] {
        if p.exists() {
            let bak = PathBuf::from(format!("{}.bak-{stamp}", p.display()));
            std::fs::rename(p, &bak).with_context(|| format!("could not move {}", p.display()))?;
            ui::info(&format!("moved {} to {}", p.display(), bak.display()));
        }
    }
    vx_switch(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct T {
        #[command(subcommand)]
        cmd: Cmd,
    }

    fn parse(args: &[&str]) -> Cmd {
        T::try_parse_from(std::iter::once("vx").chain(args.iter().copied())).unwrap().cmd
    }

    #[test]
    fn wallpaper_forms() {
        match parse(&["wallpaper", "pinewood"]) {
            Cmd::Wallpaper(WallpaperArgs { name: Some(n), cmd: None }) => assert_eq!(n, "pinewood"),
            _ => panic!("vx wallpaper <name>"),
        }
        assert!(matches!(parse(&["wallpaper"]), Cmd::Wallpaper(WallpaperArgs { name: None, cmd: None })));
        assert!(matches!(parse(&["wallpaper", "list"]), Cmd::Wallpaper(WallpaperArgs { cmd: Some(WallpaperCmd::List), .. })));
        assert!(matches!(
            parse(&["wallpaper", "set", "noir"]),
            Cmd::Wallpaper(WallpaperArgs { cmd: Some(WallpaperCmd::Set { name: Some(_), .. }), .. })
        ));
        assert!(matches!(
            parse(&["wallpaper", "try", "noir"]),
            Cmd::Wallpaper(WallpaperArgs { cmd: Some(WallpaperCmd::Try { .. }), .. })
        ));
    }

    #[test]
    fn theme_check_args() {
        match parse(&["theme", "check", "noir", "eye", "--quiet"]) {
            Cmd::Theme(ThemeCmd::Check { names, quiet, raw }) => {
                assert_eq!(names, ["noir", "eye"]);
                assert!(quiet && !raw);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn sink_names() {
        let out = "52\talsa_output.usb-BEACN.analog-stereo\tPipeWire\ts24le 2ch 48000Hz\tSUSPENDED\n60\tchannel_game\tPipeWire\tfloat32le 2ch 48000Hz\tRUNNING\n";
        assert_eq!(sinks(out), ["alsa_output.usb-BEACN.analog-stereo"]);
    }

    #[test]
    fn env_vars() {
        let out = "VSHELL_THEME=/nix/store/a-vshell-theme.json VSHELL_AUDIO_OUTPUT= PATH=/run/wrappers/bin";
        assert_eq!(
            shell_env(out),
            [("VSHELL_THEME".into(), "/nix/store/a-vshell-theme.json".into()), ("VSHELL_AUDIO_OUTPUT".into(), String::new())]
        );
    }
}
