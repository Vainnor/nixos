# The compositor. Super is the main modifier; vim keys + arrows both work.
{ inputs, ... }:
{
  flake.modules.nixos.desktop = {
    imports = [ inputs.mango.nixosModules.mango ];
    programs.mango.enable = true;

    # Electron/Chromium apps (Vesktop, Zed's webviews, Steam) use Wayland.
    environment.sessionVariables.NIXOS_OZONE_WL = "1";
  };

  flake.modules.homeManager.desktop =
    { lib, ... }:
    let
      tags = lib.range 1 9;
      dirs = {
        h = "left";
        j = "down";
        k = "up";
        l = "right";
        Left = "left";
        Down = "down";
        Up = "up";
        Right = "right";
      };
      dirBinds = mod: action: lib.mapAttrsToList (key: dir: "${mod},${key},${action},${dir}") dirs;
    in
    {
      imports = [ inputs.mango.hmModules.mango ];

      wayland.windowManager.mango = {
        enable = true;
        settings = {
          # Look
          border_px = 2;
          border_radius = 0; # square, like the rest of the desktop
          gap_inner_horizontal = 6;
          gap_inner_vertical = 6;
          gap_outer_horizontal = 10;
          gap_outer_vertical = 10;
          shadows = 0;
          blur = 0;

          # Snappy animations
          animations = 1;
          animation_type_open = "zoom";
          animation_type_close = "zoom";
          animation_duration_open = 200;
          animation_duration_close = 150;
          animation_duration_move = 200;
          animation_duration_tag = 200;

          # Behaviour
          sloppy_focus = 1;
          warp_cursor = 1;
          focus_cross_monitor = 1;
          focus_on_activate = 1;
          xkb_rules_layout = "us";
          repeat_rate = 40;
          repeat_delay = 300;
          mouse_natural_scrolling = 0;

          tag_rule = map (n: "id:${toString n},layout_name:tile") tags;

          bind = [
            # Apps
            "SUPER,Return,spawn,ghostty"
            "SUPER,space,spawn,fuzzel"
            "SUPER,b,spawn,librewolf"
            "SUPER,e,spawn,ghostty -e yazi"
            "SUPER,v,spawn_shell,cliphist list | fuzzel --dmenu | cliphist decode | wl-copy"
            "SUPER,Escape,spawn,loginctl lock-session"

            # Windows
            "SUPER,q,killclient,"
            "SUPER,f,togglefullscreen,"
            "SUPER,m,togglemaximizescreen,"
            "SUPER,t,togglefloating,"
            "SUPER,Tab,focusstack,next"
            "SUPER,n,switch_layout"
            "SUPER,o,toggleoverview,"
            "SUPER,z,toggle_scratchpad"

            # Monitors
            "SUPER,comma,focusmon,left"
            "SUPER,period,focusmon,right"
            "SUPER+SHIFT,comma,tagmon,left"
            "SUPER+SHIFT,period,tagmon,right"

            # Session
            "SUPER+SHIFT,r,reload_config"
            "SUPER+SHIFT,e,quit"

            # Media keys (volume keys live in system/audio.nix)
            "NONE,XF86AudioPlay,spawn,playerctl play-pause"
            "NONE,XF86AudioNext,spawn,playerctl next"
            "NONE,XF86AudioPrev,spawn,playerctl previous"
          ]
          ++ dirBinds "SUPER" "focusdir"
          ++ dirBinds "SUPER+SHIFT" "exchange_client"
          ++ dirBinds "SUPER+CTRL" "smartresizewin"
          ++ map (n: "SUPER,${toString n},view,${toString n},0") tags
          ++ map (n: "SUPER+SHIFT,${toString n},tag,${toString n},0") tags;

          mousebind = [
            "SUPER,btn_left,moveresize,curmove"
            "SUPER,btn_right,moveresize,curresize"
          ];

          window_rule = [
            "is_floating:1,app_id:com.saivert.pwvucontrol"
            "is_floating:1,app_id:org.rncbc.qpwgraph"
            "is_floating:1,title:^Picture-in-Picture$"
            # The light Windows VM (`just windows-lite`): fullscreen on the
            # ultrawide; the SPICE agent then sizes Windows to 3440x1440.
            "is_fullscreen:1,monitor:model:VG34VQL3A,app_id:virt-viewer"
          ];
        };
      };
    };
}
