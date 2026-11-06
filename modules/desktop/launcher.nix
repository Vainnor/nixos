# fuzzel: app launcher, and the picker for clipboard history (Super+V).
{
  flake.modules.homeManager.desktop = {
    programs.fuzzel = {
      enable = true;
      settings.main = {
        terminal = "ghostty -e";
        layer = "overlay";
        width = 40;
        lines = 12;
        prompt = "'❯ '";
      };
    };
  };
}
