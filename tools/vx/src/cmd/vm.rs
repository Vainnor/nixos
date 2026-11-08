//! VMs: the light Windows VM (win11-lite), the GPU-passthrough Windows VM
//! (win11), their setup, and WinApps. See modules/virtualisation/vm.nix.

use crate::ctx::{Ctx, Host};
use crate::ui;
use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const URI: &str = "qemu:///system";
const LITE: &str = "win11-lite";
const GPU_VM: &str = "win11";
const IMAGES: &str = "/var/lib/libvirt/images";
const TEMPLATES: &str = "/etc/libvirt-templates";
/// Where NixOS links `virtualisation.libvirtd.hooks.qemu.gpu-passthrough`.
const HOOK: &str = "/var/lib/libvirt/hooks/qemu.d/gpu-passthrough";
/// Touched by the hook once the VM has had the card since boot.
const GPU_USED: &str = "/run/win11-gpu-used";
const VIRTIO_URL: &str =
    "https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/stable-virtio/virtio-win.iso";
/// Fallbacks when the host's vfio.nix can't be read.
const DEFAULT_GPU: &str = "0000:03:00.0";
const DEFAULT_ROM: &str = "/var/lib/libvirt/vbios/rx7900xtx.rom";

const WINDOWS_ABOUT: &str = "\
Start Windows with the GPU (your Linux session closes until Windows shuts down)

