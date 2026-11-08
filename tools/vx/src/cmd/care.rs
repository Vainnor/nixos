//! Looking after the machine: secrets, snapshots and backups, what survives a
//! reboot, fans, temporary sudo, Secure Boot + TPM, and USB devices.

mod mounts;
mod persist;
mod restore;
mod secrets;

use crate::ctx::{Ctx, Host};
use crate::ui;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use std::io::Write;
use std::process::Stdio;

#[derive(Subcommand)]
pub enum Cmd {
    /// Secrets in secrets/secrets.yaml (sops)
    #[command(subcommand)]
    Secrets(secrets::Cmd),

    /// List the hourly snapshots (files are under /.btrfs/@snapshots/<name>/)
    Snapshots,
    /// Take a snapshot right now (before something risky)
    Snapshot,
    /// Run the restic backup to the homelab now, then show the latest snapshots
    Backup,
    /// Bring a file or directory back from a snapshot (pick a version from the list)
    Restore(restore::Args),
    /// Find files on the RAM-backed / that will be lost at reboot, and how to keep them
    PersistCheck(persist::Args),

    /// Temperatures and fan speeds right now
    Fans {
        #[command(subcommand)]
        cmd: Option<FansCmd>,
    },

    /// Passwordless sudo for a while
    #[command(subcommand)]
    Sudo(SudoCmd),

    /// Show Secure Boot state and whether this boot was signed
    Secureboot,
    /// Disk unlock with the TPM
    #[command(subcommand)]
    Tpm(TpmCmd),

    /// List USB devices; blocked ones say "block"
    Usb {
        #[command(subcommand)]
        cmd: Option<UsbCmd>,
    },
}

#[derive(Subcommand)]
pub enum FansCmd {
    /// Copy the fan curves from CoolerControl into the repo (a fresh install starts with them)
    Save,
}

#[derive(Subcommand)]
pub enum SudoCmd {
    /// sudo without a password for a while, e.g. so Claude can work over SSH. A reboot also ends it.
    Unlock {
        /// Minutes until it locks again
        #[arg(default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..))]
        minutes: u32,
    },
    /// End `vx sudo unlock` now
    Lock,
}

#[derive(Subcommand)]
pub enum TpmCmd {
    /// After Secure Boot is ON: unlock the disk with the TPM (passphrase still works)
    Enroll,
}

#[derive(Subcommand)]
pub enum UsbCmd {
    /// Allow a blocked device for good (number from `vx usb`)
    Allow {
        /// Device number from `vx usb`
        id: u32,
    },
}

const UNLOCK_FILE: &str = "/run/sudoers.d/unlock";
const COOLERCONTROL: &str = "/etc/coolercontrol/config.toml";
const LUKS: &str = "/dev/disk/by-partlabel/disk-main-luks";

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Secrets(c) => secrets::run(ctx, c),
        Cmd::Snapshots => {
            ctx.need(Host::NixOs)?;
            ctx.run(
                ctx.sudo("btrbk")
                    .args(["-c", "/etc/btrbk/local.conf", "list", "snapshots"]),
            )
        }
        Cmd::Snapshot => {
            ctx.need(Host::NixOs)?;
            ctx.run(ctx.sudo("systemctl").args(["start", "btrbk-local"]))?;
            ui::ok("snapshot taken; see them with: vx snapshots");
            Ok(())
        }
        Cmd::Backup => {
            ctx.need(Host::NixOs)?;
            if !crate::ctx::has("restic-homelab") {
                bail!("restic isn't set up; fill restic/* in the secrets and set meta.resticReady = true");
            }
            ctx.run(ctx.sudo("systemctl").args(["start", "restic-backups-homelab"]))?;
            ctx.run(ctx.sudo("restic-homelab").args(["snapshots", "--latest", "5"]))
        }
        Cmd::Restore(a) => restore::run(ctx, a),
        Cmd::PersistCheck(a) => persist::run(ctx, a),
        Cmd::Fans { cmd: None } => {
            ctx.need(Host::NixOs)?;
            ctx.run(&mut ctx.cmd("sensors"))
        }
        Cmd::Fans {
            cmd: Some(FansCmd::Save),
        } => fans_save(ctx),
        Cmd::Sudo(SudoCmd::Unlock { minutes }) => sudo_unlock(ctx, minutes),
        Cmd::Sudo(SudoCmd::Lock) => {
            ctx.need(Host::NixOs)?;
            ctx.run(ctx.sudo("rm").args(["-f", UNLOCK_FILE]))?;
            ctx.probe(ctx.sudo("systemctl").args(["stop", "sudo-relock.timer"]));
            ui::ok("sudo asks for a password again");
            Ok(())
        }
        Cmd::Secureboot => {
            ctx.need(Host::NixOs)?;
            ctx.run(ctx.sudo("sbctl").arg("status"))?;
            let out = ctx.probe(ctx.cmd("bootctl").arg("status")).unwrap_or_default();
            for l in out.lines().filter(|l| {
                let l = l.to_lowercase();
                l.contains("secure boot") || l.contains("setup mode")
            }) {
                println!("{l}");
            }
            Ok(())
        }
        Cmd::Tpm(TpmCmd::Enroll) => {
            ctx.need(Host::NixOs)?;
            let out = ctx.probe(ctx.cmd("bootctl").arg("status")).unwrap_or_default();
            if !secure_boot_on(&out) {
                bail!("turn Secure Boot on first (vx secureboot)");
            }
            ctx.run(ctx.sudo("systemd-cryptenroll").args([
                "--wipe-slot=tpm2",
                "--tpm2-device=auto",
                "--tpm2-pcrs=7",
                LUKS,
            ]))
        }
        Cmd::Usb { cmd } => {
            ctx.need(Host::NixOs)?;
            match cmd {
                None => ctx.run(ctx.cmd("usbguard").arg("list-devices")),
                Some(UsbCmd::Allow { id }) => {
                    ctx.run(ctx.cmd("usbguard").args(["allow-device", "-p", &id.to_string()]))
                }
            }
        }
    }
}

