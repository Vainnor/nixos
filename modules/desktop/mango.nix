# The compositor. Super is the main modifier; vim keys + arrows both work.
{ inputs, ... }:
{
  flake.modules.nixos.desktop = {
    imports = [ inputs.mango.nixosModules.mango ];
    programs.mango.enable = true;

    # Electron/Chromium apps (Vesktop, 1Password, Zed's webviews) use Wayland.
    # Steam doesn't: its client is X11 (CEF under XWayland) either way.
    environment.sessionVariables.NIXOS_OZONE_WL = "1";

    # A keyring for apps that store passwords through the Secret portal or
    # libsecret (Mango's portal config already sends Secret to gnome-keyring).
    # Unlocked with your login password at the greetd login.
    services.gnome.gnome-keyring.enable = true;
    security.pam.services.greetd.enableGnomeKeyring = true;
    # 1Password is the SSH agent (apps/1password.nix); keep the keyring's own
    # SSH agent (gcr) off so the two don't fight over SSH_AUTH_SOCK.
    services.gnome.gcr-ssh-agent.enable = false;
  };

  flake.modules.homeManager.desktop =
    { lib, osConfig, ... }:
    let
      primary = osConfig.vx.host.primaryMonitor;
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
            "SUPER,b,spawn,librewolf"
            "SUPER,e,spawn,ghostty -e yazi"

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

            # Session (log out is in the power menu, Super+Shift+Escape)
            "SUPER+SHIFT,r,reload_config"

            # Media keys (volume keys and the shell's own binds: desktop/shell.nix)
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
            # `vx wallpaper` started from the shell's wallpaper picker
            "is_floating:1,width:900,height:500,title:^vx-rebuild$"
            "is_floating:1,app_id:com.saivert.pwvucontrol"
            "is_floating:1,app_id:org.rncbc.qpwgraph"
            "is_floating:1,title:^Picture-in-Picture$"
            # Dialogs and small tool windows float (floating windows open centred).
            # Rules split on commas, so the title regexes can't contain one.
            "is_floating:1,app_id:^polkit-gnome-authentication-agent-1$"
            "is_floating:1,app_id:^xdg-desktop-portal-gtk$" # file pickers
            "is_floating:1,app_id:^(zenity|org.gnome.Zenity)$"
            "is_floating:1,width:1000,height:700,app_id:^com.github.wwmm.easyeffects$"
            "is_floating:1,app_id:^1[Pp]assword$,title:^Quick Access"
            "is_floating:1,app_id:virt-manager,title:^New VM$"
            # Steam (X11, app_id is its WM_CLASS): everything but the main window.
            "is_floating:1,app_id:^steam$,title:^(Friends List|Steam Settings|Screenshot Manager|Sign in to Steam|Steam - Self Updater|Properties - .*|Install - .*|Uninstall - .*)$"
            # The light Windows VM (`vx windows lite`): fullscreen on the main
            # screen (host.json primaryMonitor); the SPICE agent then sizes
            # Windows to fit.
            (
              if primary != null then
                "is_fullscreen:1,monitor:model:${primary},app_id:virt-viewer"
              else
                "is_fullscreen:1,app_id:virt-viewer"
            )
          ];
        };
      };
    };
}
