# WinApps: Windows apps from the win11-lite VM as normal windows on the Linux
# desktop (RDP RemoteApp). They show up in fuzzel like any other app.
#
# Needs Windows 11 **Pro** (Home has no RDP server) with a password login
# (not a PIN) for meta.windowsUser. One-time guest setup: open the WINAPPS CD
# in the VM, right-click install.bat > Run as administrator, reboot the VM.
# Then on Linux: `just winapps-setup` picks which apps to add.
#
# The RDP password comes from sops (winapps/rdp_password) when secrets are
# on; otherwise fuzzel asks for it. It never lands in a plain config file.
{ inputs, config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.vm =
    { pkgs, lib, ... }:
    let
      winapps = inputs.winapps.packages.${pkgs.stdenv.hostPlatform.system};
      # The guest-side setup files (RDP RemoteApp registry keys, time sync,
      # network profile cleanup) on a CD for the VM. Container.reg is for
      # the docker flavour only, so it's left out.
      oemIso = pkgs.runCommand "winapps-oem.iso" { nativeBuildInputs = [ pkgs.xorriso ]; } ''
        mkdir oem
        cp ${inputs.winapps}/oem/{install.bat,RDPApps.reg,TimeSync.ps1,NetProfileCleanup.ps1} oem/
        xorriso -as mkisofs -quiet -J -R -V WINAPPS -o $out oem
      '';
    in
    {
      environment.systemPackages = [ winapps.winapps ];
      environment.etc."libvirt-templates/winapps-oem.iso".source = oemIso;

      # WinApps talks to libvirt's system instance (where win11-lite lives).
      environment.sessionVariables.LIBVIRT_DEFAULT_URI = "qemu:///system";

      nix.settings = {
        extra-substituters = [ "https://winapps.cachix.org/" ];
        extra-trusted-public-keys = [ "winapps.cachix.org-1:HI82jWrXZsQRar/PChgIx1unmuEsiQMQq+zt05CD36g=" ];
      };

      sops.secrets = lib.mkIf meta.secretsReady {
        "winapps/rdp_password".owner = meta.username;
      };
    };

  flake.modules.homeManager.vm =
    { pkgs, lib, ... }:
    let
      askpass = pkgs.writeShellScript "winapps-askpass" ''
        if [ -r /run/secrets/winapps/rdp_password ]; then
          cat /run/secrets/winapps/rdp_password
        else
          ${lib.getExe pkgs.fuzzel} --dmenu --password --prompt 'Windows password: ' --lines 0 --width 30
        fi
      '';
    in
    {
      xdg.configFile."winapps/winapps.conf".text = ''
        RDP_USER="${meta.windowsUser}"
        RDP_ASKPASS="${askpass}"
        RDP_DOMAIN=""
        WAFLAVOR="libvirt"
        VM_NAME="win11-lite"
        RDP_SCALE="100"
        RDP_DESKTOP_SCALE="inherit"
        REMOVABLE_MEDIA="/run/media"
        RDP_FLAGS="/cert:tofu /sound /microphone +home-drive"
        DEBUG="true"
        AUTOPAUSE="on"
        AUTOPAUSE_TIME="300"
        FREERDP_COMMAND=""
        PORT_TIMEOUT="5"
        RDP_TIMEOUT="30"
        APP_SCAN_TIMEOUT="60"
        BOOT_TIMEOUT="120"
        HIDEF="on"
      '';
    };
}
