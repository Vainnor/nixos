# One template per language. Each project gets its own toolchain from its own
# flake.lock, loaded by direnv when you `cd` in and gone when you leave, so
# projects on different versions (Python 3.12 vs 3.13, Node 20 vs 22, JDK 17
# vs 21) never see each other and nothing is installed system-wide.
#
#   vx new python ~/code/thing      (or: nix flake init -t ~/nixos#python)
{ lib, ... }:
let
  templates = {
    python = "Python: uv + a pinned Python, venv in .venv";
    node = "TypeScript / JavaScript: Node + pnpm, deps in node_modules";
    java = "Java: JDK + Gradle + Maven";
    go = "Go: pinned Go, gopls, golangci-lint, delve";
    php = "PHP: PHP with extensions + Composer";
    rust = "Rust: cargo, clippy, rust-analyzer";
    c = "C / C++: clang tools, CMake, Ninja";
  };
in
{
  flake.templates = lib.mapAttrs (name: description: {
    path = ../../templates + "/${name}";
    inherit description;
  }) templates;
}
