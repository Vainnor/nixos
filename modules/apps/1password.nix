# 1Password: the app (starts in the tray at login), its CLI, the SSH agent and
# the LibreWolf extension. SSH keys never touch the disk: `ssh` and git
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
      text = "librewolf\n";
      mode = "0755";
    };
  };

  flake.modules.homeManager.desktop = {
    programs.ssh = {
      enable = true;
      enableDefaultConfig = false;
      settings."*".IdentityAgent = "~/.1password/agent.sock";
    };
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
