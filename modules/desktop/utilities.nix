# Small Wayland tools: clipboard history, screenshots, polkit prompts.
{
  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      services.cliphist.enable = true;
      services.polkit-gnome.enable = true;

      home.packages = with pkgs; [
        wl-clipboard
        grim
        slurp
        wlr-randr
        libnotify
      ];
    };
}
