# TODO

Things decided but not done yet, or that need the real hardware first.

## After the first NixOS boot
- [ ] **Mount the Windows data drives.** Done in the installer now: on the data drives screen, set Seagate 2TB (`D:`) and Sabrent 1TB (`L:`) to "mount as it is" (`ntfs3`, read-write or read-only) with a path like `/mnt/d`. The Windows system drive (Sabrent 2TB, `C:`) stays untouched. Already installed? Add them as `"mode": "mount"` entries in `modules/hosts/arc/disks.json` (UUIDs from `lsblk -f`).
- [ ] **Push the installer's commit** if the install had no GitHub access: `vx sync` from `~/nixos` (`modules/hosts/arc/disks.json` and the `meta.nix` edits).
- [ ] **Test the new installer on the real PC** before trusting it with `/home`: the cage window on the 7900 XTX, TPM enrollment on the first Secure Boot boot (`systemctl status tpm-first-boot-enroll`), an encrypted data drive unlocking at boot, and a "keep /home" reinstall.
- [ ] **Test the automatic setup on the real PC**: Secure Boot keys enrolled by the installer in Setup Mode (then `sbctl status` after turning Secure Boot on; `prepare-sb-auto-enroll` ran once and isn't failed), the installer's NetBird login carried into the system under the right name (`netbird status`), and secrets on a new machine (`systemctl status vx-secrets-rekey vx-secrets-wait`, the hourly re-encrypt commit on GitHub, then the waiting machine switching and `/persistent/install/secrets-ok`).
- [ ] **BEACN Studio on Linux.** Try [beacn-utility](https://github.com/beacn-on-linux/beacn-utility) (not in nixpkgs yet) to control the Studio's mixes and mic settings from Linux. If it works, package it here with its udev rules.
- [ ] **LibreWolf data.** Export Sidebery and Stylus on the Mac and import them on the PC (README > After installing > LibreWolf).
- [ ] Set `audioOutput` (`vx audio devices`) and `replayMonitor` (`gpu-screen-recorder --list-monitors`, the ultrawide) in `modules/hosts/arc/host.json`.
- [ ] Check Mango's monitor rules match both screens (`wlr-randr`).
- [ ] Check OBS loads the seeded profile and scene collections, and that VA-API encoding picks the 7900 XTX (disable the iGPU in the BIOS if it doesn't).
- [ ] Fill in `wifi/*`, `netbird/setup_key` and `restic/*` with `vx secrets edit`, then turn on `meta.resticReady`.
- [ ] Add the two scrape targets to the homelab Prometheus (README, Monitoring).
- [ ] Add `meta.signingKey` to GitHub as a signing key; make sure the key is in 1Password.
- [ ] Homelab binary cache: run the Attic setup on the homelab, then fill in `meta.binaryCache` and `attic/token`.
- [ ] RustDesk: check whether incoming sessions work on Mango. If they don't, Sunshine or SSH covers it.

## Other machines
- [ ] **NVIDIA on real hardware.** `gpu = "nvidia"` (system/gpu.nix) is only evaluated, never booted: check Mango starts on it, the boot splash, suspend/resume, VA-API in LibreWolf (nvidia-vaapi-driver), NVENC in OBS and the instant replay, and whether `WLR_NO_HARDWARE_CURSORS=1` is needed (invisible or glitchy cursor).
- [ ] **A second machine end to end**: `vx install` > new machine on another PC, then `vx switch` there from the pushed folder.
