# USBGuard: USB devices plugged in while the PC runs are blocked until you
# allow them (a stranger's "charging cable" or a BadUSB stick can't type
# commands). Anything already plugged in at boot keeps working, and the
# devices present on the very first boot are remembered as allowed.
#
# A blocked device shows a notification. Then:
#   just usb            list devices and their numbers
#   just usb-allow 12   allow device 12 for good
{
  flake.modules.nixos.base =
    { pkgs, lib, ... }:
    let
      rules = "/var/lib/usbguard/rules.conf";
    in
    {
      services.usbguard = {
        enable = true;
        ruleFile = rules;
        presentDevicePolicy = "allow"; # never lock out the keyboard at boot
        insertedDevicePolicy = "apply-policy";
        implicitPolicyTarget = "block";
        IPCAllowedGroups = [ "wheel" ];
        dbus.enable = true; # for the desktop notifier
      };
      # First boot: allow whatever is connected now.
      systemd.services.usbguard.preStart = lib.mkAfter ''
        [ -s ${rules} ] || ${pkgs.usbguard}/bin/usbguard generate-policy > ${rules}
        chmod 0600 ${rules}
      '';
      preservation.preserveAt."/persistent".directories = [
        {
          directory = "/var/lib/usbguard";
          mode = "0700";
        }
      ];
    };

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      systemd.user.services.usbguard-notifier = {
        Unit = {
          Description = "Notify when USBGuard blocks a device";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" ];
        };
        Service = {
          ExecStart = "${pkgs.usbguard-notifier}/bin/usbguard-notifier";
          Restart = "on-failure";
        };
        Install.WantedBy = [ "graphical-session.target" ];
      };
    };
}
