# Thunar: a graphical file manager next to yazi (Super+E). Thumbnails,
# archives (right-click > extract), USB drives and phones mount on plug-in,
# and the trash works.
{
  flake.modules.nixos.desktop =
    { pkgs, ... }:
    {
      programs.thunar = {
        enable = true;
        plugins = with pkgs; [
          thunar-archive-plugin
          thunar-volman
        ];
      };
      programs.xfconf.enable = true; # Thunar's settings
      services.gvfs.enable = true; # trash, MTP phones, network shares
      services.tumbler.enable = true; # thumbnails
      services.udisks2.enable = true;
      environment.systemPackages = [ pkgs.file-roller ]; # backend for the archive plugin
    };

  flake.modules.homeManager.desktop = {
    wayland.windowManager.mango.settings.bind = [ "SUPER+SHIFT,f,spawn,thunar" ];
  };
}
