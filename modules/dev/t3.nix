# T3 Code's server, so the Mac (and phone) drive agents on this machine.
# Reachable over NetBird only: the firewall opens 3773 on wt0 alone, and the
# NetBird policy decides which peers get there.
#
# Nothing to set up by hand: each switch runs `t3 service install` when the
# background server isn't installed at this package's version, which writes
# T3's own user unit (~/.config/systemd/user/t3code.service, ExecStart under
# ~/.t3/runtime), then makes sure it's enabled and running. Nix adds a drop-in
# with what the unit can't know here: the PATH to the user's tools (git, nix,
# gh, direnv, claude) and the 1Password SSH agent. Its binaries run through
# nix-ld (dev/tooling.nix).
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
          # An AppImage's Electron doesn't follow NIXOS_OZONE_WL: without this
          # it looks for an X server and exits.
          source ${pkgs.makeWrapper}/nix-support/setup-hook
          wrapProgram $out/bin/t3-code \
            --add-flags "--ozone-platform-hint=auto --enable-features=WaylandWindowDecorations --enable-wayland-ime" \
            --run 'export T3CODE_INSTALL_BIN_DIR="$HOME/.t3/app-bin"'
          # ^ The app writes its own `t3` command there, off PATH: that one runs
          #   outside the sandbox and fails on NixOS; t3-cli below is the one.

          desktop=$(find ${contents} -maxdepth 1 -name '*.desktop' | head -n1)
          install -Dm644 "$desktop" $out/share/applications/t3-code.desktop
          sed -i 's|^Exec=[^ ]*|Exec=t3-code|' $out/share/applications/t3-code.desktop
          if [ -d ${contents}/usr/share/icons ]; then
            cp -r ${contents}/usr/share/icons $out/share/
          fi
        '';
      };
      # The `t3` CLI (`t3 service install`, `t3 service status`, ...). The app's
      # own ~/.t3/bin/t3 runs the bundled binary outside the AppImage's
      # sandbox, where it finds none of its libraries on NixOS; this runs it
      # inside (the wrapper without the app's Wayland flags, which Node
      # would reject).
      t3-cli = pkgs.writeShellScriptBin "t3" ''
        export T3CODE_HOME="''${T3CODE_HOME:-$HOME/.t3}"
        export T3CODE_CLI_PATH="/etc/profiles/per-user/$USER/bin/t3"
        export ELECTRON_RUN_AS_NODE=1
        exec ${t3-desktop}/bin/.t3-code-wrapped ${contents}/resources/app.asar/apps/server/dist/bin.mjs "$@"
      '';
    in
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      home.packages = [
        t3-desktop
        (lib.hiPrio t3-cli)
      ];

      # The background server listens beyond this machine (T3CODE_LAN), so the
      # Mac links to it straight over NetBird (http://<arc>:3773), with no T3
      # Connect tunnel and no Tailscale. Only NetBird reaches it: the firewall
      # opens 3773 on wt0 alone. T3 keeps its other service settings here.
      home.activation.t3ServiceEnv = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        f=${config.home.homeDirectory}/.t3/service.env
        run mkdir -p ${config.home.homeDirectory}/.t3
        touch "$f"
        if grep -q '^T3CODE_LAN=' "$f"; then
          run ${pkgs.gnused}/bin/sed -i 's/^T3CODE_LAN=.*/T3CODE_LAN=true/' "$f"
        else
          run sh -c 'printf "%s\n" T3CODE_LAN=true >> "$1"' sh "$f"
        fi
      '';

      home.activation.t3Service = lib.hm.dag.entryAfter [ "reloadSystemd" "t3ServiceEnv" ] ''
        # The switch runs without your session's bus; T3's installer needs it.
        export XDG_RUNTIME_DIR="''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
        export DBUS_SESSION_BUS_ADDRESS="''${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"
        unit=${config.xdg.configHome}/systemd/user/t3code.service
        # The app's own `t3` from before T3CODE_INSTALL_BIN_DIR: it shadows ours.
        shim=${config.home.homeDirectory}/.local/bin/t3
        if [ -f "$shim" ] && grep -q "Written by T3 Code" "$shim"; then
          run rm -f "$shim"
        fi
        if [ ! -f "$unit" ] || [ ! -e ${config.home.homeDirectory}/.t3/runtime/versions/${version}/.install-complete ]; then
          run ${t3-cli}/bin/t3 service install || echo "t3 service install failed; see ~/.t3/userdata/logs" >&2
        fi
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
