# Run `just` to list recipes. Recipes run from the repo root.

set shell := ["bash", "-euo", "pipefail", "-c"]

host := "arc"
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

# ── Checks ─────────────────────────────────────────────────────────────

# Evaluate everything without building (works on the Mac)
check:
    {{ nix }} flake check --no-build --all-systems

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
