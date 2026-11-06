# Dev basics: git, direnv + nix-direnv (per-project shells load on `cd`),
# nix-ld (lets random prebuilt binaries run), and a global Rust language server.
# Each project pins its own toolchain in its flake (see templates/rust).
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.dev = {
    programs.nix-ld.enable = true;
  };

  flake.modules.homeManager.dev =
    { pkgs, ... }:
    {
      programs.git = {
        enable = true;
        settings = {
          user.name = meta.username;
          user.email = meta.email;
          init.defaultBranch = "main";
          pull.rebase = true;
          push.autoSetupRemote = true;
        };
      };
      programs.gh.enable = true;
      programs.lazygit.enable = true;

      programs.direnv = {
        enable = true;
        nix-direnv.enable = true;
      };

      home.packages = with pkgs; [
        rustup # quick experiments; real projects use the flake template
        hyperfine
        tokei
      ];
    };
}
