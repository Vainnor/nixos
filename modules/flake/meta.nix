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

    # Public SSH keys allowed to log in (over NetBird only). Paste yours here.
    sshKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores vainnor@Carsons-MBP.home"
    ];
  };
}
