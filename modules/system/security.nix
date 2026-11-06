# Baseline hardening that doesn't break gaming or browsers.
{
  flake.modules.nixos.base = {
    networking.firewall.enable = true;

    security = {
      sudo = {
        execWheelOnly = true;
        wheelNeedsPassword = true;
        # `just sudo-unlock` drops a timed NOPASSWD rule in here (RAM only, so
        # a reboot always clears it). Last in the file, so it wins over %wheel.
        extraConfig = "@includedir /run/sudoers.d";
      };
      polkit.enable = true;
      apparmor.enable = true;
    };

    systemd.tmpfiles.rules = [ "d /run/sudoers.d 0750 root root -" ];

    boot.kernel.sysctl = {
      "kernel.kptr_restrict" = 2;
      "kernel.dmesg_restrict" = 1;
      "kernel.kexec_load_disabled" = 1;
      "kernel.unprivileged_bpf_disabled" = 1;
      "net.core.bpf_jit_harden" = 2;
      "net.ipv4.tcp_syncookies" = 1;
      "net.ipv4.conf.all.accept_redirects" = 0;
      "net.ipv4.conf.default.accept_redirects" = 0;
      "net.ipv4.conf.all.send_redirects" = 0;
      "net.ipv6.conf.all.accept_redirects" = 0;
      "net.ipv6.conf.default.accept_redirects" = 0;
    };

    services = {
      fwupd.enable = true;
      dbus.implementation = "broker";
    };
  };
}
