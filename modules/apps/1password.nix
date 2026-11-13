# 1Password: the app (starts in the tray at login), its CLI, the SSH agent and
# the LibreWolf and Helium extensions. SSH keys never touch the disk: `ssh` and git
# signing ask 1Password, which asks you to unlock.
#
# One-time in the app: Settings > Developer > "Use the SSH agent" and
# "Integrate with 1Password CLI"; Settings > Browser > "Connect with 1Password in the browser".
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.desktop = {
    unfree.allowed = [
      "1password"
      "1password-cli"
    ];
    programs._1password.enable = true;
    programs._1password-gui = {
      enable = true;
      polkitPolicyOwners = [ meta.username ]; # system-auth prompts for the CLI/SSH agent
    };
    # The extension only talks to browsers 1Password trusts.
    environment.etc."1password/custom_allowed_browsers" = {
      text = "librewolf\nhelium\n";
      mode = "0755";
    };
    # Helium finds the app through the manifest 1Password writes to
    # ~/.config/google-chrome/NativeMessagingHosts.
    programs.chromium.extensions = [ "aeblfdkhhhdcdjpifhhbdiojplfjncoa" ];
  };

  flake.modules.homeManager.desktop =
    { config, lib, ... }:
    {
      programs.ssh = {
        enable = true;
        enableDefaultConfig = false;
        settings."*".IdentityAgent = "~/.1password/agent.sock";
      };
      # ~/.ssh/config is a real file you own (mode 600), not a link into
      # /nix/store: in the T3 app's sandbox (its own user namespace) store files
      # show as nobody's, and OpenSSH refuses a config it doesn't see you own.
      home.file.".ssh/config".enable = lib.mkForce false;
      home.activation.sshConfig = lib.hm.dag.entryAfter [ "linkGeneration" ] ''
        run mkdir -p ${config.home.homeDirectory}/.ssh
        run rm -f ${config.home.homeDirectory}/.ssh/config
        run install -m 600 ${config.home.file.".ssh/config".source} ${config.home.homeDirectory}/.ssh/config
      '';
      home.sessionVariables.SSH_AUTH_SOCK = "$HOME/.1password/agent.sock";

      wayland.windowManager.mango.autostart_sh = ''
        1password --silent &
      '';

      programs.librewolf.policies.ExtensionSettings."{d634138d-c276-4fc8-924b-40a0ea21d284}" = {
        install_url = "https://addons.mozilla.org/firefox/downloads/latest/1password-x-password-manager/latest.xpi";
        installation_mode = "force_installed";
      };
    };
}
