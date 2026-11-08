# Graphics for games: 32-bit drivers for Proton and older games. The driver
# itself, video decode and GPU tuning (LACT on AMD) follow the host's `gpu`
# (system/gpu.nix).
{
  flake.modules.nixos.gaming = {
    hardware.graphics = {
      enable = true;
      enable32Bit = true;
    };
  };
}
