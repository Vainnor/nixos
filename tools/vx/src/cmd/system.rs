//! (stub, being written)
use crate::ctx::Ctx;
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Cmd {}

pub fn run(_ctx: &Ctx, cmd: Cmd) -> Result<()> {
    match cmd {}
}

/// `vx @arc ...`: run vx on another machine.
pub fn remote(_target: &str, _args: &[String]) -> Result<()> {
    anyhow::bail!("not written yet")
}
