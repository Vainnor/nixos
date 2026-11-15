# Mainline's dev shell: `use flake $HOME/nixos#mainline` in
# ~/Programing/Mainline-sim/Mainline/.envrc (dev/projects.nix writes it), and
# the shell the PC's queue worker runs its jobs in (dev/mainline-queue.nix).
# Mainline's own flake.nix (on next) has the same packages, but not git,
# openssh or DOCKER_HOST, which the worker and `just ci-linux` need, so this
# shell stays; keep the two in step.
#
# The Rust toolchain is rustup's (rust-toolchain.toml), so builds match the
# repo's pin. The native libraries are the ones the Linux CI image installs
# (Bevy's audio, input, Wayland and keyboard maps), plus the Vulkan and X11
# libraries Bevy opens at runtime, the GL dispatch library the editor and the
# Lab open, and lsof for the repo's cleanup hooks. Docker for `just ci-linux`
# is the system's rootless Docker (dev/containers.nix), not part of the shell.
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
          # The post-merge cleanup, /cleanup and clean-stale read open files
          # with lsof; without it they keep every worktree and target/.
          lsof
          alsa-lib
          udev
          wayland
          libxkbcommon
          valgrind
        ];
        # Rootless Docker (dev/containers.nix) listens on the user's socket;
        # set it here too, so agent sessions that load this shell find it.
        shellHook = ''
          export DOCKER_HOST="''${DOCKER_HOST:-unix://''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/docker.sock}"
        '';
        # Opened with dlopen when the client, the editor or the Lab runs, not
        # linked. libglvnd is the EGL and GLX dispatch glutin opens (without it
        # the editor and the Lab stop with NoGlutinConfigs); nixpkgs builds it
        # to find the driver NixOS keeps in /run/opengl-driver.
        LD_LIBRARY_PATH = lib.makeLibraryPath (
          with pkgs;
          [
            libglvnd
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
