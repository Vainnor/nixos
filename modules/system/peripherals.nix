# Bluetooth (MediaTek RZ616). Webcam (Brio 101), keyboard and mouse need nothing.
{
  flake.modules.nixos.desktop = {
    hardware.bluetooth = {
      enable = true;
      powerOnBoot = true;
    };
  };

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      home.packages = [ pkgs.bluetui ];
    };
}