/// `bootctl status` says `Secure Boot: enabled (user)` once it's on.
pub fn secure_boot_on(bootctl: &str) -> bool {
    bootctl.to_lowercase().contains("secure boot: enabled")
}

fn fans_save(ctx: &Ctx) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let repo = ctx.repo()?;
    if ctx.probe(ctx.sudo("test").args(["-f", COOLERCONTROL])).is_none() {
        bail!("no CoolerControl config yet; set up a curve in the app first");
    }
    let toml = ctx.output(ctx.sudo("cat").arg(COOLERCONTROL))?;
    let dir = repo.join("assets/coolercontrol");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("config.toml"), format!("{toml}\n"))?;
    ctx.run(ctx.cmd("git").args(["add", "assets/coolercontrol/config.toml"]))?;
    ui::ok("saved; commit it with a message like: chore(fans): save fan curves");
    Ok(())
}

fn sudo_unlock(ctx: &Ctx, minutes: u32) -> Result<()> {
    ctx.need(Host::NixOs)?;
    let user = std::env::var("USER").context("USER is not set")?;
    let new = format!("{UNLOCK_FILE}.new");

    let mut tee = ctx.sudo("tee");
    tee.arg(&new).stdin(Stdio::piped()).stdout(Stdio::null());
    let mut child = tee.spawn().context("could not start sudo tee")?;
    child
        .stdin
        .take()
        .context("no stdin")?
        .write_all(format!("{user} ALL=(ALL:ALL) NOPASSWD: ALL\n").as_bytes())?;
    if !child.wait()?.success() {
        bail!("couldn't write {new}");
    }
    ctx.run(ctx.sudo("chmod").args(["0440", &new]))?;
    if ctx.run(ctx.sudo("visudo").args(["-cqf", &new])).is_err() {
        ctx.probe(ctx.sudo("rm").args(["-f", &new]));
        bail!("visudo rejected the rule; nothing changed");
    }
    ctx.run(ctx.sudo("mv").args([&new, UNLOCK_FILE]))?;
    ctx.probe(ctx.sudo("systemctl").args(["stop", "sudo-relock.timer"]));
    ctx.run(ctx.sudo("systemd-run").args([
        "--quiet",
        "--unit=sudo-relock",
        &format!("--on-active={minutes}m"),
        "rm",
        "-f",
        UNLOCK_FILE,
    ]))?;
    let until = ctx
        .probe(ctx.cmd("date").args(["-d", &format!("+{minutes} min"), "+%H:%M"]))
        .unwrap_or_else(|| format!("{minutes} min from now"));
    ui::ok(&format!(
        "passwordless sudo until {until}; end it now with: vx sudo lock"
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_boot_detection() {
        assert!(secure_boot_on("  Secure Boot: enabled (user)\n"));
        assert!(!secure_boot_on("  Secure Boot: disabled (setup)\n"));
        assert!(!secure_boot_on(""));
    }
}
