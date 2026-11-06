# Plymouth: a themed boot screen (Stylix colours and logo) instead of kernel
# text, including the disk-password prompt. Press Esc during boot to see the
# messages.
{
  flake.modules.nixos.desktop = {
    boot = {
      plymouth.enable = true;
      consoleLogLevel = 3;
      kernelParams = [
        "quiet"
        "udev.log_level=3"
        "rd.udev.log_level=3"
        "systemd.show_status=auto"
      ];
    };
  };
}
