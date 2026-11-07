# vx, built from this repo. Exposed as `packages.<system>.vx` (so
# `nix run github:vainnor/nixos#vx`) and installed by modules/system/vx.nix.
{
  lib,
  stdenv,
  rustPlatform,
  installShellFiles,
}:
rustPlatform.buildRustPackage {
  pname = "vx";
  version = (lib.importTOML ./Cargo.toml).package.version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ installShellFiles ];
  postInstall = lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd vx \
      --fish <($out/bin/vx completions fish) \
      --bash <($out/bin/vx completions bash) \
      --zsh <($out/bin/vx completions zsh)
  '';

  meta = {
    description = "One command for this NixOS / nix-darwin flake";
    license = lib.licenses.mit;
    mainProgram = "vx";
  };
}
