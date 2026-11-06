# Waybar on both monitors, started by systemd with the Mango session.
# Plain lowercase text labels instead of icons; the look is in desktop/square.nix.
{
  flake.modules.homeManager.desktop = {
    programs.waybar = {
      enable = true;
      systemd.enable = true;
      settings.main = {
        layer = "top";
        position = "top";
        height = 28;
        spacing = 0;
        modules-left = [ "ext/workspaces" ];
        modules-center = [ "clock" ];
        modules-right = [
          "tray"
          "pulseaudio"
          "network"
          "cpu"
          "memory"
          "custom/notification"
          "custom/power"
        ];
        "custom/power" = {
          format = "[ off ]";
          tooltip = false;
          on-click = "power-menu";
        };
        "ext/workspaces" = {
          format = "{name}";
          on-click = "activate";
          sort-by-id = true;
        };
        clock = {
          format = "{:%Y-%m-%d  %H:%M}";
          tooltip-format = "<tt>{calendar}</tt>";
        };
        pulseaudio = {
          format = "vol {volume}%";
          format-muted = "vol muted";
          on-click = "pwvucontrol";
        };
        network = {
          format-wifi = "net {essid}";
          format-ethernet = "net wired";
          format-disconnected = "net offline";
          tooltip-format = "{ifname} {ipaddr}";
        };
        cpu.format = "cpu {usage}%";
        memory.format = "mem {percentage}%";
        tray.spacing = 8;
      };
    };
  };
}
