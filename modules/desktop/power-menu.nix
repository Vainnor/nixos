# Power menu: in the shell (desktop/shell.nix), Super+Shift+Escape or [ off ] in the bar.
# "Reboot to Windows" sets a one-time EFI BootNext to Windows Boot Manager,
# so the next boot goes to Windows and the one after is back to NixOS.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.desktop =
    { pkgs, lib, ... }:
    let
      rebootToWindows = pkgs.writeShellScriptBin "reboot-to-windows" ''
        set -eu
        entry="$(${lib.getExe pkgs.efibootmgr} | ${lib.getExe pkgs.gnused} -n 's/^Boot\([0-9A-Fa-f]\{4\}\)\*\? Windows Boot Manager.*/\1/p' | head -n1)"
        [ -n "$entry" ] || { echo "No 'Windows Boot Manager' EFI entry found." >&2; exit 1; }
        ${lib.getExe pkgs.efibootmgr} --bootnext "$entry" >/dev/null
        systemctl reboot
      '';
    in
    {
      environment.systemPackages = [ rebootToWindows ];
      # Only this exact command may run as root without a password.
      security.sudo.extraRules = [
        {
          users = [ meta.username ];
          commands = [
            {
              command = "/run/current-system/sw/bin/reboot-to-windows";
              options = [ "NOPASSWD" ];
            }
          ];
        }
      ];
    };
}
