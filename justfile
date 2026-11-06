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

# First time only: hand apps you installed by hand over to Homebrew (so `just mac` can manage them)
mac-adopt:
    #!/usr/bin/env bash
    set -uo pipefail
    have=$(brew list --cask -1)
    failed=()
    for c in $({{ nix }} eval --raw {{ flake }}#darwinConfigurations.{{ mac }}.config.homebrew.casks --apply 'cs: builtins.concatStringsSep " " (map (c: c.name) cs)'); do
      grep -qx "${c##*/}" <<<"$have" && continue
      echo "== $c"
      HOMEBREW_NO_AUTO_UPDATE=1 brew install --cask --adopt "$c" || failed+=("$c")
    done
    if [ ${#failed[@]} -gt 0 ]; then
      echo "Could not adopt: ${failed[*]}"
      echo "Usually a version mismatch: update that app (or delete it) and run this again."
    else
      echo "All casks are Homebrew's now. Next: just mac"
    fi

# Remove nix-darwin and put the Mac back as it was (settings, dotfiles, /etc). `just mac-undo dry` only shows what it would do
mac-undo mode="":
    #!/usr/bin/env bash
    set -uo pipefail
    sys=$(readlink -f /run/current-system 2>/dev/null || true)
    [ -n "$sys" ] && [ -e "$sys/activate" ] || { echo "nix-darwin isn't active on this Mac; nothing to undo."; exit 0; }
    dry=false; [ "{{ mode }}" = dry ] && dry=true
    run() { if $dry; then echo "  would: $*"; else "$@"; fi; }
    hmf=$(nix-store -qR "$sys" | grep -m1 'home-manager-files$' || true)

    if ! $dry; then
      echo "This removes nix-darwin from this Mac:"
      echo "  - macOS settings it set go back to Apple's defaults, firewall off"
      echo "  - dotfiles it manages are removed, and your *.hm-backup originals put back"
      echo "  - nix-darwin itself is uninstalled, /etc files it replaced are restored"
      echo "Homebrew, your apps, Nix itself and the wallpaper stay as they are."
      read -rp "Type UNDO to continue: " a; [ "$a" = UNDO ] || { echo "Nothing changed."; exit 1; }
    fi

    echo "== macOS settings"
    grep -oE "defaults write +(-g|[^ ]+) +[^ ']+" "$sys/activate" | sort -u | while read -r _ _ dom key; do
      case "$dom" in
        /Library/*) run sudo defaults delete "$dom" "$key" 2>/dev/null || true ;;
        *) run defaults delete "$dom" "$key" 2>/dev/null || true ;;
      esac
    done
    run sudo /usr/libexec/ApplicationFirewall/socketfilterfw --setstealthmode off
    run sudo /usr/libexec/ApplicationFirewall/socketfilterfw --setglobalstate off
    run killall Dock Finder SystemUIServer 2>/dev/null || true

    echo "== dotfiles"
    if [ -n "$hmf" ]; then
      (cd "$hmf" && find . \( -type l -o -type f \) | sed 's|^\./||') | while IFS= read -r f; do
        t="$HOME/$f"
        if [ -L "$t" ] && [[ "$(readlink "$t")" == /nix/store/* ]]; then run rm "$t"; fi
        if [ -e "$t.hm-backup" ] && { $dry || [ ! -e "$t" ]; }; then run mv "$t.hm-backup" "$t"; fi
      done
    fi

    echo "== nix-darwin"
    run sudo "$sys/sw/bin/darwin-uninstaller"
    for b in /etc/*.before-nix-darwin /etc/nix/*.before-nix-darwin; do
      [ -e "$b" ] || continue
      o="${b%.before-nix-darwin}"
      run sudo rm -f "$o"
      run sudo mv "$b" "$o"
    done
    run sudo launchctl kickstart -k system/org.nixos.nix-daemon

    if $dry; then echo "Dry run: nothing changed."; else echo "Done. Log out and back in so every app sees the old settings."; fi

# Update Homebrew and upgrade every brew and cask (normal `just mac` doesn't)
mac-upgrade:
    brew update
    brew upgrade
    brew upgrade --cask --greedy

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

# ── Fans (CoolerControl) ───────────────────────────────────────────────

# Temperatures and fan speeds right now
fans:
    sensors

# Copy the fan curves from CoolerControl into the repo (a fresh install starts with them)
fans-save:
    #!/usr/bin/env bash
    set -euo pipefail
    sudo test -f /etc/coolercontrol/config.toml || { echo "No CoolerControl config yet. Set up a curve in the app first."; exit 1; }
    mkdir -p assets/coolercontrol
    sudo cat /etc/coolercontrol/config.toml > assets/coolercontrol/config.toml
    git add assets/coolercontrol/config.toml
    echo "Saved. Commit it with: just commit \"chore(fans): save fan curves\""

# ── Temporary sudo ─────────────────────────────────────────────────────

# sudo without a password for a while (default 60 min), e.g. so Claude can work over SSH. A reboot also ends it.
sudo-unlock minutes="60":
    #!/usr/bin/env bash
    set -euo pipefail
    f=/run/sudoers.d/unlock
    echo "$USER ALL=(ALL:ALL) NOPASSWD: ALL" | sudo tee "$f.new" >/dev/null
    sudo chmod 0440 "$f.new"
    sudo visudo -cqf "$f.new"
    sudo mv "$f.new" "$f"
    sudo systemctl stop sudo-relock.timer 2>/dev/null || true
    sudo systemd-run --quiet --unit=sudo-relock --on-active={{ minutes }}m rm -f "$f"
    echo "Passwordless sudo until $(date -d '+{{ minutes }} min' +%H:%M). End it now with: just sudo-lock"

# End `just sudo-unlock` now
sudo-lock:
    sudo rm -f /run/sudoers.d/unlock
    sudo systemctl stop sudo-relock.timer 2>/dev/null || true
    @echo "sudo asks for a password again."

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

# (Re)define the Windows VMs from the config and start libvirt's network (safe to rerun)
vm-setup:
    #!/usr/bin/env bash
    set -euo pipefail
    sudo virsh net-autostart default
    sudo virsh net-start default 2>/dev/null || true
    for name in win11-lite win11; do
      xml=/etc/libvirt-templates/$name.xml
      [ -e "$xml" ] || continue
      if uuid=$(sudo virsh domuuid "$name" 2>/dev/null); then
        sed "s|<name>$name</name>|<name>$name</name><uuid>$uuid</uuid>|" "$xml" > "/tmp/$name.xml"
        sudo virsh define "/tmp/$name.xml"
        rm -f "/tmp/$name.xml"
      else
        sudo virsh define "$xml"
      fi
    done

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
