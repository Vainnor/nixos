# `nix flake init -t ~/nixos#rust` to start a project with its own pinned toolchain.
{
  flake.templates.rust = {
    path = ../../templates/rust;
    description = "Rust project with a dev shell, loaded automatically by direnv";
  };
}
