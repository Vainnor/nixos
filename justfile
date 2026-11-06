# Run `just` to list recipes. Recipes run from the repo root.

set shell := ["bash", "-euo", "pipefail", "-c"]

host := "arc"
mac := "Vainnors-MacBook-Pro"
cache := "nixos"
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

# ── Preview (before installing) ────────────────────────────────────────

# Boot arc's config in a VM window (any Linux with Nix or WSL; login vainnor / preview). `gpu=soft|gl`, auto picks soft on WSL
preview gpu="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    {{ nix }} build {{ flake }}#nixosConfigurations.{{ host }}.config.system.build.vm -o /tmp/arc-preview
    mkdir -p ~/.cache/arc-preview
    gpu="{{ gpu }}"
    if [ "$gpu" = auto ]; then grep -qi microsoft /proc/version && gpu=soft || gpu=gl; fi  # WSLg has no GL for QEMU
    if [ "$gpu" = soft ]; then dev="virtio-vga"; disp="gtk"; else dev="virtio-vga-gl"; disp="gtk,gl=on"; fi
    [ -e /dev/kvm ] || echo "No /dev/kvm here: it will run, but slowly."
    NIX_DISK_IMAGE=~/.cache/arc-preview/arc.qcow2 QEMU_OPTS="-vga none -device $dev -display $disp" /tmp/arc-preview/bin/run-{{ host }}-vm

# Throw away the preview VM's disk (fresh first boot next time)
preview-reset:
    rm -f ~/.cache/arc-preview/arc.qcow2

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

# What changed between the last two system generations (on the PC)
diff:
    nvd diff $(ls -dv /nix/var/nix/profiles/system-*-link | tail -2)

# Browse what takes space in the running system
tree:
    nix-tree /run/current-system

# Push the running system to the homelab cache now (normally automatic)
cache-push:
    sudo env HOME=/var/lib/attic-push attic push homelab:{{ cache }} /run/current-system

# Open a REPL with the host config loaded (`config.services...`)
repl:
    {{ nix }} repl {{ flake }}#nixosConfigurations.{{ host }}

# ── Wallpaper and colours ──────────────────────────────────────────────

