# SwayNotificationCenter: pop-ups plus a panel with history, Do Not Disturb
# and media controls. Super+Shift+N opens the panel; the bell in the bar
# shows unread count (click: panel, right-click: Do Not Disturb).
{
  flake.modules.homeManager.desktop = {
    services.swaync = {
      enable = true;
      settings = {
        positionX = "right";
        positionY = "top";
        timeout = 5;
        timeout-low = 3;
        timeout-critical = 0;
        fit-to-screen = true;
        control-center-width = 420;
        notification-window-width = 400;
        widgets = [
          "title"
          "dnd"
          "mpris"
          "notifications"
        ];
      };
    };

    wayland.windowManager.mango.settings.bind = [
      "SUPER+SHIFT,n,spawn,swaync-client -t -sw"
    ];

    programs.waybar.settings.main = {
      "custom/notification" = {
        exec = "swaync-client -swb";
        return-type = "json";
        format = "{icon}";
        format-icons = {
          none = "󰂚";
          notification = "󱅫";
          dnd-none = "󰂛";
          dnd-notification = "󰂛";
        };
        tooltip = false;
        on-click = "swaync-client -t -sw";
        on-click-right = "swaync-client -d -sw";
        escape = true;
      };
    };
  };
}
