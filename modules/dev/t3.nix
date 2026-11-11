# T3 Code's server, so the Mac (and phone) drive agents on this machine.
# Reachable over NetBird only: the firewall opens 3773 on wt0 alone, and the
# NetBird policy decides which peers get there.
#
# T3 installs and updates itself: it writes its own user unit,
# ~/.config/systemd/user/t3code.service (ExecStart under ~/.t3/runtime), and
# rewrites it on each update, so the unit stays T3's. Nix adds a drop-in with
# what the unit can't know here: the PATH to the user's tools (git, nix, gh,
# direnv, claude) and the 1Password SSH agent. Its binaries run through nix-ld
# (dev/tooling.nix). Once T3's unit is there (installed, or imported from WSL
# by hosts/arc/wsl-import.nix), each switch makes sure it's enabled and running.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.dev = {
    # The user manager (and T3) runs from boot, without a login.
    users.users.${meta.username}.linger = true;
    networking.firewall.interfaces.wt0.allowedTCPPorts = [ 3773 ];
  };

  flake.modules.homeManager.dev =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      home.activation.t3Service = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
        unit=${config.xdg.configHome}/systemd/user/t3code.service
        if [ -f "$unit" ] && command -v systemctl >/dev/null; then
          run ${pkgs.systemd}/bin/systemctl --user daemon-reload || true
          run ${pkgs.systemd}/bin/systemctl --user enable --now t3code.service || true
        fi
      '';

      xdg.configFile."systemd/user/t3code.service.d/nix.conf".text = ''
        [Service]
        Environment="PATH=%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/run/wrappers/bin:/nix/var/nix/profiles/default/bin:/run/current-system/sw/bin"
        Environment="SSH_AUTH_SOCK=%h/.1password/agent.sock"
      '';
    };
}
