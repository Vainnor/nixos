# Locking: the lock screen itself is part of the shell (desktop/shell.nix).
# Locks after 10 min idle, screens off after 15, and always before sleep.
# Super+Escape (or the power menu) locks now.
{
  flake.modules.homeManager.desktop =
    { pkgs, lib, ... }:
    let
      lock = "/etc/profiles/per-user/$USER/bin/vshell ipc call lock lock";
    in
    {
      services.swayidle = {
        enable = true;
        events = {
          before-sleep = lock;
          lock = lock; # `loginctl lock-session`
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
