//! vx: one command for this flake. Each area of commands lives in src/cmd/<area>.rs
//! and is flattened into the top-level command list here.

mod cmd;
mod ctx;
mod ui;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vx", version, about = "One command for this NixOS / nix-darwin flake", long_about = None)]
#[command(propagate_version = true, arg_required_else_help = true)]
pub struct Cli {
    /// Path to the flake repo (default: ~/nixos on Linux, ~/Programing/nixos on macOS)
    #[arg(long, global = true, env = "VX_REPO")]
    pub repo: Option<std::path::PathBuf>,

    /// Print commands before running them
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    #[command(flatten)]
    System(cmd::system::Cmd),
    #[command(flatten)]
    Pkgs(cmd::pkgs::Cmd),
    #[command(flatten)]
    Care(cmd::care::Cmd),
    #[command(flatten)]
    Desktop(cmd::desktop::Cmd),
    #[command(flatten)]
    Vm(cmd::vm::Cmd),

    /// Print shell completions (fish, bash, zsh)
    #[command(hide = true)]
    Completions { shell: clap_complete::Shell },
}

fn main() {
    if let Err(e) = run() {
        ui::fail(&format!("{e:#}"));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // `vx @arc switch` / `vx @mac status`: run on the other machine over SSH.
    let args: Vec<String> = std::env::args().collect();
    if let Some(target) = args.get(1).and_then(|a| a.strip_prefix('@')) {
        return cmd::system::remote(target, &args[2..]);
    }

    let cli = Cli::parse();
    let ctx = ctx::Ctx::new(cli.repo.clone(), cli.verbose)?;
    match cli.cmd {
        Cmd::System(c) => cmd::system::run(&ctx, c),
        Cmd::Pkgs(c) => cmd::pkgs::run(&ctx, c),
        Cmd::Care(c) => cmd::care::run(&ctx, c),
        Cmd::Desktop(c) => cmd::desktop::run(&ctx, c),
        Cmd::Vm(c) => cmd::vm::run(&ctx, c),
        Cmd::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "vx", &mut std::io::stdout());
            Ok(())
        }
    }
}
