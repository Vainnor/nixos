# TODO

Things decided but not done yet, or that need the real hardware first.

## After the first NixOS boot
- [ ] **Mount the Windows data drives.** Done in the installer now: on the data drives screen, set Seagate 2TB (`D:`) and Sabrent 1TB (`L:`) to "mount as it is" (`ntfs3`, read-write or read-only) with a path like `/mnt/d`. The Windows system drive (Sabrent 2TB, `C:`) stays untouched. Already installed? Add them as `"mode": "mount"` entries in `modules/hosts/arc/disks.json` (UUIDs from `lsblk -f`).
- [ ] **Push the installer's commit** (`modules/hosts/arc/disks.json` and the `meta.nix` edits) from `~/nixos` after `gh auth login`.
- [ ] **Test the new installer on the real PC** before trusting it with `/home`: the cage window on the 7900 XTX, TPM enrollment on the first Secure Boot boot (`systemctl status tpm-first-boot-enroll`), an encrypted data drive unlocking at boot, and a "keep /home" reinstall.
- [ ] **BEACN Studio on Linux.** Try [beacn-utility](https://github.com/beacn-on-linux/beacn-utility) (not in nixpkgs yet) to control the Studio's mixes and mic settings from Linux. If it works, package it here with its udev rules.
- [ ] **LibreWolf data.** Export Sidebery and Stylus on the Mac and import them on the PC (README > After installing > LibreWolf).
- [ ] Set `meta.audioOutput` (`vx audio devices`) and `meta.replayMonitor` (`gpu-screen-recorder --list-monitors`, the ultrawide).
- [ ] Check Mango's monitor rules match both screens (`wlr-randr`).
- [ ] Check OBS loads the seeded profile and scene collections, and that VA-API encoding picks the 7900 XTX (disable the iGPU in the BIOS if it doesn't).
- [ ] Fill in `wifi/*`, `netbird/setup_key` and `restic/*` with `vx secrets edit`, then turn on `meta.resticReady`.
- [ ] Add the two scrape targets to the homelab Prometheus (README, Monitoring).
- [ ] Add `meta.signingKey` to GitHub as a signing key; make sure the key is in 1Password.
- [ ] Homelab binary cache: run the Attic setup on the homelab, then fill in `meta.binaryCache` and `attic/token`.
- [ ] RustDesk: check whether incoming sessions work on Mango. If they don't, Sunshine or SSH covers it.
