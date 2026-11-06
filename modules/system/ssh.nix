# OpenSSH server reachable only over NetBird (wt0), keys only.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base = {
    services.openssh = {
      enable = true;
      openFirewall = false;
      settings = {
        PasswordAuthentication = false;
        KbdInteractiveAuthentication = false;
        PermitRootLogin = "no";
        AllowUsers = [ meta.username ];
      };
    };
    networking.firewall.interfaces.wt0.allowedTCPPorts = [ 22 ];

    users.users.${meta.username}.openssh.authorizedKeys.keys = meta.sshKeys;
  };
}
