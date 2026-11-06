{ config, ... }:
{
  flake.modules.nixos.base = {
    time.timeZone = config.meta.timeZone;
    i18n.defaultLocale = "en_US.UTF-8";
    console.keyMap = "us";
  };
}
