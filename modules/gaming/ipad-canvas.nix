# Drawing in Aseprite from the iPad mini with the Apple Pencil.
#
# Moonlight on the iPad sends the Pencil to Sunshine as a pen (pressure, tilt),
# and Sunshine replays it on a virtual drawing tablet that Mango hands to apps
# like any USB tablet. The catch is shape: streaming the 21:9 ultrawide to a
# ~3:2 iPad leaves a tiny letterboxed canvas. So the "Aseprite (iPad)" app in
# Moonlight gets its own screen instead:
#
#   start: Mango creates a virtual output "ipad" at exactly the resolution and
#          frame rate Moonlight asked for (so nothing is scaled or
#          letterboxed), at 2x for a readable UI on a high-DPI client.
#          Sunshine streams that output, and Aseprite opens (or the running
#          one moves) onto it, fullscreen.
#   quit:  (Moonlight > long-press the app > Quit) Aseprite leaves fullscreen,
#          moves back to the main screen, and the virtual output goes away.
#
# Pencil pressure: Aseprite (X11, under XWayland) looks for a "stylus" input
# device once, when it starts, and Sunshine only creates its virtual pen when
# the stream connects. So Aseprite is launched only once XWayland has that
# device. One that was already open before the first iPad session of a login
# never sees the pen: restart it once.
#
# Sunshine's screen choice is one global setting (output_name). It's "ipad":
# when that output doesn't exist Sunshine falls back to its first screen, the
# ultrawide it always streamed, so "Desktop" from the Mac is unchanged. Only
# while an iPad session is running would Desktop show the iPad screen.
#
# Setting apps and output_name here means Sunshine's web UI can no longer
# change its settings or apps (pairing and PINs still work there).
#
# iPad setup, once: install NetBird (its peer must be in the
# remote-desktop-clients group, which the Sunshine policies allow) and
# Moonlight, add arc.netbird.cloud and pair (see sunshine.nix). In Moonlight's
# settings pick the "Full Screen" resolution (the iPad's own) and Touchscreen
# touch mode.
{ ... }:
let
  output = "ipad";
in
{
  # Sunshine's virtual pen reports positions relative to the streamed screen
  # (0..1 across it), but an unmapped tablet spans every monitor, so the
  # Pencil landed all over the desktop. Pin Sunshine's pen (and touchscreen)
  # to the iPad screen.
  flake.modules.homeManager.gaming = {
    wayland.windowManager.mango.settings.device_rule = [
      "name:libvirtualhid Pen Tablet,monitor:${output}"
      "name:libvirtualhid Touchscreen,monitor:${output}"
    ];
  };

  flake.modules.nixos.gaming =
    { config, lib, pkgs, ... }:
    let
      mango = config.programs.mango.package;
      primary = config.vx.host.primaryMonitor;
      canvas = pkgs.writeShellApplication {
        name = "ipad-canvas";
        runtimeInputs = [
          mango
          pkgs.jq
          pkgs.wlr-randr
          pkgs.xinput
          pkgs.util-linux
          pkgs.coreutils
          pkgs.gnugrep
        ];
        text = ''
          shopt -s nullglob
          socks=("''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"/mango-*.sock)
          if [ ''${#socks[@]} -eq 0 ]; then
            echo "ipad-canvas: Mango isn't running" >&2
            exit 1
          fi
          export MANGO_INSTANCE_SIGNATURE="''${socks[0]}"

          has_output() {
            mmsg get all-monitors | jq -e 'any(.monitors[]; .name == "${output}")' >/dev/null
          }
          asepriteWindows() {
            mmsg get all-clients | jq -r '.clients[] | select(.appid | test("^aseprite$"; "i")) | .id'
          }
          isFullscreen() {
            mmsg get client "$1" | jq -e '.is_fullscreen' >/dev/null
          }
          # Onto the iPad screen, fullscreen.
          place() {
            mmsg dispatch tagmon,${output} client,"$1" >/dev/null
            isFullscreen "$1" || mmsg dispatch togglefullscreen client,"$1" >/dev/null
          }
          hasStylus() {
            xinput list --name-only 2>/dev/null | grep -qi stylus
          }

          case "''${1:-}" in
            # Sunshine prep command: the screen, sized like the client's stream.
            start)
              width="''${SUNSHINE_CLIENT_WIDTH:-2266}"
              height="''${SUNSHINE_CLIENT_HEIGHT:-1488}"
              fps="''${SUNSHINE_CLIENT_FPS:-60}"
              scale=1
              [ "$height" -ge 1200 ] && scale=2
              has_output || mmsg dispatch create_virtual_output,${output} >/dev/null
              for _ in $(seq 50); do has_output && break; sleep 0.1; done
              wlr-randr --output ${output} --custom-mode "''${width}x''${height}@''${fps}Hz" --scale "$scale"
              for id in $(asepriteWindows); do place "$id"; done
              ;;
            # Sunshine detached command: Aseprite, once the pen exists.
            launch)
              for _ in $(seq 150); do hasStylus && break; sleep 0.2; done
              ids="$(asepriteWindows)"
              if [ -z "$ids" ]; then
                setsid -f ${lib.getExe pkgs.aseprite} >/dev/null 2>&1
                for _ in $(seq 200); do
                  ids="$(asepriteWindows)"
                  [ -n "$ids" ] && break
                  sleep 0.1
                done
              fi
              for id in $ids; do place "$id"; done
              ;;
            stop)
              for id in $(asepriteWindows); do
                if isFullscreen "$id"; then mmsg dispatch togglefullscreen client,"$id" >/dev/null; fi
                ${lib.optionalString (primary != null) ''mmsg dispatch tagmon,model:${primary} client,"$id" >/dev/null''}
              done
              mmsg dispatch destroy_all_virtual_output >/dev/null
              ;;
            *)
              echo "usage: ipad-canvas start|launch|stop" >&2
              exit 2
              ;;
          esac
        '';
      };
    in
    {
      environment.systemPackages = [ canvas ];

      # libinput decides a pen's tip is down from pressure alone (above 5% of
      # the range, up below 1%), ignoring BTN_TOUCH. A quick Pencil tap
      # starts with almost no force, so it never got there: taps did nothing
      # and only strokes pressed past it drew. For Sunshine's pen (pressure
      # 0-4096), any pressure is contact. Read when Mango starts: log out and
      # back in after changing it.
      environment.etc."libinput/local-overrides.quirks".text = ''
        [Sunshine virtual pen]
        MatchName=libvirtualhid Pen Tablet
        MatchUdevType=tablet
        AttrPressureRange=1:0
      '';

      services.sunshine = {
        settings.output_name = output;
        # Sunshine's own defaults (minus its X11-only "Low Res Desktop"), plus
        # the iPad canvas.
        applications.apps = [
          {
            name = "Desktop";
            image-path = "desktop.png";
          }
          {
            name = "Steam Big Picture";
            image-path = "steam.png";
            detached = [ "setsid steam steam://open/bigpicture" ];
            prep-cmd = [
              {
                do = "";
                undo = "setsid steam steam://close/bigpicture";
              }
            ];
          }
          {
            name = "Aseprite (iPad)";
            image-path = "${pkgs.aseprite}/share/icons/hicolor/256x256/apps/aseprite.png";
            prep-cmd = [
              {
                do = "${lib.getExe canvas} start";
                undo = "${lib.getExe canvas} stop";
              }
            ];
            detached = [ "${lib.getExe canvas} launch" ];
          }
        ];
      };
    };
}
