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
| `vm`      | libvirt + virt-manager, Windows 11 VM with the 7900 XTX passed through          |
| `streaming` | OBS with the Vainnor overlays (`assets/overlays`), 3 theme scene collections, BEACN-style tracks |

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

## After installing (once)

### Secure Boot + TPM unlock
1. On first boot, your own Secure Boot keys are created in `/var/lib/sbctl` (kept on `/persistent`).
2. Reboot into the BIOS. Go to *Security > Secure Boot*, set the mode to **Custom**, then **delete all keys** (this is Setup Mode). Save and reboot. systemd-boot enrolls your keys plus Microsoft's; Windows and the GPU's option ROM need Microsoft's.
3. Back in the BIOS, turn Secure Boot **on**. Then run `just secureboot-status`. It should show *Secure Boot: enabled*.
4. Run `just tpm-enroll`. From then on the disk unlocks automatically as long as the Secure Boot state is unchanged. Your passphrase still works as a fallback.

### Secrets (sops)
`secrets/secrets.yaml` is encrypted (it holds the Twitch stream key and the StreamElements ID and token). Only the recipients in `.sops.yaml` can open it.
1. The installer prints the PC's age key, and also saves it in `~/nixos/HOST_AGE_KEY.txt`. `just secrets-host` shows it again.
2. On the Mac, run `just secrets-add-host age1...`. This re-encrypts the file for the PC and sets `meta.secretsReady = true`.
3. Run `just secrets-edit` to fill in real values, then `just commit "..."` and `just deploy`.

### Windows VM with the GPU
1. Run `just vm-gpu` and `just vm-usb` on the PC. Fill in `meta.vfio` in `modules/flake/meta.nix` with the GPU, its audio function, and the USB devices to hand over (keyboard, mouse, BEACN, headset, webcam). Then run `just switch`.
2. Put a Windows 11 ISO at `/var/lib/libvirt/images/Win11.iso`. Then run `just vm-virtio`, `just vm-disk 250G` and `just vm-setup`.
3. `just windows` starts the VM. Your Linux session closes, and Windows takes over the monitors at near-native speed. When you shut Windows down, you're back at the Linux login.
   The first time, install Windows and load the virtio drivers from the second CD drive.
   To boot your existing Windows install instead, set `meta.vfio.windowsNvme` to its NVMe's PCI address. Windows may then ask you to re-activate.

Games with kernel anti-cheat refuse to run in VMs. For those, boot Windows natively: use the power menu's **Reboot to Windows**, or press F11 at boot.

## Day to day

Run `just` to see all recipes. The common ones:

| Command | What it does |
|---|---|
| `just switch` | Rebuild and switch (on the PC) |
| `just test` / `just boot` | Activate until reboot / activate on next boot only |
| `just upgrade` | Update all flake inputs, then switch |
| `just update nixpkgs` | Update a single input |
| `just rollback` | Go back to the previous generation |
| `just clean` | Delete old generations (keeps 10) |
| `just deploy` | From the Mac: push, then pull and switch on the PC over NetBird SSH |
| `just check` / `just fmt` | Evaluate everything without building (works on the Mac) / format |
| `just commit "msg"` | Format, check, and commit |
| `just ephemeral` | List files on `/` that would be lost at reboot |
| `just password` | Change your login password |
| `just iso` | Build the custom installer ISO |
| `just secrets-edit` | Edit the encrypted secrets (on the Mac) |
| `just secureboot-status` / `just tpm-enroll` | Check Secure Boot / enroll the TPM unlock |
| `just windows` | Start the Windows VM with the GPU passed through |
| `just audio-devices` | List audio outputs (set the real one as `meta.audioOutput`) |
| `just obs-reset` | Back up OBS settings and re-seed the Vainnor profile/scenes |

## Streaming

On first launch OBS opens the **Vainnor** profile and the **Vainnor Purple** scene collection.
Mono and Horror are under *Scene Collection*. You still have to:
1. Run `just audio-devices` and put your real output in `meta.audioOutput`, then `just switch`. This makes **System** the default output, so all app audio reaches OBS.
2. In pwvucontrol, move each app to its channel: games to **Game**, Vesktop/TeamSpeak to **Chat**, Spotify to **Music**, LibreWolf to **Browser**. Set the BEACN mic as the default input.
3. In OBS: add your Twitch stream key or account (*Settings > Stream*), and pick the Brio in the **Webcam** source if it isn't already selected.
4. In Steam, set each game's launch options to `obs-gamecapture %command%` so **Game Capture** sees it.
5. Optional: put your StreamElements ID and JWT in `~/.local/share/vainnor-overlays/js/config.js`. That file is never committed to git.

Audio tracks: 1 Audience (everything), 2 VOD (no music), 3 voices, 4 game + other. The stream uses Track 1, the Twitch VOD uses Track 2, and recordings get all four.

`nix flake init -t ~/nixos#rust` starts a new Rust project with its own dev shell.
