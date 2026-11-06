# Ghostty: GPU-accelerated, fast, sane defaults; Stylix sets colours and font.
{
  flake.modules.homeManager.desktop = {
    programs.ghostty = {
      enable = true;
      enableFishIntegration = true;
      settings = {
        window-padding-x = 8;
        window-padding-y = 8;
        window-decoration = false;
        confirm-close-surface = false;
        cursor-style = "block";
        shell-integration-features = "no-cursor";
      };
    };
  };
}