# List the wallpapers (the current one is marked)
wallpapers:
    #!/usr/bin/env bash
    cur=$(sed -n 's/^ *wallpaper = "\(.*\)";/\1/p' modules/flake/meta.nix)
    for d in assets/wallpapers/*/; do n=$(basename "$d"); [ "$n" = "$cur" ] && echo "* $n" || echo "  $n"; done

# Switch wallpaper and colours on this machine: `just wallpaper pinewood` (no name: pick from a list)
wallpaper name="":
    #!/usr/bin/env bash
    set -euo pipefail
    n="{{ name }}"
    if [ -z "$n" ]; then
      n=$(ls assets/wallpapers | {{ nix }} run nixpkgs#fzf -- --prompt "wallpaper > " --height 15) || exit 0
    fi
    [ -f "assets/wallpapers/$n/wide.png" ] || { echo "No wallpaper '$n'. See: just wallpapers"; exit 1; }
    sed -i.bak -e "s|^\( *\)wallpaper = .*;|\1wallpaper = \"$n\";|" -e "s|^\( *\)theme = .*;|\1theme = \"match\";|" modules/flake/meta.nix
    rm -f modules/flake/meta.nix.bak
    git add -A assets/wallpapers modules/flake/meta.nix
    if [ "$(uname)" = Darwin ]; then just mac; else nh os switch {{ flake }}; fi
    echo "Now: $n. Commit it with: just commit 'style: wallpaper $n'"

# Try a wallpaper's colours on the Mac without changing anything: a throwaway Ghostty window + the 4K image in Quick Look
wallpaper-try name:
    #!/usr/bin/env bash
    set -euo pipefail
    d="assets/wallpapers/{{ name }}"
    [ -f "$d/theme.nix" ] || { echo "No wallpaper '{{ name }}'. See: just wallpapers"; exit 1; }
    tmp=$(mktemp -d)
    {{ nix }} eval --json --file "$d/theme.nix" scheme | python3 -c '
    import json, sys
    c = json.load(sys.stdin)
    ansi = ["base00","base08","base0B","base0A","base0D","base0E","base0C","base05",
            "base03","base08","base0B","base0A","base0D","base0E","base0C","base07"]
    print("background = #" + c["base00"])
    print("foreground = #" + c["base05"])
    print("cursor-color = #" + c["base05"])
    print("selection-background = #" + c["base02"])
    for i, k in enumerate(ansi): print("palette = %d=#%s" % (i, c[k]))
    print("font-family = JetBrainsMono Nerd Font\nwindow-padding-x = 8\nwindow-padding-y = 8")
    ' > "$tmp/ghostty"
    cat > "$tmp/demo.sh" <<'SH'
    printf '\033[1m%s\033[0m\n\n' "$1"
    for i in 0 1 2 3 4 5 6 7; do printf '\033[4%sm    \033[0m' "$i"; done; echo
    for i in 0 1 2 3 4 5 6 7; do printf '\033[10%sm    \033[0m' "$i"; done; echo; echo
    printf '\033[34maccent\033[0m  \033[31mred\033[0m  \033[32mgreen\033[0m  \033[33myellow\033[0m  \033[90mcomment\033[0m\n\n'
    exec /bin/zsh -f
    SH
    echo "command = /bin/sh $tmp/demo.sh {{ name }}" >> "$tmp/ghostty"
    /Applications/Ghostty.app/Contents/MacOS/ghostty --config-default-files=false --config-file="$tmp/ghostty" >/dev/null 2>&1 &
    qlmanage -p "$d/4k.png" >/dev/null 2>&1 &
    echo "Close both windows when done. Nothing on the Mac was changed."

# Add a wallpaper: `just wallpaper-add forest ~/Downloads/wide.png ~/Downloads/4k.png` (makes a matching palette)
wallpaper-add name wide uhd:
    #!/usr/bin/env bash
    set -euo pipefail
    d="assets/wallpapers/{{ name }}"
    mkdir -p "$d"
    py='with import (builtins.getFlake "nixpkgs") {}; python3.withPackages (p: [ p.pillow ])'
    {{ nix }} shell --impure --expr "$py" -c python3 -I - "{{ wide }}" "$d/wide.png" <<'PY'
    import sys; from PIL import Image; Image.open(sys.argv[1]).convert("RGB").save(sys.argv[2])
    PY
    {{ nix }} shell --impure --expr "$py" -c python3 -I - "{{ uhd }}" "$d/4k.png" <<'PY'
    import sys; from PIL import Image; Image.open(sys.argv[1]).convert("RGB").save(sys.argv[2])
    PY
    {{ nix }} shell --impure --expr "$py" -c python3 -I scripts/wallpaper-palette.py "$d/wide.png" --name "{{ name }}" > "$d/theme.nix"
    git add "$d"
    echo "Added $d. Tweak $d/theme.nix if a colour feels off, then: just wallpaper {{ name }}"

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

# Set one secret at a hidden prompt (not saved in shell history): `just secret wifi/psk`
secret key:
    #!/usr/bin/env bash
    set -euo pipefail
    read -rsp "{{ key }}: " v; echo
    [ -n "$v" ] || { echo "Empty, nothing changed."; exit 1; }
    keypath=$(python3 -c 'import json,sys; print("".join("[%s]" % json.dumps(p) for p in sys.argv[1].split("/")))' "{{ key }}")
    printf '%s' "$v" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))' | sops set --value-stdin secrets/secrets.yaml "$keypath"
    echo "Set {{ key }}."

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

# ── Projects ───────────────────────────────────────────────────────────

# New project with its own toolchain: `just new python ~/code/thing` (python node java go php rust c)
new lang dir:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{ invocation_directory() }}"
    mkdir -p "{{ dir }}" && cd "{{ dir }}"
    nix --extra-experimental-features 'nix-command flakes' flake init -t "{{ flake }}#{{ lang }}"
    [ -d .git ] || git init -q
    git add -A   # flakes only see files git knows about
    if command -v direnv >/dev/null; then direnv allow; else echo "direnv not installed here; run: nix develop"; fi
    echo "Ready in $PWD. The toolchain loads when you cd in."

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
