<p align="center">
  <img src="assets/readme/banner.svg" alt="vainnor/nixos: one flake for the PC and the Mac, installed with one command" width="100%">
</p>

<p align="center">
  <a href="https://github.com/Vainnor/nixos/actions/workflows/secrets.yml"><img alt="secrets" src="https://github.com/Vainnor/nixos/actions/workflows/secrets.yml/badge.svg?branch=main"></a>
  <img alt="NixOS: 26.11" src="https://img.shields.io/badge/NixOS-26.11-B5A8F5?style=flat-square&labelColor=1D232E&logo=nixos&logoColor=F5F7FB"> <img alt="nix-darwin: Mac" src="https://img.shields.io/badge/nix--darwin-Mac-B5A8F5?style=flat-square&labelColor=1D232E&logo=apple&logoColor=F5F7FB"> <img alt="theme: matches the wallpaper" src="https://img.shields.io/badge/theme-matches%20the%20wallpaper-243142?style=flat-square&labelColor=1D232E">
</p>
<p align="center">
  <img alt="aspects: 7" src="https://img.shields.io/badge/aspects-7-B5A8F5?style=flat-square&labelColor=1D232E"> <img alt="modules: 60" src="https://img.shields.io/badge/modules-60-B5A8F5?style=flat-square&labelColor=1D232E"> <a href="TODO.md"><img alt="read: the todo list" src="https://img.shields.io/badge/read-the%20todo%20list-243142?style=flat-square&labelColor=1D232E"></a>
</p>

---

