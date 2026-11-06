# Run `just` to list recipes. Recipes run from the repo root.

set shell := ["bash", "-euo", "pipefail", "-c"]

host := "arc"
mac := "Vainnors-MacBook-Pro"
remote := "vainnor@arc.netbird.cloud"
flake := justfile_directory()
nix := "nix --extra-experimental-features 'nix-command flakes'"

# List recipes
default:
    @just --list --unsorted

# ── Install (from the NixOS installer USB) ─────────────────────────────

# Erase the dedicated drive and install NixOS (asks for YES first)
install:
    sudo {{ nix }} run {{ flake }}#install -- {{ host }}

# Build the custom installer ISO (x86_64 Linux only; WSL works)
iso:
    {{ nix }} build {{ flake }}#nixosConfigurations.installer.config.system.build.isoImage
    @ls -lh result/iso/*.iso

# ── Day to day (on the PC) ─────────────────────────────────────────────

# Build and switch to the current config
switch:
    nh os switch {{ flake }}

# Build and activate without adding a boot entry (reverts on reboot)
test:
    nh os test {{ flake }}

# Build and make it the default for the next boot, without switching now
boot:
    nh os boot {{ flake }}

# Build only, and show what would change
build:
    nh os build {{ flake }}

# Update every flake input (or just one: `just update nixpkgs`)
update *input:
    {{ nix }} flake update {{ input }} --flake {{ flake }}

# Update inputs, then switch
upgrade: update switch

# Go back to the previous generation
rollback:
    sudo nixos-rebuild switch --rollback

# Remove old generations and unused store paths (keeps the last 10)
clean:
    nh clean all --keep 10 --keep-since 14d

# ── Deploy from another machine (the Mac) ──────────────────────────────

# Push, then pull and switch on the PC over NetBird SSH
deploy: push
    ssh -t {{ remote }} 'cd ~/nixos && git pull --ff-only && nh os switch .'

# Like deploy, but only activate on the next boot
deploy-boot: push
    ssh -t {{ remote }} 'cd ~/nixos && git pull --ff-only && nh os boot .'

# Push local commits to GitHub
push:
    git push

# ── The Mac (nix-darwin) ───────────────────────────────────────────────

# Build and switch the Mac (first time: see README "The Mac")
mac:
    sudo darwin-rebuild switch --flake {{ flake }}#{{ mac }}

# Build the Mac config only, and show what would change
mac-build:
    {{ nix }} build {{ flake }}#darwinConfigurations.{{ mac }}.system --no-link --print-out-paths

# ── Checks ─────────────────────────────────────────────────────────────

# Evaluate everything without building (works on the Mac)
check:
    {{ nix }} flake check --no-build --all-systems
    {{ nix }} eval --raw {{ flake }}#darwinConfigurations.{{ mac }}.system.drvPath > /dev/null

# Format all Nix files
fmt:
    {{ nix }} fmt

# Format, check, then commit everything with a message
commit message: fmt check
    git add -A
    git commit -m "{{ message }}"

# Open a REPL with the host config loaded (`config.services...`)
repl:
    {{ nix }} repl {{ flake }}#nixosConfigurations.{{ host }}

# ── Streaming / audio ──────────────────────────────────────────────────

# List audio outputs; put the real one's name in meta.audioOutput
audio-devices:
    @pactl list short sinks | awk '{print $2}' | grep -v '^channel_'

# Back up OBS settings and re-seed the Vainnor profile and scene collections
obs-reset:
    mv ~/.config/obs-studio ~/.config/obs-studio.bak-$(date +%F_%H-%M-%S)
    mv ~/.local/share/vainnor-overlays/js/config.js ~/.local/share/vainnor-overlays/js/config.js.bak-$(date +%F_%H-%M-%S) || true
    nh os switch {{ flake }}

# ── Secrets (sops) ─────────────────────────────────────────────────────

# Edit the encrypted secrets (on the Mac, which holds the editing key)
secrets-edit:
    sops secrets/secrets.yaml

# Print this PC's age recipient (run on the PC)
secrets-host:
    @nix --extra-experimental-features 'nix-command flakes' run nixpkgs#ssh-to-age -- -i /persistent/etc/ssh/ssh_host_ed25519_key.pub

# Add the PC as a recipient, re-encrypt, and turn secrets on (run on the Mac)
secrets-add-host key:
    #!/usr/bin/env bash
    set -euo pipefail
    grep -q "{{ key }}" .sops.yaml || {
      sed -i.bak -e 's|^  # - &arc age1.*|  - \&arc {{ key }} # the PC (arc)|' -e 's|^          # - \*arc|          - *arc|' .sops.yaml
      rm -f .sops.yaml.bak
    }
    sops updatekeys --yes secrets/secrets.yaml
    sed -i.bak 's|secretsReady = false;|secretsReady = true;|' modules/flake/meta.nix && rm -f modules/flake/meta.nix.bak
    echo "Done. Review with git diff, then: just commit 'chore: add arc as a secrets recipient' && just deploy"

# ── Secure Boot + TPM ──────────────────────────────────────────────────

# Show Secure Boot state and whether this boot was signed
secureboot-status:
    sudo sbctl status
    bootctl status 2>/dev/null | grep -iE 'secure boot|setup mode' || true

# After Secure Boot is ON: unlock the disk with the TPM (passphrase still works)
tpm-enroll:
    #!/usr/bin/env bash
    set -euo pipefail
    bootctl status 2>/dev/null | grep -qi 'Secure Boot: enabled' || { echo "Turn Secure Boot on first (just secureboot-status)."; exit 1; }
    sudo systemd-cryptenroll --wipe-slot=tpm2 --tpm2-device=auto --tpm2-pcrs=7 /dev/disk/by-partlabel/disk-main-luks

# ── Virtual machines ───────────────────────────────────────────────────

# Show the GPU's PCI addresses and IOMMU group (for meta.vfio)
vm-gpu:
    #!/usr/bin/env bash
    for d in /sys/bus/pci/devices/*; do
      n=$(basename "$d"); g=$(basename "$(readlink "$d/iommu_group" 2>/dev/null)" 2>/dev/null)
      lspci -nns "$n" | grep -qiE 'vga|audio|non-volatile' && echo "group ${g:-?}  $(lspci -nns "$n")"
    done | sort -V

# List USB devices (vendor:product) to pass into the Windows VM
vm-usb:
    lsusb

# One-time: start libvirt's default network and define the Windows VMs
vm-setup:
    sudo virsh net-autostart default
    sudo virsh net-start default || true
    sudo virsh define /etc/libvirt-templates/win11-lite.xml
    if [ -e /etc/libvirt-templates/win11.xml ]; then sudo virsh define /etc/libvirt-templates/win11.xml; fi

# Create a VM disk image (sparse): `just vm-disk win11-lite 80G`, `just vm-disk win11 250G`
vm-disk name="win11-lite" size="80G":
    sudo qemu-img create -f raw /var/lib/libvirt/images/{{ name }}.img {{ size }}

# Download the virtio drivers ISO; put your Windows 11 ISO at /var/lib/libvirt/images/Win11.iso
vm-virtio:
    sudo curl -L -o /var/lib/libvirt/images/virtio-win.iso https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/stable-virtio/virtio-win.iso

# Start Windows with the GPU (your Linux session closes until Windows shuts down)
windows:
    sudo virsh start win11

# Open the light Windows VM in a window (half the CPU, no GPU passthrough)
windows-lite:
    virsh -c qemu:///system domstate win11-lite | grep -q running || virsh -c qemu:///system start win11-lite
    virt-viewer -c qemu:///system --attach --wait win11-lite &

# WinApps: pick Windows apps to add to the app launcher (win11-lite must be running)
winapps-setup:
    winapps-setup --user

# WinApps: add more apps after installing them in Windows
winapps-add:
    winapps-setup --user --add-apps

# WinApps: the full Windows desktop over RDP, in a window
winapps-desktop:
    winapps windows

# Shut the light Windows VM down cleanly
windows-lite-stop:
    virsh -c qemu:///system shutdown win11-lite

# ── Backups ────────────────────────────────────────────────────────────

# List the hourly snapshots (files are under /.btrfs/@snapshots/<name>/)
snapshots:
    sudo btrbk -c /etc/btrbk/local.conf list snapshots

# Take a snapshot right now (before something risky)
snapshot:
    sudo systemctl start btrbk-local

# Run the restic backup to the homelab now, then show the latest snapshots
backup:
    sudo systemctl start restic-backups-homelab
    sudo restic-homelab snapshots --latest 5

# Restore a path from the latest restic backup into /tmp/restore (e.g. `just restore /home/vainnor/Documents`)
restore path:
    sudo restic-homelab restore latest --include "{{ path }}" --target /tmp/restore
    @echo "Restored into /tmp/restore{{ path }}"

# ── USB (USBGuard) ─────────────────────────────────────────────────────

# List USB devices; blocked ones say "block"
usb:
    usbguard list-devices

# Allow a blocked device for good (number from `just usb`)
usb-allow id:
    usbguard allow-device -p {{ id }}

# ── Impermanence helpers ───────────────────────────────────────────────

# List files on the wiped root that will be lost at reboot (find state worth persisting)
ephemeral:
    sudo find / -xdev -type f \
        -not -path '/proc/*' -not -path '/sys/*' -not -path '/run/*' \
        -not -path '/tmp/*' -not -path '/etc/static/*' \
        2>/dev/null | sort

# Change your login password (stored on /persistent; applies at next login)
password:
    #!/usr/bin/env bash
    set -euo pipefail
    read -rsp "New password: " a; echo
    read -rsp "Repeat:       " b; echo
    [ -n "$a" ] && [ "$a" = "$b" ] || { echo "Empty or didn't match. Nothing changed."; exit 1; }
    printf '%s' "$a" | mkpasswd -m yescrypt --stdin | sudo tee "/persistent/passwords/$(whoami)" > /dev/null
    echo "Password updated."