GPU passthrough is currently disabled on this PC: reserveGpu (hosts/arc/vfio.nix) is
false and the card gives no picture in Windows. The command is kept, but it
warns and asks first: starting it takes the GPU from the desktop, and the PC
reboots when Windows shuts down (the card can't be handed back to Linux).";

#[derive(Subcommand)]
pub enum Cmd {
    /// Virtual machines: status, stop, setup, disks, drivers
    Vm {
        #[command(subcommand)]
        cmd: VmCmd,
    },
    /// Start Windows with the GPU (disabled on this PC; see --help), or `lite` for the light VM
    #[command(long_about = WINDOWS_ABOUT, args_conflicts_with_subcommands = true)]
    Windows {
        #[command(subcommand)]
        cmd: Option<WindowsCmd>,
        /// Don't ask before taking the GPU
        #[arg(short, long)]
        yes: bool,
    },
    /// WinApps: Windows apps from win11-lite as windows on the Linux desktop
    Winapps {
        #[command(subcommand)]
        cmd: WinappsCmd,
    },
}

#[derive(Subcommand)]
pub enum VmCmd {
    /// Both VMs' state, the GPU's driver, hugepages and the libvirt hook
    Status,
    /// Shut a Windows VM down cleanly (waits; --force pulls the plug after the timeout)
    Stop {
        /// Which VM (default: the one that's running)
        which: Option<Which>,
        /// Destroy the VM if it hasn't shut down by the timeout
        #[arg(short, long)]
        force: bool,
        /// Seconds to wait for Windows to shut down
        #[arg(short, long, default_value_t = 120)]
        timeout: u64,
        /// Don't ask before stopping the GPU VM (the PC reboots after it)
        #[arg(short, long)]
        yes: bool,
    },
    /// Open the light Windows VM's window (it must be running; `vx windows lite` starts it)
    Console,
    /// Show the GPU's PCI addresses and IOMMU group (for modules/hosts/<host>/vfio.nix)
    Gpu,
    /// Save the GPU's video BIOS for the gaming VM (needed for a picture; see gpuRom in hosts/<host>/vfio.nix)
    Vbios {
        /// PCI address of the card (default: gpu in hosts/<host>/vfio.nix)
        gpu: Option<String>,
    },
    /// List USB devices (vendor:product) to pass into the Windows VM
    Usb,
    /// (Re)define the Windows VMs from the config and start libvirt's network (safe to rerun)
    Setup,
    /// Create a VM disk image (sparse): `vx vm disk win11-lite 80G`, `vx vm disk win11 250G`
    Disk {
        /// VM name (the image is /var/lib/libvirt/images/<name>.img)
        #[arg(default_value = LITE)]
        name: String,
        /// Size, as qemu-img takes it (80G, 250G)
        #[arg(default_value = "80G")]
        size: String,
        /// Overwrite an existing image without asking
        #[arg(short, long)]
        yes: bool,
    },
    /// Download the virtio drivers ISO; put your Windows 11 ISO at /var/lib/libvirt/images/Win11.iso
    Virtio,
}

#[derive(Subcommand)]
pub enum WindowsCmd {
    /// Open the light Windows VM in a window (half the CPU, no GPU passthrough)
    Lite {
        #[command(subcommand)]
        cmd: Option<LiteCmd>,
    },
}

#[derive(Subcommand)]
pub enum LiteCmd {
    /// Shut the light Windows VM down cleanly
    Stop,
}

#[derive(Subcommand)]
pub enum WinappsCmd {
    /// Pick Windows apps to add to the app launcher (win11-lite must be running)
    Setup,
    /// Add more apps after installing them in Windows
    Add,
    /// The full Windows desktop over RDP, in a window
    Desktop,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Which {
    /// win11-lite, the everyday VM in a window
    #[value(alias = "win11-lite")]
    Lite,
    /// win11, the GPU-passthrough VM
    #[value(alias = "win11")]
    Gpu,
}

impl Which {
    fn name(self) -> &'static str {
        match self {
            Which::Lite => LITE,
            Which::Gpu => GPU_VM,
        }
    }
}

pub fn run(ctx: &Ctx, cmd: Cmd) -> Result<()> {
    ctx.need(Host::NixOs)?;
    match cmd {
        Cmd::Vm { cmd } => match cmd {
            VmCmd::Status => status(ctx),
            VmCmd::Stop {
                which,
                force,
                timeout,
                yes,
            } => stop(ctx, which, force, timeout, yes),
            VmCmd::Console => console(ctx),
            VmCmd::Gpu => gpu_list(ctx),
            VmCmd::Vbios { gpu } => vbios(ctx, gpu),
            VmCmd::Usb => ctx.run(&mut ctx.cmd("lsusb")),
            VmCmd::Setup => setup(ctx),
            VmCmd::Disk { name, size, yes } => disk(ctx, &name, &size, yes),
            VmCmd::Virtio => virtio(ctx),
        },
        Cmd::Windows { cmd: None, yes } => windows(ctx, yes),
        Cmd::Windows {
            cmd: Some(WindowsCmd::Lite { cmd: None }),
            ..
        } => lite(ctx),
        Cmd::Windows {
            cmd: Some(WindowsCmd::Lite {
                cmd: Some(LiteCmd::Stop),
            }),
            ..
        } => stop(ctx, Some(Which::Lite), false, 120, true),
        Cmd::Winapps { cmd } => {
            let mut c = match cmd {
                WinappsCmd::Setup => with_args(ctx.cmd("winapps-setup"), &["--user"]),
                WinappsCmd::Add => with_args(ctx.cmd("winapps-setup"), &["--user", "--add-apps"]),
                WinappsCmd::Desktop => with_args(ctx.cmd("winapps"), &["windows"]),
            };
            ctx.run(&mut c)
        }
    }
}

fn with_args(mut c: Command, args: &[&str]) -> Command {
    c.args(args);
    c
}

// ── hosts/<host>/vfio.nix ──

/// The parts of a host's `vfio` (modules/hosts/<host>/vfio.nix) vx needs.
#[derive(Debug, PartialEq, Eq)]
struct Vfio {
    gpu: String,
    gpu_audio: String,
    gpu_rom: Option<String>,
    reserve_gpu: bool,
    windows_nvme: Option<String>,
}

impl Vfio {
    /// PCI devices the GPU VM takes, labelled.
    fn devices(&self) -> Vec<(&'static str, &str)> {
        let mut d = vec![
            ("gpu", self.gpu.as_str()),
            ("gpuAudio", self.gpu_audio.as_str()),
        ];
        if let Some(n) = &self.windows_nvme {
            d.push(("windowsNvme", n.as_str()));
        }
        d
    }
}

/// This host's passthrough settings: `vfio = { ... };` in
/// modules/hosts/<host>/vfio.nix. No file (a host without passthrough) is None.
fn read_vfio(ctx: &Ctx) -> Result<Option<Vfio>> {
    let path = ctx.repo()?.join(vfio_rel(ctx.nixos_host()));
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
    };
    parse_vfio(&text).with_context(|| format!("could not parse vfio in {}", path.display()))
}

