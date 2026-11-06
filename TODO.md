# TODO

Things decided but not done yet, or that need the real hardware first.

## After the first NixOS boot
- [ ] **Mount the Windows data drives.** Seagate 2TB (`D:`) and Sabrent 1TB (`L:`) with the kernel's `ntfs3` driver, read-write or read-only (decide then). The Windows system drive (Sabrent 2TB, `C:`) stays untouched. Needs their UUIDs from `lsblk -f` on the PC.
- [ ] **BEACN Studio on Linux.** Try [beacn-utility](https://github.com/beacn-on-linux/beacn-utility) (not in nixpkgs yet) to control the Studio's mixes and mic settings from Linux. If it works, package it here with its udev rules.
- [ ] **LibreWolf data.** Export Sidebery and Stylus on the Mac and import them on the PC (README > After installing > LibreWolf).
- [ ] Set `meta.audioOutput` (`just audio-devices`) and `meta.replayMonitor` (`gpu-screen-recorder --list-monitors`, the ultrawide).
- [ ] Check Mango's monitor rules match both screens (`wlr-randr`).
- [ ] Check OBS loads the seeded profile and scene collections, and that VA-API encoding picks the 7900 XTX (disable the iGPU in the BIOS if it doesn't).
- [ ] Fill in `wifi/*`, `netbird/setup_key` and `restic/*` with `just secrets-edit`, then turn on `meta.resticReady`.
- [ ] Add the two scrape targets to the homelab Prometheus (README, Monitoring).
- [ ] Add `meta.signingKey` to GitHub as a signing key; make sure the key is in 1Password.
- [ ] Homelab binary cache: run the Attic setup on the homelab, then fill in `meta.binaryCache` and `attic/token`.
- [ ] RustDesk: check whether incoming sessions work on Mango. If they don't, Sunshine or SSH covers it.
