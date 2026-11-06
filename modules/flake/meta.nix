# Values shared by every feature. Read them from any flake-parts module via
# `config.meta.<name>` instead of threading specialArgs around.
{ lib, ... }:
{
  options.meta = lib.mkOption {
    type = lib.types.attrsOf lib.types.anything;
    default = { };
    description = "Shared values for all features (user, theme, paths).";
  };

  config.meta = {
    username = "vainnor";
    fullName = "Vainnor";
    email = "me@carsonberget.com";

    timeZone = "America/Chicago";

    # GitHub repo (cloned into ~/nixos by the installer). Public is fine: no secrets live here.
    repoUrl = "https://github.com/vainnor/nixos";

    # The installer only ever targets an NVMe disk of this model (the 1TB Crucial P310).
    installDiskModel = "CT1000P310SSD8";

    # Where the repo is cloned on the PC (nh uses this).
    flakePath = "/home/vainnor/nixos";

    # One-line theme switch: any name from `ls $(nix build nixpkgs#base16-schemes --print-out-paths)/share/themes`
    theme = "catppuccin-mocha";

    # PipeWire name of your real output (BEACN Studio headphones or the HyperX
    # dongle), e.g. "alsa_output.usb-....analog-stereo". Run `just audio-devices`
    # on the PC to list them. null = virtual channels exist but System isn't default.
    audioOutput = null;

    # Wallpaper: a nixos-artwork name (e.g. "catppuccin-mocha", "nineish-dark-gray")
    # or a path like ../../assets/wallpaper.png. Colours still come from `theme`.
    wallpaper = "catppuccin-mocha";

    # sops-nix secrets (secrets/secrets.yaml). Flip to true once the PC's host
    # key is a recipient in .sops.yaml and the file was re-encrypted
    # (`just secrets-host` on the PC, then `just secrets-add-host <key>` on the Mac).
    secretsReady = false;

    # GPU passthrough to the Windows VM. null = libvirt/virt-manager only.
    # Fill in from `just vm-gpu` / `just vm-usb` on the PC, e.g.
    #   { gpu = "0000:03:00.0"; gpuAudio = "0000:03:00.1";
    #     usb = [ { vendor = "0x046d"; product = "0x0000"; } ];
    #     windowsNvme = null; }   # or the PCI address of the Windows NVMe to boot real Windows
    vfio = null;

    # Your Windows account in the VMs (WinApps logs in as this; use a password, not a PIN).
    windowsUser = "vainnor";

    # Public SSH keys allowed to log in (over NetBird only). Paste yours here.
    sshKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores vainnor@Carsons-MBP.home"
    ];

    # Git commits are signed with this key through the 1Password SSH agent
    # (the private half lives in 1Password, never on disk).
    signingKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores";

    # The Mac's hostname (`scutil --get LocalHostName`); darwin-rebuild picks
    # darwinConfigurations.<this> automatically.
    macHost = "Vainnors-MacBook-Pro";

    # Night light (wlsunset) follows sunset here. Chicago.
    location = {
      latitude = 41.88;
      longitude = -87.63;
    };

    # Instant replay records this monitor. "screen" = the first one found;
    # `gpu-screen-recorder --list-monitors` on the PC shows names like "DP-1".
    replayMonitor = "screen";

    # restic off-site backups. Turn on once restic/* is filled in secrets.yaml
    # (`just secrets-edit`) and secretsReady is true.
    resticReady = false;

    # Homelab binary cache (Attic). null = off. Fill in from the homelab, e.g.
    #   { endpoint = "https://cache.lab.vainnor.com"; cache = "nixos";
    #     publicKey = "nixos:AAAA..."; }
    # Both machines then download from it; the PC also pushes what it builds
    # (token in sops as attic/token).
    binaryCache = null;
  };
}
