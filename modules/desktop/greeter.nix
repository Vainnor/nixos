# Text login (greetd + tuigreet) that starts Mango. No display manager bloat.
{
  flake.modules.nixos.desktop =
    { pkgs, lib, ... }:
    {
      services.greetd = {
        enable = true;
        settings.default_session = {
          command = "${lib.getExe pkgs.tuigreet} --time --remember --asterisks --cmd mango";
          user = "greeter";
        };
      };
    };
}
