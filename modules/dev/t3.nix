# T3 Code's server, so the Mac (and phone) drive agents on this machine.
# Reachable over NetBird only: the firewall opens 3773 on wt0 alone, and the
# NetBird policy decides which peers get there.
#
# T3 is installed once by hand and updates itself (`~/.t3/bin/t3`); the unit
# waits for it (ConditionPathExists), so a fresh install isn't a failing unit.
# Its dynamic binaries run through nix-ld (dev/tooling.nix).
{ config, ... }:
let
  inherit (config) meta;
  port = 3773;
in
{
  flake.modules.nixos.dev = {
    # Run without a login session: T3 is reached from other machines.
    users.users.${meta.username}.linger = true;
    networking.firewall.interfaces.wt0.allowedTCPPorts = [ port ];
  };

  flake.modules.homeManager.dev =
    { lib, pkgs, ... }:
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      systemd.user.services.t3code = {
        Unit = {
          Description = "T3 Code server";
          After = [ "network-online.target" ];
          ConditionPathExists = "%h/.t3/bin/t3";
        };
        Service = {
          ExecStart = "%h/.t3/bin/t3 serve --host 0.0.0.0 --port ${toString port} --no-browser";
          Restart = "always";
          RestartSec = 5;
          # Agents run git, nix, gh, direnv and the rest from the user's profile.
          Environment = [
            "PATH=%h/.t3/bin:/run/wrappers/bin:%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/nix/var/nix/profiles/default/bin:/run/current-system/sw/bin"
            "SSH_AUTH_SOCK=%h/.1password/agent.sock"
          ];
        };
        Install.WantedBy = [ "default.target" ];
      };
    };
}
