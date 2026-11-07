# Text login (greetd + tuigreet) that starts Mango. No display manager bloat.
# The colours are the console's named ANSI colours, which Stylix sets from the
# palette (console.colors), so the greeter follows the theme too.
{
  flake.modules.nixos.desktop =
    { pkgs, lib, ... }:
    let
      theme = lib.concatStringsSep ";" [
        "border=blue"
        "title=blue"
        "greet=white"
        "prompt=blue"
        "input=white"
        "action=gray"
        "button=blue"
        "time=gray"
        "container=black"
        "text=white"
      ];
      # Mango sets XDG_CURRENT_DESKTOP itself at startup, but only for its own
      # children; setting both here means the session (and its portals) see it
      # from the very first process.
      session = "env XDG_CURRENT_DESKTOP=mango XDG_SESSION_DESKTOP=mango mango";
    in
    {
      services.greetd = {
        enable = true;
        settings.default_session = {
          command = lib.concatStringsSep " " [
            (lib.getExe pkgs.tuigreet)
            "--time --time-format '%a %d %b  %H:%M'"
            "--greeting 'arc'"
            "--remember --asterisks"
            "--theme '${theme}'"
            "--cmd '${session}'"
          ];
          user = "greeter";
        };
      };
    };
}
