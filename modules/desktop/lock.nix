# Lock after 10 min idle, screens off after 15, lock before sleep.
{
  flake.modules.nixos.desktop = {
    security.pam.services.swaylock = { };
  };

  flake.modules.homeManager.desktop =
    { pkgs, lib, ... }:
    let
      lock = "${lib.getExe pkgs.swaylock} -f";
    in
    {
      programs.swaylock.enable = true;
      services.swayidle = {
        enable = true;
        events = {
          before-sleep = lock;
          lock = lock;
        };
        timeouts = [
          {
            timeout = 600;
            command = lock;
          }
          {
            timeout = 900;
            command = "${lib.getExe pkgs.wlopm} --off '*'";
            resumeCommand = "${lib.getExe pkgs.wlopm} --on '*'";
          }
        ];
      };
    };
}
