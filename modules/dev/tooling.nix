# Dev basics: git, direnv + nix-direnv (per-project shells load on `cd`),
# nix-ld (lets random prebuilt binaries run), and a global Rust language server.
# Each project pins its own toolchain in its flake (see templates/rust).
#
# Commits are signed with your SSH key from 1Password (meta.signingKey); GitHub
# shows them as Verified once the key is added there as a *signing* key.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.dev = {
    programs.nix-ld.enable = true;
  };

  flake.modules.homeManager.dev =
    { pkgs, lib, ... }:
    let
      inherit (pkgs.stdenv.hostPlatform) isLinux;
    in
    {
      programs.git = {
        enable = true;
        signing = {
          format = "ssh";
          key = meta.signingKey;
          signByDefault = true;
          # 1Password signs (asks for Touch ID / your 1Password unlock). The Mac
          # keeps the signer it already has.
          signer = lib.mkIf isLinux "${pkgs._1password-gui}/bin/op-ssh-sign";
        };
        settings = {
          gpg.ssh.allowedSignersFile = "~/.config/git/allowed_signers";
          user.name = meta.username;
          user.email = meta.email;
          init.defaultBranch = "main";
          pull.rebase = true;
          push.autoSetupRemote = true;
        };
      };
      xdg.configFile."git/allowed_signers".text = "${meta.email} ${meta.signingKey}\n";

      programs.gh.enable = true;
      programs.lazygit.enable = true;

      programs.direnv = {
        enable = true;
        nix-direnv.enable = true;
      };

      home.packages =
        with pkgs;
        [
          hyperfine
          tokei
        ]
        # quick experiments; real projects use the flake template. The Mac has its own rustup.
        ++ lib.optional isLinux rustup;
    };
}
