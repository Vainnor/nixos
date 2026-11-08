# GPU passthrough to arc's Windows VM (virtualisation/vm.nix reads it as
# vx.host.vfio). Other hosts with the vm aspect get libvirt and the light VM
# without passthrough. Fill in from `vx vm gpu` / `vx vm usb` on the PC, e.g.
#   { gpu = "0000:03:00.0"; gpuAudio = "0000:03:00.1";
#     usb = [ { vendor = "0x046d"; product = "0x0000"; } ];
#     windowsNvme = null; }   # or the PCI address of the Windows NVMe to boot real Windows
# `vfio = null;` turns passthrough off.
let
  vfio = {
    gpu = "0000:03:00.0"; # RX 7900 XTX
    gpuAudio = "0000:03:00.1"; # its HDMI/DP audio
    # Keep the card away from Linux from boot: Linux runs on the CPU's
    # graphics (monitor on the motherboard's port), and Windows gets a card
    # the Linux driver never touched (once Linux has used it, Windows shows
    # code 43 and a black screen). `gpu-linux` lends it to Linux until the
    # next reboot. false = Linux owns it and the VM takes it when it starts.
    reserveGpu = false; # true: no picture from the card in Windows on this PC (tested Nov 2026)
    gpuAudioId = "1002:ab30"; # vendor:device of gpuAudio, from `vx vm gpu`
    # The card's video BIOS, dumped by `vx vm vbios` (kept on the PC, not
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
    # A disk image for now (`vx vm disk win11 250G`). Once Windows has its
    # own drive, put that NVMe's PCI address here (see `vx vm gpu`).
    windowsNvme = null;
  };
in
{
  flake.modules.nixos.arc.vx.host = { inherit vfio; };
}
