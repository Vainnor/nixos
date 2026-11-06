# Controllers: Xbox over Bluetooth (xpadneo: rumble, battery, all buttons),
# the Xbox wireless USB dongle (xone), and udev rules for PlayStation, Switch,
# 8BitDo and friends so Steam Input sees them. Steam's own module already
# covers Steam controllers and Steam Deck-style devices.
{
  flake.modules.nixos.gaming =
    { pkgs, ... }:
    {
      unfree.allowed = [ "xone-dongle-firmware" ];
      hardware.xpadneo.enable = true;
      hardware.xone.enable = true;
      hardware.uinput.enable = true;
      services.udev.packages = [ pkgs.game-devices-udev-rules ];
    };
}
