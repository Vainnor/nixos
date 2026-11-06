# RX 7900 XTX: open amdgpu + Mesa RADV (no proprietary driver), 32-bit for
# Proton, VA-API video decode, and LACT for fan curves / power limit / undervolt.
{
  flake.modules.nixos.gaming = {
    hardware.graphics = {
      enable = true;
      enable32Bit = true;
    };
    hardware.amdgpu.overdrive.enable = true;
    services.lact.enable = true;
  };
}