fn vfio_rel(host: &str) -> String {
    format!("modules/hosts/{host}/vfio.nix")
}

/// Find `vfio = { ... };` (or `vfio = null;`) and pull out its scalar keys.
fn parse_vfio(text: &str) -> Result<Option<Vfio>> {
    // Comments out of the way first, so their braces and examples don't count.
    let code: String = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let start = Regex::new(r"(?m)^\s*vfio\s*=\s*")?;
    let m = start.find(&code).context("no `vfio =` in the file")?;
    let rest = &code[m.end()..];
    if rest.trim_start().starts_with("null") {
        return Ok(None);
    }
    if !rest.starts_with('{') {
        bail!("`vfio =` is neither null nor an attribute set");
    }
    // The block, up to its matching brace.
    let mut depth = 0usize;
    let mut end = None;
    for (i, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let block = &rest[1..end.context("unclosed `vfio = {`")?];

    // Only keys at the block's own level (not inside the usb list's sets).
    let mut top = String::new();
    let mut depth = 0usize;
    for ch in block.chars() {
        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => top.push(ch),
            _ => {}
        }
    }
    let kv = Regex::new(r#"(\w+)\s*=\s*("([^"]*)"|true|false|null)\s*;"#)?;
    let mut vals: HashMap<String, Option<String>> = HashMap::new();
    for c in kv.captures_iter(&top) {
        let v = match &c[2] {
            "null" => None,
            s if s.starts_with('"') => Some(c[3].to_string()),
            s => Some(s.to_string()),
        };
        vals.entry(c[1].to_string()).or_insert(v);
    }
    let get = |k: &str| vals.get(k).cloned().flatten();
    Ok(Some(Vfio {
        gpu: get("gpu").context("vfio has no gpu")?,
        gpu_audio: get("gpuAudio").context("vfio has no gpuAudio")?,
        gpu_rom: get("gpuRom"),
        reserve_gpu: get("reserveGpu").as_deref() == Some("true"),
        windows_nvme: get("windowsNvme"),
    }))
}

// ── libvirt helpers ──

fn virsh(ctx: &Ctx) -> Command {
    let mut c = ctx.cmd("virsh");
    c.args(["-c", URI]);
    c
}

/// `running`, `shut off`, ...; None when the VM isn't defined.
fn domstate(ctx: &Ctx, name: &str) -> Option<String> {
    ctx.probe(virsh(ctx).args(["domstate", name]))
        .map(|s| s.trim().to_string())
}

fn is_running(ctx: &Ctx, name: &str) -> bool {
    domstate(ctx, name).as_deref() == Some("running")
}

/// The driver bound to a PCI device, or None.
fn pci_driver(addr: &str) -> Option<String> {
    let link = std::fs::read_link(format!("/sys/bus/pci/devices/{addr}/driver")).ok()?;
    link.file_name().map(|n| n.to_string_lossy().into_owned())
}

// ── commands ──

fn status(ctx: &Ctx) -> Result<()> {
    ui::heading("vms");
    let state = |n: &str| domstate(ctx, n).unwrap_or_else(|| "not defined (vx vm setup)".into());
    ui::kv(&[(LITE, state(LITE)), (GPU_VM, state(GPU_VM))]);

    ui::heading("gpu passthrough");
    match read_vfio(ctx) {
        Ok(None) => ui::info("no vfio in this host's modules/hosts/<host>/vfio.nix: no GPU VM"),
        Err(e) => ui::warn(&format!("{e:#}")),
        Ok(Some(v)) => {
            let mut rows: Vec<(&str, String)> = v
                .devices()
                .into_iter()
                .map(|(k, a)| {
                    (
                        k,
                        format!(
                            "{a}  {}",
                            pci_driver(a).unwrap_or_else(|| "no driver".into())
                        ),
                    )
                })
                .collect();
            rows.push((
                "reserveGpu",
                if v.reserve_gpu {
                    "true (vfio-pci from boot)".into()
                } else {
                    "false (Linux owns the card)".into()
                },
            ));
            let rom = v.gpu_rom.as_deref().unwrap_or(DEFAULT_ROM);
            rows.push((
                "vbios",
                format!(
                    "{rom}  {}",
                    if Path::new(rom).exists() {
                        "saved"
                    } else {
                        "missing (vx vm vbios)"
                    }
                ),
            ));
            rows.push((
                "used since boot",
                if Path::new(GPU_USED).exists() {
                    "yes (reboot before Linux can have it)".into()
                } else {
                    "no".into()
                },
            ));
            ui::kv(&rows);
        }
    }

    ui::heading("host");
    let hp = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| hugepages(&m));
    let hp = match hp {
        Some((total, free, kb)) => format!("{free} of {total} free ({} MiB each)", kb / 1024),
        None => "unknown".into(),
    };
    ui::kv(&[("hugepages", hp), ("libvirt hook", hook_state())]);
    Ok(())
}

