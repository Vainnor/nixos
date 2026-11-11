# Mainline's dev shell: `use flake $HOME/nixos#mainline` in
# ~/Programing/Mainline-sim/Mainline/.envrc (dev/projects.nix writes it), and
# the shell the PC's queue worker runs its jobs in (dev/mainline-queue.nix).
# The same as the flake.nix on Mainline's chore/nix-dev-shell branch; once that
# is merged, this can go and the .envrc becomes plain `use flake`.
#
# The Rust toolchain is rustup's (rust-toolchain.toml), so builds match the
# repo's pin. The native libraries are the ones the Linux CI image installs
# (Bevy's audio, input, Wayland and keyboard maps), plus the Vulkan and X11
# libraries Bevy opens at runtime. Docker for `just ci-linux` is the system's
# rootless Docker (dev/containers.nix), not part of the shell.
{
  perSystem =
    { pkgs, lib, ... }:
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      devShells.mainline = pkgs.mkShell {
        packages = with pkgs; [
          rustup
          pkg-config
          just
          cargo-nextest
          cargo-deny
          gitleaks
          python3
          git
          openssh
          alsa-lib
          udev
          wayland
          libxkbcommon
          valgrind
        ];
        # Opened with dlopen when the client runs, not linked.
        LD_LIBRARY_PATH = lib.makeLibraryPath (
          with pkgs;
          [
            vulkan-loader
            wayland
            libxkbcommon
            libx11
            libxcursor
            libxi
            libxrandr
            alsa-lib
            udev
          ]
        );
      };
    };
}
