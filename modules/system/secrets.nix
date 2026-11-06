# sops-nix: secrets live encrypted in secrets/secrets.yaml (safe in a public
# repo) and are decrypted at boot into /run/secrets with the PC's SSH host key.
# Recipients are in .sops.yaml: your Mac's age key (to edit) and the PC.
#
# Features declare what they need, guarded by meta.secretsReady, e.g.
#   sops.secrets."twitch/stream_key".owner = "vainnor";
{ inputs, ... }:
{
  flake.modules.nixos.base = {
    imports = [ inputs.sops-nix.nixosModules.sops ];

    sops = {
      defaultSopsFile = ../../secrets/secrets.yaml;
      age.sshKeyPaths = [ "/persistent/etc/ssh/ssh_host_ed25519_key" ];
      gnupg.sshKeyPaths = [ ];
    };
  };
}
