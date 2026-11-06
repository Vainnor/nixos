# Waybar on both monitors, started by systemd with the Mango session.
{
  flake.modules.homeManager.desktop = {
    programs.waybar = {
      enable = true;
      systemd.enable = true;
      settings.main = {
        layer = "top";
        position = "top";
        height = 28;
        spacing = 8;
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
          format = "⏻";
          tooltip = false;
          on-click = "power-menu";
        };
        "ext/workspaces" = {
          format = "{name}";
          on-click = "activate";
          sort-by-id = true;
        };
        clock = {
          format = "{:%a %b %d  %H:%M}";
          tooltip-format = "<tt>{calendar}</tt>";
        };
        pulseaudio = {
          format = "{icon} {volume}%";
          format-muted = "󰝟 muted";
          format-icons.default = [
            "󰕿"
            "󰖀"
            "󰕾"
          ];
          on-click = "pwvucontrol";
        };
        network = {
          format-wifi = "󰖩 {essid}";
          format-ethernet = "󰈀 wired";
          format-disconnected = "󰖪 offline";
          tooltip-format = "{ifname} {ipaddr}";
        };
        cpu.format = "󰍛 {usage}%";
        memory.format = "󰘚 {percentage}%";
        tray.spacing = 8;
      };
    };
  };
}