fn hook_state() -> String {
    let p = Path::new(HOOK);
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.file_type().is_symlink() => match std::fs::metadata(p) {
            Ok(_) => "linked".into(),
            Err(_) => "linked, but the target is gone (vx vm setup)".into(),
        },
        Ok(_) => "present (not a link)".into(),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            "unknown (permission denied)".into()
        }
        Err(_) => "not linked (vx vm setup)".into(),
    }
}

/// (HugePages_Total, HugePages_Free, Hugepagesize in kB) from /proc/meminfo.
fn hugepages(meminfo: &str) -> Option<(u64, u64, u64)> {
    let field = |k: &str| {
        meminfo
            .lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix(':'))
            .and_then(|v| v.split_whitespace().next()?.parse().ok())
    };
    Some((
        field("HugePages_Total")?,
        field("HugePages_Free")?,
        field("Hugepagesize")?,
    ))
}

fn stop(ctx: &Ctx, which: Option<Which>, force: bool, timeout: u64, yes: bool) -> Result<()> {
    let which = match which {
        Some(w) => w,
        None => {
            let running: Vec<Which> = [Which::Lite, Which::Gpu]
                .into_iter()
                .filter(|w| is_running(ctx, w.name()))
                .collect();
            match running.as_slice() {
                [] => {
                    ui::ok("no Windows VM is running");
                    return Ok(());
                }
                [w] => *w,
                _ => bail!("both VMs are running; say which: vx vm stop lite | vx vm stop gpu"),
            }
        }
    };
    let name = which.name();
    match domstate(ctx, name).as_deref() {
        None => bail!("{name} isn't defined (vx vm setup)"),
        Some("shut off") => {
            ui::ok(&format!("{name} is already shut off"));
            return Ok(());
        }
        _ => {}
    }
    if which == Which::Gpu {
        ui::warn("the PC reboots once Windows shuts down (the card can't go back to Linux)");
        if !yes && !ui::confirm(&format!("shut {name} down?")) {
            bail!("cancelled");
        }
    }

    ctx.run(virsh(ctx).args(["shutdown", name]))?;
    ui::info(&format!(
        "waiting for {name} to shut down (up to {timeout}s)"
    ));
    let deadline = Instant::now() + Duration::from_secs(timeout);
    while Instant::now() < deadline {
        if !matches!(
            domstate(ctx, name).as_deref(),
            Some("running" | "in shutdown")
        ) {
            ui::ok(&format!("{name} is shut off"));
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !force {
        bail!(
            "{name} is still running after {timeout}s; `vx vm stop {} --force` pulls the plug",
            which_arg(which)
        );
    }
    ui::warn(&format!("{name} didn't shut down; forcing it off"));
    ctx.run(virsh(ctx).args(["destroy", name]))?;
    ui::ok(&format!("{name} is off"));
    Ok(())
}

fn which_arg(w: Which) -> &'static str {
    match w {
        Which::Lite => "lite",
        Which::Gpu => "gpu",
    }
}

/// virt-viewer on win11-lite, in the background (like `just windows-lite`'s `&`).
fn open_viewer(ctx: &Ctx) -> Result<()> {
    let mut c = ctx.cmd("virt-viewer");
    c.args([
        "-c",
        URI,
        "--attach",
        "--wait",
        "--auto-resize=always",
        LITE,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    if ctx.verbose {
        ui::dim(&format!(
            "$ virt-viewer -c {URI} --attach --wait --auto-resize=always {LITE} &"
        ));
    }
    c.spawn().context("could not start virt-viewer")?;
    Ok(())
}

fn console(ctx: &Ctx) -> Result<()> {
    match domstate(ctx, LITE).as_deref() {
        None => bail!("{LITE} isn't defined (vx vm setup)"),
        Some("running") => open_viewer(ctx),
        Some(s) => bail!("{LITE} is {s}; `vx windows lite` starts it and opens the window"),
    }
}

fn lite(ctx: &Ctx) -> Result<()> {
    if !is_running(ctx, LITE) {
        ctx.run(virsh(ctx).args(["start", LITE]))?;
    }
    open_viewer(ctx)
}

fn windows(ctx: &Ctx, yes: bool) -> Result<()> {
    let vfio = read_vfio(ctx).unwrap_or_else(|e| {
        ui::warn(&format!("{e:#}"));
        None
    });
    if vfio.is_none() && !Path::new(TEMPLATES).join(format!("{GPU_VM}.xml")).exists() {
        bail!("no GPU VM on this PC (this host has no vfio); `vx windows lite` is the light VM");
    }
    if vfio.as_ref().is_some_and(|v| !v.reserve_gpu) {
        ui::warn("GPU passthrough is disabled on this PC: the card gives no picture in Windows");
    }
    ui::warn("this takes the GPU from the desktop (your session closes)");
    ui::warn("when Windows shuts down, the PC reboots back into Linux");
    if !yes && !ui::confirm("start win11 anyway?") {
        bail!("cancelled; `vx windows lite` is the light VM in a window");
    }
    ctx.run(ctx.sudo("virsh").args(["start", GPU_VM]))
}

fn gpu_list(ctx: &Ctx) -> Result<()> {
    let filter = Regex::new(r"(?i)vga|audio|non-volatile")?;
    let vfio = read_vfio(ctx).ok().flatten();
    let out = ctx.output(ctx.cmd("lspci").arg("-nnD"))?;
    let mut rows: Vec<(Option<u64>, String)> = out
        .lines()
        .filter(|l| filter.is_match(l))
        .map(|l| {
            let addr = l.split_whitespace().next().unwrap_or("");
            let group = std::fs::read_link(format!("/sys/bus/pci/devices/{addr}/iommu_group"))
                .ok()
                .and_then(|p| p.file_name()?.to_str()?.parse().ok());
            let tag = vfio
                .as_ref()
                .and_then(|v| v.devices().into_iter().find(|(_, a)| *a == addr))
                .map(|(k, _)| format!("  <- vfio.{k}"))
                .unwrap_or_default();
            (group, format!("{l}{tag}"))
        })
        .collect();
    rows.sort_by(|a, b| (a.0.unwrap_or(u64::MAX), &a.1).cmp(&(b.0.unwrap_or(u64::MAX), &b.1)));
    for (g, line) in rows {
        let g = g.map(|g| g.to_string()).unwrap_or_else(|| "?".into());
        println!("group {g:<3} {line}");
    }
    Ok(())
}

fn vbios(ctx: &Ctx, gpu: Option<String>) -> Result<()> {
    let vfio = read_vfio(ctx).ok().flatten();
    let gpu = gpu
        .or_else(|| vfio.as_ref().map(|v| v.gpu.clone()))
        .unwrap_or_else(|| DEFAULT_GPU.into());
    let rom = vfio
        .and_then(|v| v.gpu_rom)
        .unwrap_or_else(|| DEFAULT_ROM.into());
    if !pci_addr_ok(&gpu) {
        bail!("{gpu} doesn't look like a PCI address (0000:03:00.0; see vx vm gpu)");
    }
    let d = format!("/sys/bus/pci/devices/{gpu}");
    if !Path::new(&d).exists() {
        bail!("no PCI device {gpu} (see vx vm gpu)");
    }
    let tmp = "/tmp/vbios.rom";
    ctx.run(ctx.sudo("sh").args([
        "-c",
        &format!("echo 1 > {d}/rom; cat {d}/rom > {tmp}; echo 0 > {d}/rom"),
    ]))?;
    let mut head = [0u8; 2];
    let valid = std::fs::File::open(tmp)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .is_ok()
        && head == [0x55, 0xaa];
    if !valid {
        let _ = ctx.run(ctx.sudo("rm").args(["-f", tmp]));
        bail!("not a video BIOS; is the card in use by the VM?");
    }
    ctx.run(ctx.sudo("install").args(["-D", "-m", "0644", tmp, &rom]))?;
    // The temp file is root's (and /tmp is sticky), so it takes sudo to remove.
    ctx.run(ctx.sudo("rm").args(["-f", tmp]))?;
    let size = std::fs::metadata(&rom)
        .map(|m| m.len().to_string())
        .unwrap_or_else(|_| "?".into());
    ui::ok(&format!("saved {rom} ({size} bytes)"));
    Ok(())
}

fn pci_addr_ok(a: &str) -> bool {
    Regex::new(r"^[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]$").is_ok_and(|r| r.is_match(a))
}

fn setup(ctx: &Ctx) -> Result<()> {
    // Relink the libvirt hooks (GPU passthrough) after a config change; a
    // rebuild alone doesn't, and without them `vx windows` black-screens.
    ctx.run(ctx.sudo("systemctl").args([
        "restart",
        "libvirtd-config.service",
        "libvirtd.service",
    ]))?;
    ctx.run(ctx.sudo("virsh").args(["net-autostart", "default"]))?;
    let _ = ctx.probe(ctx.sudo("virsh").args(["net-start", "default"]));
    for name in [LITE, GPU_VM] {
        let xml = Path::new(TEMPLATES).join(format!("{name}.xml"));
        if !xml.exists() {
            continue;
        }
        // Keep the existing UUID so libvirt redefines the VM instead of
        // refusing a second one with the same name.
        match ctx.probe(ctx.sudo("virsh").args(["domuuid", name])) {
            Some(uuid) if !uuid.trim().is_empty() => {
                let text = std::fs::read_to_string(&xml)
                    .with_context(|| format!("could not read {}", xml.display()))?;
                let tmp = std::env::temp_dir().join(format!("vx-{name}.xml"));
                std::fs::write(&tmp, with_uuid(&text, name, uuid.trim()))?;
                let r = ctx.run(ctx.sudo("virsh").arg("define").arg(&tmp));
                let _ = std::fs::remove_file(&tmp);
                r?;
            }
            _ => ctx.run(ctx.sudo("virsh").arg("define").arg(&xml))?,
        }
    }
    ui::ok("VMs defined");
    Ok(())
}

/// Insert `<uuid>` after the domain's `<name>`, like the justfile's sed.
fn with_uuid(xml: &str, name: &str, uuid: &str) -> String {
    let tag = format!("<name>{name}</name>");
    xml.replacen(&tag, &format!("{tag}<uuid>{uuid}</uuid>"), 1)
}

fn disk(ctx: &Ctx, name: &str, size: &str, yes: bool) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        bail!("{name:?} isn't a VM name (win11-lite, win11)");
    }
    let img = format!("{IMAGES}/{name}.img");
    if Path::new(&img).exists() {
        ui::warn(&format!(
            "{img} exists; a new image wipes that VM's Windows"
        ));
        if !yes && !ui::confirm("overwrite it?") {
            bail!("cancelled");
        }
    }
    ctx.run(
        ctx.sudo("qemu-img")
            .args(["create", "-f", "raw", &img, size]),
    )
}

fn virtio(ctx: &Ctx) -> Result<()> {
    let iso = format!("{IMAGES}/virtio-win.iso");
    ctx.run(ctx.sudo("curl").args(["-fL", "-o", &iso, VIRTIO_URL]))?;
    ui::ok(&format!("saved {iso}"));
    let win = format!("{IMAGES}/Win11.iso");
    if !Path::new(&win).exists() {
        ui::info(&format!("now put your Windows 11 ISO at {win}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"
    # GPU passthrough. null = libvirt only. e.g.
    #   { gpu = "0000:99:00.0"; gpuAudio = "x"; }
    vfio = {
      gpu = "0000:03:00.0"; # RX 7900 XTX
      gpuAudio = "0000:03:00.1"; # its audio
      reserveGpu = false; # true: no picture (tested Nov 2026)
      gpuAudioId = "1002:ab30"; # from `just vm-gpu`
      gpuRom = "/var/lib/libvirt/vbios/rx7900xtx.rom";
      usb = [
        {
          vendor = "0x3434";
          product = "0x0280";
        } # Keychron { weird }
      ];
      windowsNvme = null;
    };
    windowsUser = "vainnor";
    "#;

    #[test]
    fn parses_vfio() {
        let v = parse_vfio(META).unwrap().unwrap();
        assert_eq!(
            v,
            Vfio {
                gpu: "0000:03:00.0".into(),
                gpu_audio: "0000:03:00.1".into(),
                gpu_rom: Some("/var/lib/libvirt/vbios/rx7900xtx.rom".into()),
                reserve_gpu: false,
                windows_nvme: None,
            }
        );
        assert_eq!(v.devices().len(), 2);
    }

    #[test]
    fn parses_vfio_true_and_nvme() {
        let m = META
            .replace("reserveGpu = false", "reserveGpu = true")
            .replace("windowsNvme = null", r#"windowsNvme = "0000:05:00.0""#);
        let v = parse_vfio(&m).unwrap().unwrap();
        assert!(v.reserve_gpu);
        assert_eq!(v.devices()[2], ("windowsNvme", "0000:05:00.0"));
    }

    #[test]
    fn parses_vfio_null() {
        assert_eq!(
            parse_vfio("  # vfio = { gpu = \"a\"; };\n  vfio = null;\n").unwrap(),
            None
        );
    }

    #[test]
    fn parses_real_vfio() {
        assert_eq!(vfio_rel("arc"), "modules/hosts/arc/vfio.nix");
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../modules/hosts/arc/vfio.nix");
        if let Ok(text) = std::fs::read_to_string(p) {
            let v = parse_vfio(&text).unwrap();
            if let Some(v) = v {
                assert!(pci_addr_ok(&v.gpu) && pci_addr_ok(&v.gpu_audio));
            }
        }
    }

    #[test]
    fn hugepages_from_meminfo() {
        let m = "MemTotal: 1 kB\nHugePages_Total:    8192\nHugePages_Free:     100\nHugePages_Rsvd: 0\nHugepagesize:       2048 kB\n";
        assert_eq!(hugepages(m), Some((8192, 100, 2048)));
        assert_eq!(hugepages("MemTotal: 1 kB\n"), None);
    }

    #[test]
    fn uuid_goes_after_name() {
        let x = "<domain><name>win11</name><memory/></domain>";
        assert_eq!(
            with_uuid(x, "win11", "abc"),
            "<domain><name>win11</name><uuid>abc</uuid><memory/></domain>"
        );
        // win11 must not match win11-lite's name tag.
        let l = "<domain><name>win11-lite</name></domain>";
        assert_eq!(with_uuid(l, "win11", "abc"), l);
    }

    #[test]
    fn pci_addresses() {
        assert!(pci_addr_ok("0000:03:00.0"));
        assert!(!pci_addr_ok("03:00.0"));
        assert!(!pci_addr_ok("0000:03:00.0; reboot"));
    }
}
