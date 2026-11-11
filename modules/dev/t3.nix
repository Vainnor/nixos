# T3 Code's server, so the Mac (and phone) drive agents on this machine.
# Reachable over NetBird only: the firewall opens 3773 on wt0 alone, and the
# NetBird policy decides which peers get there.
#
# T3 installs and updates itself: it writes its own user unit,
# ~/.config/systemd/user/t3code.service (ExecStart under ~/.t3/runtime), and
# rewrites it on each update, so the unit stays T3's. Nix adds a drop-in with
# what the unit can't know here: the PATH to the user's tools (git, nix, gh,
# direnv, claude) and the 1Password SSH agent. Its binaries run through nix-ld
# (dev/tooling.nix). Once T3 has installed its unit, each switch makes sure it's
# enabled and running.
#
# The desktop app is T3's AppImage from its releases, pinned here (its own
# updater can't replace a store path): bump version and hash to update.
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
    let
      version = "0.1.8";
      src = pkgs.fetchurl {
        url = "https://github.com/pingdotgg/t3code/releases/download/v${version}/T3-Code-${version}-x86_64.AppImage";
        hash = "sha256-4Rdpi+TMWeXsFBbFHf1EKlPvoxQZd7wgy9DYH9P9WCQ=";
      };
      contents = pkgs.appimageTools.extract {
        pname = "t3-code";
        inherit version src;
      };
      t3-desktop = pkgs.appimageTools.wrapType2 {
        pname = "t3-code";
        inherit version src;
        # The launcher entry and icon from inside the AppImage.
        extraInstallCommands = ''
          desktop=$(find ${contents} -maxdepth 1 -name '*.desktop' | head -n1)
          install -Dm644 "$desktop" $out/share/applications/t3-code.desktop
          sed -i 's|^Exec=[^ ]*|Exec=t3-code|' $out/share/applications/t3-code.desktop
          if [ -d ${contents}/usr/share/icons ]; then
            cp -r ${contents}/usr/share/icons $out/share/
          fi
        '';
      };
    in
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      home.packages = [ t3-desktop ];

      home.activation.t3Service = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
        unit=${config.xdg.configHome}/systemd/user/t3code.service
        if [ -f "$unit" ]; then
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
