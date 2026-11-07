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

    # "open terminal here" as a Thunar custom action that runs Ghostty directly.
    # Thunar's built-in one goes through exo-open and the Xfce helpers, which
    # need xfce4-settings (not installed) and don't know Ghostty.
    xdg.configFile."Thunar/uca.xml".text = ''
      <?xml version="1.0" encoding="UTF-8"?>
      <actions>
      <action>
        <icon>utilities-terminal</icon>
        <name>open terminal here</name>
        <submenu></submenu>
        <unique-id>1700000000000000-1</unique-id>
        <command>ghostty --working-directory=%f</command>
        <description>open ghostty in this folder</description>
        <range></range>
        <patterns>*</patterns>
        <startup-notify/>
        <directories/>
      </action>
      </actions>
    '';
  };
}