NixOS for **arc** (Ryzen 7 9800X3D · RX 7900 XTX · 32 GB), dual-booting with Windows on its own
drive, plus the MacBook through nix-darwin. **One flake describes both, and one command installs the PC.**
The layout is [dendritic](https://vimjoyer.com/nix/dendritic/): every file under `modules/` is a
flake-parts module for one feature, holding its NixOS and home-manager halves side by side. A machine
is just the list of aspects it picks. Day to day, `vx` (`tools/vx`, Rust) is the one command for
everything, on both machines; `vx` on its own lists it all (see [Day to day](#day-to-day)).

<p align="center">
  <img src="assets/readme/architecture.svg" alt="Flake inputs feed the modules directory, where seven aspects (base, desktop, gaming, streaming, dev, vm, privacy) and meta.nix live. arc, the Mac and the installer pick from them. arc's drive is LUKS2 with btrfs subvolumes, and / is a tmpfs wiped every boot." width="100%">
</p>

## What's in it

| Aspect | What you get |
|---|---|
| `base` | systemd-boot via lanzaboote, LUKS + TPM, impermanence, NetBird, SSH (NetBird only), sops, fish, hourly snapshots, restic, USBGuard, Prometheus exporters |
| `desktop` | Mango, greetd, vshell (our own Quickshell shell: bar, launcher with apps, clipboard, emoji, calculator and files, notifications, control center, volume OSD, power menu, lock screen), Plymouth, night light, PipeWire channels, Stylix, 1Password, Thunar, Cider, Vesktop, TeamSpeak |
| `gaming` | Steam + Proton-GE, gamescope, gamemode, MangoHud, instant replay, Sunshine, scx_lavd, Xbox/PlayStation controllers |
| `streaming` | OBS seeded with the Vainnor overlays: a profile, 3 themed scene collections, BEACN-style audio tracks, global hotkeys, an EasyEffects mic chain, Chatterino |
| `dev` | Zed, vis, git signed through 1Password, direnv + nix-direnv, nix-ld, rootless Docker, kubectl, k9s, OpenTofu, Ansible, per-project toolchains for Python, TS/JS, Java, Go, PHP, Rust and C/C++ |
| `vm` | libvirt: `win11-lite` in a window, WinApps; on arc also `win11` with the 7900 XTX passed through |
| `privacy` | i2pd, and a LibreWolf profile that only talks through it |
| `browser` | LibreWolf with Sidebery (vertical tabs), Stylus and 1Password, the tab strip hidden, square corners, about: pages in the system palette (on the Mac too) |

The graphics driver follows each host's `gpu` (`amd`: amdgpu, VA-API and LACT for the GPU fans;
`nvidia`; `intel`; `none`). arc also has CoolerControl for its CPU and case fans.

Shared values (user, theme, wallpaper) live in `modules/flake/meta.nix`; what differs per machine
(features, GPU, main monitor, audio output) lives in its `modules/hosts/<name>/host.json`.
**Change `theme` in meta.nix and everything re-themes.** The look is flat
and square everywhere (no rounded corners, shadows or blur, lowercase text labels), and the colours
come from the wallpaper (see [Wallpapers](#wallpapers)).

## Built with

Versions are the ones pinned in `flake.lock` today; `vx update` moves them.

| Layer | Tool | Version |
|---|---|---|
| **System** | <img alt="NixOS: 26.11 unstable" src="https://img.shields.io/badge/NixOS-26.11%20unstable-B5A8F5?style=flat-square&labelColor=1D232E&logo=nixos&logoColor=F5F7FB"> | `26.11 unstable` |
|  | <img alt="Linux: 7.3.1" src="https://img.shields.io/badge/Linux-7.3.1-B5A8F5?style=flat-square&labelColor=1D232E&logo=linux&logoColor=F5F7FB"> | `7.3.1` |
|  | <img alt="Nix: 2.34.9" src="https://img.shields.io/badge/Nix-2.34.9-B5A8F5?style=flat-square&labelColor=1D232E&logo=nixos&logoColor=F5F7FB"> | `2.34.9` |
| **Flake** | <img alt="flake-parts: dendritic" src="https://img.shields.io/badge/flake--parts-dendritic-243142?style=flat-square&labelColor=1D232E"> |  |
|  | <img alt="home-manager: unstable" src="https://img.shields.io/badge/home--manager-unstable-243142?style=flat-square&labelColor=1D232E"> |  |
|  | <img alt="nix-darwin: Mac" src="https://img.shields.io/badge/nix--darwin-Mac-243142?style=flat-square&labelColor=1D232E&logo=apple&logoColor=F5F7FB"> |  |
| **Desktop** | <img alt="Mango: nightly" src="https://img.shields.io/badge/Mango-nightly-243142?style=flat-square&labelColor=1D232E"> |  |
|  | <img alt="Ghostty: 1.4.0" src="https://img.shields.io/badge/Ghostty-1.4.0-B5A8F5?style=flat-square&labelColor=1D232E"> | `1.4.0` |
|  | <img alt="LibreWolf: 159.0" src="https://img.shields.io/badge/LibreWolf-159.0-B5A8F5?style=flat-square&labelColor=1D232E&logo=librewolf&logoColor=F5F7FB"> | `159.0` |
|  | <img alt="Quickshell: 0.3.2" src="https://img.shields.io/badge/Quickshell-0.3.2-B5A8F5?style=flat-square&labelColor=1D232E"> | `0.3.2` (vshell) |
|  | <img alt="Stylix: from the wallpaper" src="https://img.shields.io/badge/Stylix-from%20the%20wallpaper-243142?style=flat-square&labelColor=1D232E"> |  |
| **Graphics** | <img alt="Mesa RADV: 26.2.6" src="https://img.shields.io/badge/Mesa%20RADV-26.2.6-B5A8F5?style=flat-square&labelColor=1D232E&logo=amd&logoColor=F5F7FB"> | `26.2.6` |
|  | <img alt="PipeWire: 1.6.9" src="https://img.shields.io/badge/PipeWire-1.6.9-B5A8F5?style=flat-square&labelColor=1D232E"> | `1.6.9` |
| **Gaming** | <img alt="Steam: Proton GE 11-11" src="https://img.shields.io/badge/Steam-Proton%20GE%2011--11-B5A8F5?style=flat-square&labelColor=1D232E&logo=steam&logoColor=F5F7FB"> | `Proton GE 11-11` |
|  | <img alt="gamescope: 3.17.0" src="https://img.shields.io/badge/gamescope-3.17.0-B5A8F5?style=flat-square&labelColor=1D232E"> | `3.17.0` |
|  | <img alt="Sunshine: 2026.914" src="https://img.shields.io/badge/Sunshine-2026.914-B5A8F5?style=flat-square&labelColor=1D232E"> | `2026.914` |
|  | <img alt="scx_lavd: 1.1.4" src="https://img.shields.io/badge/scx__lavd-1.1.4-B5A8F5?style=flat-square&labelColor=1D232E"> | `1.1.4` |
|  | <img alt="gpu-screen-recorder: 6.1.6" src="https://img.shields.io/badge/gpu--screen--recorder-6.1.6-B5A8F5?style=flat-square&labelColor=1D232E"> | `6.1.6` |
| **Streaming** | <img alt="OBS Studio: 32.2.2" src="https://img.shields.io/badge/OBS%20Studio-32.2.2-B5A8F5?style=flat-square&labelColor=1D232E&logo=obsstudio&logoColor=F5F7FB"> | `32.2.2` |
| **Dev** | <img alt="Zed: 1.26.2" src="https://img.shields.io/badge/Zed-1.26.2-B5A8F5?style=flat-square&labelColor=1D232E&logo=zedindustries&logoColor=F5F7FB"> | `1.26.2` |
|  | <img alt="vis: in use" src="https://img.shields.io/badge/vis-in%20use-243142?style=flat-square&labelColor=1D232E"> |  |
|  | <img alt="Docker: rootless" src="https://img.shields.io/badge/Docker-rootless-243142?style=flat-square&labelColor=1D232E&logo=docker&logoColor=F5F7FB"> |  |
| **Security** | <img alt="lanzaboote: 1.2.0" src="https://img.shields.io/badge/lanzaboote-1.2.0-B5A8F5?style=flat-square&labelColor=1D232E"> | `1.2.0` |
|  | <img alt="LUKS2: TPM unlock" src="https://img.shields.io/badge/LUKS2-TPM%20unlock-243142?style=flat-square&labelColor=1D232E"> |  |
|  | <img alt="sops + age: 3.13.3" src="https://img.shields.io/badge/sops%20%2B%20age-3.13.3-B5A8F5?style=flat-square&labelColor=1D232E"> | `3.13.3` |
|  | <img alt="1Password: SSH agent" src="https://img.shields.io/badge/1Password-SSH%20agent-243142?style=flat-square&labelColor=1D232E&logo=1password&logoColor=F5F7FB"> |  |
|  | <img alt="USBGuard: in use" src="https://img.shields.io/badge/USBGuard-in%20use-243142?style=flat-square&labelColor=1D232E"> |  |
| **Network** | <img alt="NetBird: 0.82.0" src="https://img.shields.io/badge/NetBird-0.82.0-B5A8F5?style=flat-square&labelColor=1D232E&logo=wireguard&logoColor=F5F7FB"> | `0.82.0` |
|  | <img alt="i2pd: in use" src="https://img.shields.io/badge/i2pd-in%20use-243142?style=flat-square&labelColor=1D232E"> |  |
| **Backups** | <img alt="btrbk: 0.32.7" src="https://img.shields.io/badge/btrbk-0.32.7-B5A8F5?style=flat-square&labelColor=1D232E"> | `0.32.7` |
|  | <img alt="restic: 0.19.1" src="https://img.shields.io/badge/restic-0.19.1-B5A8F5?style=flat-square&labelColor=1D232E"> | `0.19.1` |

## Three things to know before changing anything

1. **The installer only erases the drives you pick.** It lists every disk, never offers the USB stick,
   asks twice for anything with Windows on it, shows in red what will be erased, and waits for you to type
   `YES`. The drives it set up are written to `modules/hosts/<name>/disks.json` (the machine's own
   folder); nothing else names a disk.
2. **`/` is wiped on every boot.** Anything not declared here or listed in
   `modules/system/impermanence.nix` is gone after a reboot. `vx persist-check` shows what would be lost.
3. **The repo is public; secrets are not.** They're sops-encrypted in `secrets/`, and CI fails on anything
   that looks like a leaked credential. The Mac's age key is the one thing this repo can't rebuild.

## Install

**1. Get the installer onto a USB stick.** Build the custom ISO with `vx iso` (x86_64 Linux; WSL works),
or `nix build .#nixosConfigurations.installer.config.system.build.isoImage`. Write it to a stick with
`vx iso --flash /dev/sdX` (or any image writer). Any other NixOS ISO works too, see the end of this section.

**2. BIOS.** Boot in UEFI mode. Turn Secure Boot **off**, only so the installer USB boots; it gets turned on
again after the install (see Secure Boot + TPM below). In Windows, turn off Fast Startup first.

**3. Boot the stick.** The installer starts by itself, full screen. Plug in Ethernet before it starts; for
Wi-Fi, quit it, run `nmtui`, then `vx-installer` to start it again. If the graphics don't come up it runs on
the plain console instead. Nothing is erased until the review screen.

The screens:
- **welcome**: checks root, UEFI, network, Secure Boot / Setup Mode and the TPM, and offers to fetch the
  latest repo from GitHub (default) or install the copy on the stick.
- **machine**: reinstall a machine that's already in the repo (arc), or set up a **new machine** (see
  [Another PC](#another-pc)).
- **previous install**: if a disk holds a NixOS install made by this flake, unlock it with its disk password
  and choose what to **carry over** into the new install (see below) and what happens to `/home`: **fresh**
  (empty), **keep** (reinstall onto the same disk without reformatting it: only the system and
  `/persistent` are replaced; encryption and swap stay as they were) or **back up** (copied to a partition
  that isn't being erased, before anything is wiped).
- **system drive**: the disk NixOS goes on. Disks with Windows on them need an extra typed confirmation.
- **data drives**: any other disk can be **formatted** as an extra btrfs drive (optionally encrypted; it
  unlocks at boot with a key file kept on `/persistent`) or **mounted as it is**, like the Windows data
  drives through `ntfs3` (read-write or read-only). Each gets a name and a mount path. A missing data
  drive never stops the boot.
- **options**: encryption, TPM auto-unlock, swap file size (zram is always on), hostname, username, time
  zone, theme and wallpaper.
- **passwords**: the disk passphrase and your login password (or the carried-over one).
- **review**: everything you picked, with what will be **erased** in red. Type `YES`.
- **progress**: each step with a live log, then a summary with what to do next, and reboot or a shell.

**Carry-over** copies state from the previous install's `/persistent`, so the new system starts where the
old one left off. Each item can be left out:
- the SSH host key: it's also the key that opens `secrets/`, so secrets work on the first boot;
- the NetBird login (same peer, same address);
- your Secure Boot keys: if they are still enrolled in the firmware, there's no Setup Mode step;
- the login password;
- Bluetooth pairings and saved Wi-Fi;
- everything else in `/persistent` (machine id, fan curves, VM state, ...).

**4. The commit.** The installer edits its copy of the repo (now `~/nixos` on the PC):
`modules/hosts/<name>/disks.json` (the drives), the machine's `host.json` and `_hardware.nix` for a new
machine, and `modules/flake/meta.nix` (username, time zone, theme, wallpaper). It commits them and pushes
to GitHub (you log in with a code on your phone). Without a network to GitHub it only commits; push later
with `vx sync`, because a clone without `disks.json` builds the default layout (one encrypted disk, no data
drives), not yours.

**5. First boot.** Pick the NixOS drive with F11 if Windows starts. Type the disk passphrase. On this first
boot your own Secure Boot keys are created (unless they were carried over) and the boot files are signed.
Log in, then run `sudo netbird up` (skip it if the login was carried over). For secrets, see below.

**Without the custom ISO**: from any NixOS installer, connect to the network and run
`sudo nix --extra-experimental-features 'nix-command flakes' run github:vainnor/nixos`. It's the same
installer, just on the console. From a clone of the repo: `sudo nix run ~/nixos#install`, or `vx install`.

## Another PC

The same config installs onto any x86_64 PC, and arc's files are never touched by it.

1. Boot the installer stick on the new PC (step 1 to 3 above).
2. On the **machine** screen choose **new machine** and give it a name (lowercase letters, digits and `-`,
   e.g. `forge`).
3. Pick its **features**: desktop, dev and privacy are on by default; gaming, streaming and vm are off.
   `base` is always on. The GPU is detected (`amd`, `nvidia`, `intel` or `none`).
4. Go through the rest of the screens as usual. The installer creates `modules/hosts/<name>/` with
   `host.json` (name, features, GPU), `_hardware.nix` (from `nixos-generate-config`) and `disks.json`, then
   commits it and pushes it to GitHub. From then on that PC is `nixosConfigurations.<name>`: rebuild it with
   `vx switch` like arc.

A host folder is all it takes; every `modules/hosts/<name>/host.json` becomes a machine
(`modules/flake/hosts.nix`). Settings that differ per machine go in its `host.json`:

| field | |
|---|---|
| `aspects` | the features, e.g. `["base", "desktop", "dev", "privacy"]` |
| `gpu` | `amd`, `nvidia`, `intel` or `none` |
| `primaryMonitor` | EDID model of the main screen (`wlr-randr`), or `null` for the first one |
| `audioOutput` | the real output's PipeWire name (`vx audio devices`), or `null` |
| `replayMonitor` | the screen the instant replay records (`"screen"` = the first) |
| `stateVersion` | set once at install, never changed |

Anything else only that machine needs goes in a `.nix` file in its folder, as
`flake.modules.nixos.<name>`; arc keeps its monitor layout, fans and GPU passthrough that way.

## Using this config yourself

Fork it. You don't have to do that by hand: log in to GitHub on the installer's first screens and it finds
your fork of `vainnor/nixos`, or offers to create one, and installs from it and pushes to it. Only the owner
can push to `vainnor/nixos` itself; everyone else's machines live in their own fork.

A fork's install makes the config yours: in `modules/flake/meta.nix` your username, name, email, SSH keys
(from `github.com/<you>.keys`, so the owner's key can never log in to your PC), signing key and repo URL
replace the owner's. Secrets start **off** (`secretsReady = false`, `resticReady = false`, no binary cache):
nothing reads the owner's `secrets/secrets.yaml`, and Wi-Fi, NetBird's setup key, stream keys and backups
are simply skipped until you make your own (see [Secrets](#after-installing-once)). `signingKey = null`
gives plain unsigned commits and `sshKeys = [ ]` allows no SSH logins. The owner-only values that stay
(`githubUser`, `macHost`, `macLibrewolfProfile`) do nothing on your machines; set the Mac ones to `null` or
to your own Mac.

## Impermanence

`/` is a RAM disk that gets wiped on every boot. Only the following survive:
- `/nix` (the system),
- `/home` (your files),
- `/var/log`,
- what `modules/system/impermanence.nix` lists under `/persistent`.

If a service forgets its state after a reboot, add its directory to that list. `vx persist-check` finds
files on `/` that would be lost and says how to keep each one.
Because `/etc/shadow` is wiped as well, your password is stored as a hash in `/persistent/passwords/<user>`. To change it, run
`vx password` (it applies at the next login), or by hand:

```
mkpasswd -m yescrypt | sudo tee /persistent/passwords/vainnor
```

## After installing (once)

<details>
<summary><b>Secure Boot + TPM unlock</b></summary>

1. On first boot, your own Secure Boot keys are created in `/var/lib/sbctl` (kept on `/persistent`) and the boot files are signed with them.
2. Reboot into the BIOS. Go to *Security > Secure Boot*, set the mode to **Custom**, then **delete all keys** (this is Setup Mode). Save and reboot. systemd-boot enrolls your keys plus Microsoft's; Windows and the GPU's option ROM need Microsoft's.
3. Back in the BIOS, turn Secure Boot **on**. Then run `vx secureboot`. It should show *Secure Boot: enabled*.
4. If you chose TPM unlock in the installer, that first boot with Secure Boot on enrolls the TPM by itself (`tpm-first-boot-enroll`, then it removes its one-time key). Otherwise, or to redo it, run `vx tpm enroll`. From then on the disk unlocks automatically as long as the Secure Boot state is unchanged. Your passphrase still works as a fallback.

Carried-over Secure Boot keys that are still enrolled in the firmware: the boot files were already signed during the install, so skip step 2; just turn Secure Boot on (step 3), and step 4 follows by itself.

</details>

<details>
<summary><b>Secrets (sops)</b></summary>

`secrets/secrets.yaml` is encrypted (it holds the Twitch stream key and the StreamElements ID and token). Only the recipients in `.sops.yaml` can open it.
1. If the SSH host key was carried over, secrets already work; skip this. Otherwise the installer's summary shows the PC's new age key, and `vx secrets host` shows it again.
2. On the Mac, run `vx secrets add-host age1...`. This re-encrypts the file for the PC and sets `meta.secretsReady = true`.
3. Run `vx secrets edit` to fill in real values, then `vx commit "..."` and `vx deploy`.

What's in there: the Twitch key and StreamElements token, the WinApps password, your Wi-Fi (`wifi/ssid`, `wifi/psk`), a NetBird setup key (`netbird/setup_key`, so the PC joins the mesh by itself) and the restic target (see Backups).

</details>

<details>
<summary><b>1Password</b></summary>

Sign in once, then in *Settings > Developer* turn on **Use the SSH agent** and **Integrate with 1Password CLI**. SSH and git signing then go through 1Password, and the LibreWolf extension connects to the app. Commits are signed with `meta.signingKey`; add it on GitHub as a **signing key** so they show as Verified. The key has to be in your 1Password vault, otherwise `git commit` fails.

</details>

<details>
<summary><b>Fan curves</b></summary>

Open **CoolerControl** from the launcher. Pick a fan, make a curve against a temperature (CPU, liquid, GPU), and apply it. The curves keep running after you close the app. For a quick switch, make a "quiet" and a "gaming" mode.
- Leave the graphics card's fans to **LACT**. Giving them a curve in CoolerControl too makes the two programs fight over the fan.
- If a case fan shows a speed but won't change, set its control mode to PWM in the BIOS (Hardware Monitor) first.
- `vx fans save` copies your curves into the repo; commit them. A reinstall then starts with the same curves.

</details>

<details>
<summary><b>Backups</b></summary>

- **Snapshots** (already on): `@home` and `@persistent` every hour, kept 48 h / 14 days / 6 weeks. `vx snapshots` lists them, `vx snapshot` takes one now (before something risky), and `vx restore <path>` brings a file back from one; old files are under `/.btrfs/@snapshots/`. They live on the same disk, so they protect against mistakes, not against a dead drive.
- **restic to the homelab**: put the repository URL, a password and (for S3/MinIO) keys in `restic/*` with `vx secrets edit`, then set `meta.resticReady = true` and deploy. It runs nightly at 03:30 (or at the next boot if the PC was off). `vx backup` runs it now, and `vx restore --restic <path>` gets files back from it into `/tmp/restore` (plain `vx restore <path>` picks a version from the local snapshots).

</details>

<details>
<summary><b>LibreWolf (Sidebery and Stylus)</b></summary>

The extensions, the sidebar layout and both stylesheets are in the config. What each extension
keeps inside itself is not: export it once on the Mac and import it on the PC.

- **Sidebery**: *Settings > Help > Export addon data*, then *Import addon data* on the PC.
- **Stylus**: *Manage > Backup > Export*, then *Import* on the PC.

Keep the two exports somewhere safe (1Password works). The stylesheets are in `assets/librewolf/`;
`userContent.css.in` takes its colours from the theme, so edit the rules there, not the colours.

</details>

<details>
<summary><b>Monitoring</b></summary>

Add `arc.netbird.cloud:9100` (node exporter) and `arc.netbird.cloud:9633` (SMART) as scrape targets in the homelab's Prometheus. Both ports are only open on NetBird.

</details>

<details>
<summary><b>Sunshine (game streaming)</b></summary>

Open `https://localhost:47990` on the PC, create the admin login, then pair Moonlight (Mac, phone, TV) with the PIN it shows. Works over NetBird when you're away.

</details>

<details>
<summary><b>USB devices</b></summary>

Devices that are plugged in at boot always work, and everything connected on the first boot is remembered. A new device plugged in later is blocked and a notification says so: `vx usb` shows it, `vx usb allow <n>` allows it for good.

</details>

<details>
<summary><b>Homelab binary cache (Attic)</b></summary>

Once the homelab runs Attic, put its endpoint, cache name and public key in `meta.binaryCache` and the
push token in the secrets (`attic/token`), then deploy. Both machines download from it first, and the PC
pushes everything it builds, so the Mac and any reinstall reuse those builds. If the homelab is down, Nix
gives up on it after 5 seconds and carries on. `vx cache-push` pushes the running system by hand.

</details>

<details>
<summary><b>Windows VM with the GPU</b></summary>

1. Run `vx vm gpu` and `vx vm usb` on the PC. Fill in `vfio` in `modules/hosts/arc/vfio.nix` with the GPU, its audio function, and the USB devices to hand over (keyboard, mouse, BEACN, headset, webcam). Then run `vx switch`. (Passthrough is arc's alone; another host with the `vm` feature gets libvirt and `win11-lite` only, unless it gets its own `vfio.nix`.)
2. Put a Windows 11 ISO at `/var/lib/libvirt/images/Win11.iso`. Then run `vx vm virtio`, `vx vm disk win11 250G` and `vx vm setup`.
3. `vx windows` starts the VM. Your Linux session closes, and Windows takes over the monitors at near-native speed. When you shut Windows down, you're back at the Linux login.
   The first time, install Windows and load the virtio drivers from the second CD drive.
   To boot your existing Windows install instead, set `vfio.windowsNvme` to its NVMe's PCI address. Windows may then ask you to re-activate.

</details>

<details>
<summary><b>Light Windows VM (no GPU passthrough)</b></summary>

`win11-lite` is for small Windows-only apps. It runs in a window on your Linux desktop and uses 4 cores / 8 threads and 8 GB of RAM. Graphics are a basic virtual display, so it handles office tools and 2D apps but not games.
1. Run `vx vm setup` (once), `vx vm virtio` and `vx vm disk win11-lite 80G`.
2. Put a Windows 11 ISO at `/var/lib/libvirt/images/Win11.iso`, then run `vx windows lite`.
3. Setup runs on its own: Windows 11 Pro, a local account named `meta.windowsUser` (no Microsoft account), and privacy settings turned down.
4. Log in with the password `changeme`. A first-login script then:
   - turns telemetry down to the minimum Pro allows, and switches off ads, suggestions, Copilot, Recall, web search in Start, activity history and location;
   - removes OneDrive and the preinstalled store apps;
   - installs the virtio drivers, the guest agent and the SPICE agent (clipboard, auto-resize);
   - in `win11-lite`, sets up WinApps.
   - in `win11-lite`, adds software OpenGL 4.6 (Mesa llvmpipe) for CRC. There's no GPU in this VM, so apps that need OpenGL 3.2+ would otherwise refuse to start. The DLLs sit next to `CRC.exe` and are put back at every login if a CRC update removes them. For another app, copy `C:\Tools\mesa\x64\*.dll` next to its `.exe`.
5. The VM restarts after a minute. At the next login, Windows asks you to choose your own password.

The answer file is in `modules/virtualisation/_unattend.nix`. The full `win11` VM gets the same treatment when it installs onto a new disk image (not when it boots your real Windows drive). If you've already defined the VMs, run `vx vm setup` again to pick up changes.

`vx windows lite stop` shuts it down. The two Windows VMs have separate disks, so they don't affect each other.

</details>

<details>
<summary><b>WinApps (Windows apps as Linux windows)</b></summary>

WinApps runs apps from `win11-lite` as separate windows on your desktop (over RDP RemoteApp). They appear in the launcher (Super+Space) like normal apps.
1. `win11-lite` installs as Windows 11 **Pro** (Home has no RDP), with the account named in `meta.windowsUser`. Log in with a **password**, not a PIN.
2. The first-login script already ran the WINAPPS setup. To redo it, open the **WINAPPS** CD in the VM, right-click `install.bat`, choose *Run as administrator*, and reboot.
3. Set the password: run `vx secrets edit` and fill in `winapps.rdp_password`. If secrets aren't set up yet, a password box asks for it instead.
4. With the VM running, run `vx winapps setup` and pick apps. Later, `vx winapps add` adds apps you install, and `vx winapps desktop` opens the whole Windows desktop.

Your Linux home folder shows up in Windows as `\\tsclient\home`. The VM pauses itself after 5 idle minutes.

Games with kernel anti-cheat refuse to run in VMs. For those, boot Windows natively: use the power menu's **Reboot to Windows**, or press F11 at boot.

</details>

## Day to day

Everything goes through `vx` (`tools/vx`, Rust), on the PC and the Mac alike. Run `vx` to see every
command, `vx <command> --help` for one; tab completion works in fish. Before vx is installed (a
fresh Mac, say), `nix run .#vx -- <command>` runs it from the repo. It works on the repo at `~/nixos`
on the PC and `~/Programing/nixos` on the Mac; `--repo` (or `VX_REPO`) points it anywhere else. The common ones:

| Command | What it does |
|---|---|
| `vx status` | One screen: generation, repo, inputs, failed units, disks, backups, network |
| `vx doctor` | Deeper checks (secrets, SSH agent, signing, caches, the flake), each with its fix |
| `vx switch` | Rebuild and switch (darwin-rebuild on the Mac) |
| `vx test` / `vx boot` | Activate until reboot / activate on next boot only |
| `vx update` | Update all flake inputs (or `vx update nixpkgs`), show what changes, offer to switch |
| `vx upgrade` | Update all flake inputs, then switch |
| `vx rollback` | Go back to the previous generation |
| `vx clean` | Delete old generations (keeps the last 10 and 14 days) |
| `vx sync` | Format, check, commit, push, switch here, then offer the other machine |
| `vx deploy` | From the Mac: push, then pull and switch on the PC over NetBird SSH |
| `vx @arc …` / `vx @mac …` | Run any vx command on the other machine over SSH (`vx @arc status`) |
| `vx check` / `vx fmt` | Evaluate everything without building (works on the Mac) / format |
| `vx commit "msg"` | Format, check, and commit |
| `vx add <pkg>` / `vx remove <pkg>` | Install a package (into `modules/apps/extra.nix`; `--host arc`/`mac`) / take it out again |
| `vx try <pkg>` / `vx search <words>` | Run a package once without installing it / search the pinned nixpkgs |
| `vx persist-check` | Find files on `/` that would be lost at reboot, and how to keep them |
| `vx password` | Change your login password |
| `vx iso` / `vx iso --flash <device>` | Build the custom installer ISO / and write it to a USB stick |
| `vx secrets edit` | Edit the encrypted secrets (on the Mac) |
| `vx secureboot` / `vx tpm enroll` | Check Secure Boot / enroll the TPM unlock |
| `vx windows` | Start the Windows VM with the GPU passed through |
| `vx audio devices` | List audio outputs (set the real one as `audioOutput` in the host's `host.json`) |
| `vx obs reset` | Back up OBS settings and re-seed the Vainnor profile/scenes |
| `vx fans` / `vx fans save` | Temperatures and fan speeds / save the fan curves into the repo |
| `vx snapshots` / `vx snapshot` | List the hourly snapshots / take one now |
| `vx backup` / `vx restore <path>` | Run the restic backup now / bring a path back from a snapshot (`--restic`: from the homelab) |
| `vx usb` / `vx usb allow <n>` | List USB devices / allow a blocked one |
| `vx mac build` / `vx mac upgrade` | Build the Mac and show what would change / upgrade the Homebrew apps |
| `vx diff` / `vx tree` | What the working tree would change vs the running system / what takes space in the system |
| `vx new <lang> <dir>` | New project with its own toolchain (see Projects) |

### Keys worth knowing

| Keys | What it does |
|---|---|
| Super+Space · Super+V | Launcher · clipboard history |
| Super+; · Super+C · Super+/ | Emoji · calculator · find a file |
| Super+A · Super+Shift+N | Control center · notifications |
| Super+Shift+Escape · Super+Escape | Power menu (log out, reboot, Windows) · lock |
| Super+Alt+N | Night light on/off |
| Print · Shift+Print · Super+Print | Screenshot a region · the focused monitor · the focused window |
| Super+Shift+F | Thunar (Super+E is yazi) |
| Alt+F10 · Alt+F9 | Save the last 2 minutes · the last 30 seconds (instant replay, `~/Videos/Replays`) |
| Super+F1…F8 | OBS: switch scene |
| Super+F9 · Super+F10 · Super+Shift+F12 | OBS: record on/off · mic mute · **stream** on/off |

## Wallpapers

Each wallpaper is a folder in `assets/wallpapers/`: `wide.png` (3440x1440) goes on the ultrawide,
`4k.png` on the left monitor and the Mac, and `theme.nix` is a palette made from the image. Every
themed app (the shell, borders, terminal, LibreWolf, GTK, Qt, the lock screen) follows it.

```sh
vx wallpapers                       # list them; * marks the current one
vx wallpaper pinewood               # switch wallpaper + colours and rebuild this machine
vx wallpaper                        # same, picked from a list
vx wallpaper add forest wide.png 4k.png   # add one; makes its palette for you
vx wallpaper try forest             # on the Mac: preview its colours without changing anything
vx theme check forest               # check the palette's contrast
```

`vx wallpaper` changes two lines in `modules/flake/meta.nix` (`wallpaper` and `theme = "match"`),
so commit afterwards and the other machine follows on its next rebuild. To keep a fixed palette
whatever the wallpaper, set `theme` to `vainnor-slate`, `caelus` or any base16 name. If a generated
colour feels off, edit that wallpaper's `theme.nix` (`base0D` is the accent, `base00` the background).

## Projects

Nothing language-specific is installed system-wide. Each project carries its own toolchain in its
own `flake.nix`; direnv loads it when you `cd` in and drops it when you leave. Two projects on
Python 3.12 and 3.13 (or Node 20 and 22, JDK 17 and 21) never see each other, and deleting a
project leaves nothing behind. Works the same on the Mac.

```sh
vx new python ~/code/scraper     # python · node · java · go · php · rust · c
cd ~/code/scraper && uv init && uv add requests && uv run main.py
```

| Template | You get | Change the version |
|---|---|---|
| `python` | Python, uv (venv in `.venv`), ruff, basedpyright | `python = pkgs.python312;` |
| `node` | Node, pnpm, TypeScript, its language server, eslint/json/css servers | `node = pkgs.nodejs_20;` |
| `java` | JDK, Gradle, Maven, jdtls | `jdk = pkgs.jdk17;` |
| `go` | Go (never auto-downloads another), gopls, golangci-lint, delve | `go = pkgs.go_1_24;` |
| `php` | PHP with xdebug, Composer, phpactor | `php = pkgs.php83.buildEnv { ... };` |
| `rust` | cargo, rustc, clippy, rustfmt, rust-analyzer | pin nixpkgs, or add a rust overlay |
| `c` | clangd, clang-format, CMake, Ninja, lldb | |

Zed picks up each project's language servers from the same shell. For quick one-off scripts
outside a project, `uv run --python 3.12 script.py` works anywhere.

## The Mac

nix-darwin manages the shell (fish, starship, atuin, eza, bat, ...), git with signing, direnv, vis, the Stylix theme, the wallpaper, your Homebrew apps and macOS settings. It leaves Zed's settings, `~/.ssh/config` and your `starship.toml` alone. In LibreWolf it only writes the two stylesheets into your profile (`meta.macLibrewolfProfile`), never `profiles.ini`. Files it takes over (like `~/.config/fish/config.fish`) are kept next to it as `*.hm-backup`.

First time:
1. `sudo mv /etc/nix/nix.conf /etc/nix/nix.conf.before-nix-darwin` (nix-darwin manages it from now on). If it also complains about `/etc/bashrc` or `/etc/zshrc`, move those the same way.
2. `sudo nix --extra-experimental-features 'nix-command flakes' run nix-darwin -- switch --flake ~/Programing/nixos`
3. `vx mac adopt` hands the apps you installed by hand to Homebrew (one time; otherwise `brew bundle` stops at "already an App at ...").
4. After that, `vx switch`. Touch ID works for `sudo`.

Apps come from Homebrew (`modules/hosts/mac/homebrew.nix`); nothing is uninstalled automatically.
`vx mac upgrade` updates them. To undo the whole thing, `vx mac undo` removes nix-darwin and puts
back your settings, dotfiles and `/etc` files (`vx mac undo --dry` shows what it would do first);
`sudo darwin-rebuild --rollback` undoes just the last switch. macOS settings (dark mode, Dock auto-hide, Finder list view and path
bar, fast key repeat, no auto-correct, screenshots in `~/Pictures/Screenshots`, firewall on) are in
`modules/hosts/mac/settings.nix`.

## Streaming

On first launch OBS opens the **Vainnor** profile and the **Vainnor Purple** scene collection.
Mono and Horror are under *Scene Collection*. You still have to:
1. Run `vx audio devices` and put your real output in `audioOutput` in `modules/hosts/<name>/host.json`, then `vx switch`. This makes **System** the default output, so all app audio reaches OBS.
2. In pwvucontrol, move each app to its channel: games to **Game**, Vesktop/TeamSpeak to **Chat**, Cider to **Music**, LibreWolf to **Browser**. Set the BEACN mic as the default input. EasyEffects cleans it up (noise removal, gate, compressor, limiter) and OBS records the cleaned **Easy Effects Source**.
3. In OBS: add your Twitch stream key or account (*Settings > Stream*), and pick the Brio in the **Webcam** source if it isn't already selected.
4. In Steam, set each game's launch options to `obs-gamecapture %command%` so **Game Capture** sees it.
5. Optional: put your StreamElements ID and JWT in the secrets (`streamelements/*`). They're written into the overlays' `config.js` at every switch, and that file is never committed.

Audio tracks: 1 Audience (everything), 2 VOD (no music), 3 voices, 4 game + other. The stream uses Track 1, the Twitch VOD uses Track 2, and recordings get all four.

## Layout

| | |
|---|---|
| `modules/flake/` | flake plumbing: `meta.nix` (shared values), `hosts.nix` (a machine per host folder), the installer, home-manager wiring, dev shell |
| `modules/hosts/<name>/` | one folder per machine: `host.json` (features, GPU, monitor, audio), `_hardware.nix`, `disks.json`, and its own `.nix` files (arc: monitors, fans, GPU passthrough, `vx preview`) |
| `modules/hosts/` | also `_disko.nix` (the disk layout every machine shares), `mac`, the installer ISO |
| `modules/system/` | boot, security, impermanence, networking, secrets, backups, audio, USBGuard, monitoring |
| `modules/desktop/` · `apps/` | the Mango session and the apps on it |
| `modules/gaming/` · `streaming/` · `dev/` · `virtualisation/` · `privacy/` | one folder per aspect |
| `secrets/` | sops-encrypted; `.sops.yaml` says who can open it |
| `assets/` | the overlay pack and these README images |
| `templates/` | per-language project templates for `vx new` |

Files and folders whose path contains `/_` (like `hosts/_disko.nix`) aren't auto-loaded.

## If the drive dies

Snapshots live on the same disk, so they won't help. Install again (the custom ISO, or `nix run github:vainnor/nixos`),
add the new host key with `vx secrets add-host`, then `vx restore --restic /home/vainnor` (and anything from
`/persistent` you need) and copy it back from `/tmp/restore`. Without the Mac's age key, nothing in `secrets/` can be decrypted.
