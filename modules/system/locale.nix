{ config, ... }:
{
  flake.modules.nixos.base =
    { pkgs, ... }:
    {
      time.timeZone = config.meta.timeZone;
      i18n.defaultLocale = "en_US.UTF-8";
      console = {
        keyMap = "us";
        # Terminus at 24px: readable on the 4K screens from early boot (the
        # LUKS prompt) through to the greeter.
        font = "ter-v24n";
        packages = [ pkgs.terminus_font ];
        earlySetup = true;
      };
    };
}
