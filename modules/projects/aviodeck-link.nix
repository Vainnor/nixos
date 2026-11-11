# aviodeck-link's dev shell: `use flake $HOME/nixos#aviodeck-link` in
# ~/Programing/aviodeck-link/.envrc (dev/projects.nix writes it and keeps it
# out of git through .git/info/exclude), so nothing lands in its repo.
#
# Tauri v2 with npm workspaces. The Windows-only parts (SimConnect, Windows
# Graphics Capture) are gated to Windows targets, so it builds and tests here;
# releases build on GitHub's windows-latest runners. Playwright's browsers come
# from `npx playwright install` (they must match the npm package's version).
{
  perSystem =
    { pkgs, lib, ... }:
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      devShells.aviodeck-link = pkgs.mkShell {
        packages = with pkgs; [
          rustup
          pkg-config
          nodejs_22
          # Tauri
          webkitgtk_4_1
          gtk3
          libsoup_3
          librsvg
          libayatana-appindicator
          glib-networking
          openssl
        ];
        GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules";
        LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.libayatana-appindicator ];
      };
    };
}
