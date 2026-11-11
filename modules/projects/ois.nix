# OIS's dev shell: `use flake $HOME/nixos#ois` in ~/Programing/OIS/.envrc
# (dev/projects.nix writes it). Kept here rather than in OIS, whose rules leave
# its branches and commits to you; copy it in as OIS's own flake when you like.
#
# Rust is rustup's rolling nightly (rust-toolchain.toml), linked with clang +
# mold (.cargo/config.toml). pnpm is corepack's, at package.json's pin. Postgres
# runs in docker compose as in the repo; psql and sqlx-cli are here. The desktop
# app (Tauri) needs the WebKitGTK stack CI installs with apt.
{
  perSystem =
    { pkgs, lib, ... }:
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      devShells.ois = pkgs.mkShell {
        packages = with pkgs; [
          rustup
          clang
          mold
          pkg-config
          just
          cargo-nextest
          cargo-deny
          sccache # on PATH; opt in with RUSTC_WRAPPER=sccache
          sqlx-cli
          gitleaks
          postgresql_17 # psql, pg_dump
          nodejs_22
          corepack_22 # pnpm@9.12.0 from package.json
          # Tauri (desktop/)
          webkitgtk_4_1
          gtk3
          libsoup_3
          librsvg
          libayatana-appindicator
          glib-networking
          openssl
        ];
        # Rootless Docker (dev/containers.nix) listens on the user's socket;
        # set it here too, so agent sessions that load this shell find it.
        shellHook = ''
          export DOCKER_HOST="''${DOCKER_HOST:-unix://''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/docker.sock}"
        '';
        # TLS for the webview's network requests.
        GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules";
        # Ayatana's tray library is opened at runtime.
        LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.libayatana-appindicator ];
      };
    };
}
