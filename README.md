# nixos

NixOS for **arc** (Ryzen 7 9800X3D · RX 7900 XTX · 32 GB), dual-booting with Windows on a separate drive.
Mango (Wayland) · Ghostty · fuzzel · waybar · LibreWolf · Stylix theming · LUKS + btrfs.

## Layout (dendritic)

Every `.nix` file under `modules/` is a flake-parts module, loaded automatically by import-tree.
Files are organized by **feature**, and one file can hold both the NixOS and home-manager halves
of that feature (`flake.modules.nixos.<aspect>` / `flake.modules.homeManager.<aspect>`).
Files and folders whose path contains `/_` (for example `hosts/arc/_disko.nix`) are not auto-loaded.

| Aspect    | What's in it                                                                   |
|-----------|--------------------------------------------------------------------------------|
| `base`    | nix, boot, security, networking + NetBird, SSH (NetBird only), user, shell     |
| `desktop` | mango, greetd, waybar, fuzzel, mako, swaylock, audio, theme, apps              |
| `gaming`  | Steam + Proton-GE, gamescope, gamemode, MangoHud, amdgpu/LACT                  |
| `dev`     | Zed, vis, git, direnv, nix-ld, rootless Docker                                 |
| `privacy` | i2pd                                                                           |

`modules/hosts/arc/default.nix` picks the aspects for this machine. Shared values (username,
theme, time zone, SSH keys) live in `modules/flake/meta.nix`. **Change `theme` there to re-theme everything.**

## Install

Before you start: turn off Fast Startup in Windows and Secure Boot in the BIOS. Secure Boot can be turned back on later with lanzaboote.

Boot any NixOS installer USB. Connect Ethernet, or run `nmtui` for Wi-Fi. Then run:

```
sudo nix --extra-experimental-features 'nix-command flakes' run github:vainnor/nixos
```

The installer:
- finds the 1TB Crucial P310 by its model name;
- refuses to continue if it finds Windows/NTFS partitions on it, or if it's the USB stick;
- shows the disk and waits for you to type `YES`;
- asks for the disk-encryption password and your login password, then installs.

When it finishes, reboot. Choose the NixOS drive with F11 if Windows starts instead. Log in, then run `sudo netbird up`.

**Optional custom ISO**: the installer is preloaded, so you just run `sudo install-nixos`.
`nix build .#nixosConfigurations.installer.config.system.build.isoImage` (on x86_64 Linux; WSL works).

## Impermanence

`/` is a RAM disk that gets wiped on every boot. Only the following survive:
- `/nix` (the system),
- `/home` (your files),
- `/var/log`,
- what `modules/system/impermanence.nix` lists under `/persistent`.

If a service forgets its state after a reboot, add its directory to that list.
Because `/etc/shadow` is wiped as well, your password is stored as a hash in `/persistent/passwords/<user>`. To change it:

```
mkpasswd -m yescrypt | sudo tee /persistent/passwords/vainnor
```

## Day to day

```
nh os switch            # rebuild and switch
nh os switch --update   # update inputs, then rebuild
nix flake init -t ~/nixos#rust   # new Rust project with its own dev shell
```
