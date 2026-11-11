# The system ssh_config pulls in no files from /nix/store. OpenSSH checks the
# owner of every file a config Includes, and inside the T3 app's sandbox (its
# own user namespace, which maps only your user) root's files show as
# nobody's, so one such Include makes every ssh and git-over-ssh there fail
# ("Bad owner or permissions"). NixOS adds two: systemd's proxy for
# `ssh vsock/...` / `ssh machine/...` and libvirt's for `ssh qemu:...`. Neither
# is used here. (~/.ssh/config is a real file for the same reason:
# apps/1password.nix.)
{
  flake.modules.nixos.dev = {
    programs.ssh.systemd-ssh-proxy.enable = false;
    virtualisation.libvirtd.sshProxy = false;
  };
}
