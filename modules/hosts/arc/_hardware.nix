# Hardware for arc: MSI MAG X670E Tomahawk WiFi, Ryzen 7 9800X3D, RX 7900 XTX,
# 32GB RAM, MediaTek RZ616 Wi-Fi/BT, Realtek 2.5GbE.
{
  nixpkgs.hostPlatform = "x86_64-linux";

  boot.initrd.availableKernelModules = [
    "nvme"
    "xhci_pci"
    "ahci"
    "usbhid"
    "usb_storage"
    "sd_mod"
  ];
  # amdgpu loads after the initrd, not in it: the gaming VM's card has to be
  # set aside first (virtualisation/vm.nix, meta.vfio.reserveGpu).
  boot.kernelModules = [ "kvm-amd" ];

  # Zen 5, single CCD: no V-Cache core parking needed.
  boot.kernelParams = [ "amd_pstate=active" ];
  hardware.cpu.amd.updateMicrocode = true;
  hardware.enableRedistributableFirmware = true;
  services.power-profiles-daemon.enable = true;
}
