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

    # Wallpaper and colours. `just wallpaper <name>` changes this line and rebuilds.
    #   wallpaper: a folder in assets/wallpapers (`just wallpapers` lists them),
    #              "dots" (flat dot grid), or a path like ../../assets/some.png
    #   theme:     "match" = the wallpaper's own palette; or "vainnor-slate" /
    #              "caelus" (assets/themes); or any base16 name
    wallpaper = "eye";
    theme = "match";
    # Colours laid over the active theme last, for quick experiments, e.g.
    # { base0D = "E0A040"; border = "3A3F3C"; } (see assets/themes/default.nix).
    # Lasting fixes go in the theme file's own `overrides`. `just theme-check` checks them.
    themeOverrides = { };

    # PipeWire name of your real output (BEACN Studio headphones or the HyperX
    # dongle), e.g. "alsa_output.usb-....analog-stereo". Run `just audio-devices`
    # on the PC to list them. null = virtual channels exist but System isn't default.
    audioOutput = null;

    # sops-nix secrets (secrets/secrets.yaml). Flip to true once the PC's host
    # key is a recipient in .sops.yaml and the file was re-encrypted
    # (`just secrets-host` on the PC, then `just secrets-add-host <key>` on the Mac).
    secretsReady = false;

    # GPU passthrough to the Windows VM. null = libvirt/virt-manager only.
    # Fill in from `just vm-gpu` / `just vm-usb` on the PC, e.g.
    #   { gpu = "0000:03:00.0"; gpuAudio = "0000:03:00.1";
    #     usb = [ { vendor = "0x046d"; product = "0x0000"; } ];
    #     windowsNvme = null; }   # or the PCI address of the Windows NVMe to boot real Windows
    vfio = {
      gpu = "0000:03:00.0"; # RX 7900 XTX
      gpuAudio = "0000:03:00.1"; # its HDMI/DP audio
      # Keep the card away from Linux from boot: Linux runs on the CPU's
      # graphics (monitor on the motherboard's port), and Windows gets a card
      # the Linux driver never touched (once Linux has used it, Windows shows
      # code 43 and a black screen). `gpu-linux` lends it to Linux until the
      # next reboot. false = Linux owns it and the VM takes it when it starts.
      reserveGpu = false; # true: no picture from the card in Windows on this PC (tested Nov 2026)
      gpuAudioId = "1002:ab30"; # vendor:device of gpuAudio, from `just vm-gpu`
      # The card's video BIOS, dumped by `just vm-vbios` (kept on the PC, not
      # in this public repo). Without it the VM shows a black screen.
      gpuRom = "/var/lib/libvirt/vbios/rx7900xtx.rom";
      # A virtual screen alongside the card, for installing Windows: the card
      # shows nothing until Windows has the AMD driver. Turn off afterwards
      # (games would see it as a second monitor).
      installDisplay = false; # true adds a virtual screen (VNC on :5909) for installing Windows
      # Handed to Windows while the VM runs, back to Linux after.
      usb = [
        {
          vendor = "0x3434";
          product = "0x0280";
        } # Keychron K8 Pro
        {
          vendor = "0x258a";
          product = "0x0033";
        } # wired gaming mouse
        {
          vendor = "0x33ae";
          product = "0x0003";
        } # BEACN Studio
        {
          vendor = "0x03f0";
          product = "0x098d";
        } # HyperX Cloud Alpha Wireless
        {
          vendor = "0x046d";
          product = "0x094d";
        } # Logitech Brio 101
      ];
      # A disk image for now (`just vm-disk win11 250G`). Once Windows has its
      # own drive, put that NVMe's PCI address here (see `just vm-gpu`).
      windowsNvme = null;
    };

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
    # The Mac's LibreWolf profile folder (about:support > Profile Folder); it gets
    # the same userChrome/userContent as the PC.
    macLibrewolfProfile = "t3syv4fs.default-release";

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
