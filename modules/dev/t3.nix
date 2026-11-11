# T3 Code's server, so the Mac (and phone) drive agents on this machine.
# Reachable over NetBird only: the firewall opens 3773 on wt0 alone, and the
# NetBird policy decides which peers get there.
#
# T3 lets one server own a T3 home, so the background server has its own,
# ~/.t3-server, and the desktop app keeps ~/.t3: both run at once, each with
# its own threads (pair the app with the server to see the server's too). The
# `t3` command works on the server's home.
#
# Nothing to set up by hand: t3-install (below) runs `t3 service install` when
# the background server isn't installed at this package's version, which writes
# T3's own user unit (~/.config/systemd/user/t3code.service, ExecStart under
# ~/.t3-server/runtime), then makes sure it's enabled and running. Nix adds a drop-in
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
      # The background server's T3 home (the desktop app's is ~/.t3).
      serverHome = "${config.home.homeDirectory}/.t3-server";
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
        export T3CODE_HOME="''${T3CODE_HOME:-${serverHome}}"
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

      # Installs T3's background server when it isn't installed at this
      # package's version: a oneshot in your user manager (T3's installer
      # talks to it, which it can't from the switch itself). The switch starts
      # it when it changes, i.e. with each new T3 version; it also runs at boot.
      systemd.user.services.t3-install = {
        Unit.Description = "Install T3 Code's background server for this version";
        Service = {
          Type = "oneshot";
          Environment = [ "T3CODE_HOME=${serverHome}" ];
          ExecStart = "${pkgs.writeShellScript "t3-install" ''
            unit=$HOME/.config/systemd/user/t3code.service
            # The app's own `t3` from before T3CODE_INSTALL_BIN_DIR: it shadows ours.
            shim=$HOME/.local/bin/t3
            if [ -f "$shim" ] && ${pkgs.gnugrep}/bin/grep -q "Written by T3 Code" "$shim"; then ${pkgs.coreutils}/bin/rm -f "$shim"; fi
            # One installed before the server had its own home serves ~/.t3,
            # which locks the desktop app out: remove it.
            if [ -f "$unit" ] && ! ${pkgs.gnugrep}/bin/grep -qF "T3CODE_HOME=${serverHome}" "$unit"; then
              ${pkgs.systemd}/bin/systemctl --user disable --now t3code.service || true
              T3CODE_HOME=$HOME/.t3 ${t3-cli}/bin/t3 service uninstall || ${pkgs.coreutils}/bin/rm -f "$unit"
              ${pkgs.coreutils}/bin/rm -f "$unit"
            fi
            if [ ! -f "$unit" ] || [ ! -e "${serverHome}/runtime/versions/${version}/.install-complete" ]; then
              ${t3-cli}/bin/t3 service install
            fi
            # The server listens beyond this machine (T3CODE_LAN), so the Mac
            # links to it straight over NetBird (http://<arc>:3773), with no
            # T3 Connect tunnel and no Tailscale. Only NetBird reaches it: the
            # firewall opens 3773 on wt0 alone. T3 keeps its other service
            # settings in the same file; the server reads it when it starts.
            env=${serverHome}/service.env
            settings=${serverHome}/userdata/settings.json
            state() { ${pkgs.coreutils}/bin/cat "$env" "$settings" 2>/dev/null | ${pkgs.coreutils}/bin/sha256sum; }
            before=$(state)
            ${pkgs.coreutils}/bin/mkdir -p ${serverHome}
            ${pkgs.coreutils}/bin/touch "$env"
            if ${pkgs.gnugrep}/bin/grep -q '^T3CODE_LAN=' "$env"; then
              ${pkgs.gnused}/bin/sed -i 's/^T3CODE_LAN=.*/T3CODE_LAN=true/' "$env"
            else
              echo T3CODE_LAN=true >> "$env"
            fi
            # Linked environments on (a Labs setting), so the Mac's T3 can link
            # to this server; the rest of settings.json stays as T3 wrote it.
            ${pkgs.coreutils}/bin/mkdir -p ${serverHome}/userdata
            [ -s "$settings" ] || echo '{}' > "$settings"
            if [ "$(${pkgs.jq}/bin/jq '.linkedEnvironmentsEnabled' "$settings")" != true ]; then
              ${pkgs.jq}/bin/jq '.linkedEnvironmentsEnabled = true' "$settings" > "$settings.tmp" \
                && ${pkgs.coreutils}/bin/mv "$settings.tmp" "$settings"
            fi
            ${pkgs.systemd}/bin/systemctl --user daemon-reload
            ${pkgs.systemd}/bin/systemctl --user enable --now t3code.service
            if [ "$before" != "$(state)" ]; then
              ${pkgs.systemd}/bin/systemctl --user restart t3code.service
            fi
          ''}";
        };
        Install.WantedBy = [ "default.target" ];
      };

      xdg.configFile."systemd/user/t3code.service.d/nix.conf".text = ''
        [Service]
        Environment="PATH=%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/run/wrappers/bin:/nix/var/nix/profiles/default/bin:/run/current-system/sw/bin"
        Environment="SSH_AUTH_SOCK=%h/.1password/agent.sock"
        # Rootless Docker (dev/containers.nix): its socket is the user's, and
        # agents' `docker`/`docker compose` look for /var/run/docker.sock without this.
        Environment="DOCKER_HOST=unix://%t/docker.sock"
      '';
    };
}
