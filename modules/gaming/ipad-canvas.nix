# Drawing in Aseprite from the iPad mini with the Apple Pencil.
#
# Moonlight on the iPad sends the Pencil to Sunshine as a pen (pressure, tilt),
# and Sunshine replays it on a virtual drawing tablet that Mango hands to apps
# like any USB tablet. The catch is shape: streaming the 21:9 ultrawide to a
# ~3:2 iPad leaves a tiny letterboxed canvas. So the "Aseprite (iPad)" app in
# Moonlight gets its own screen instead:
#
#   start: Mango creates a virtual output "ipad" the size of the iPad's
#          display, Aseprite opens (or the running one moves) onto it, and
#          Sunshine streams that output.
#   quit:  (Moonlight > long-press the app > Quit) Aseprite moves back to the
#          main screen and the virtual output goes away.
#
# Sunshine's screen choice is one global setting (output_name). It's "ipad":
# when that output doesn't exist Sunshine falls back to its first screen, the
# ultrawide it always streamed, so "Desktop" from the Mac is unchanged. Only
# while an iPad session is running would Desktop show the iPad screen.
#
# Setting apps and output_name here means Sunshine's web UI can no longer
# change its settings or apps (pairing and PINs still work there).
#
# iPad setup, once: install NetBird (same account) and Moonlight, add
# arc.netbird.cloud and pair (see sunshine.nix), and in Moonlight's settings
# set the resolution to the iPad's native one.
{ ... }:
let
  output = "ipad";
  # iPad mini 6th/7th gen: 2266x1488 pixels, drawn at 2x so Aseprite's UI is a
  # readable size on an 8.3" screen. (5th gen: 2048x1536.)
  width = 2266;
  height = 1488;
  scale = 2;
in
{
  flake.modules.homeManager.gaming = {
    wayland.windowManager.mango.settings.monitor_rule = [
      "name:^${output}$,width:${toString width},height:${toString height},refresh:60,scale:${toString scale}"
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
          pkgs.util-linux
          pkgs.coreutils
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

          case "''${1:-}" in
            start)
              has_output || mmsg dispatch create_virtual_output,${output}
              for _ in $(seq 50); do has_output && break; sleep 0.1; done
              ids="$(asepriteWindows)"
              if [ -z "$ids" ]; then
                setsid -f ${lib.getExe pkgs.aseprite} >/dev/null 2>&1
                for _ in $(seq 200); do
                  ids="$(asepriteWindows)"
                  [ -n "$ids" ] && break
                  sleep 0.1
                done
              fi
              for id in $ids; do mmsg dispatch tagmon,${output} client,"$id"; done
              ;;
            stop)
              ${lib.optionalString (primary != null) ''
                for id in $(asepriteWindows); do mmsg dispatch tagmon,model:${primary} client,"$id"; done
              ''}
              mmsg dispatch destroy_all_virtual_output
              ;;
            *)
              echo "usage: ipad-canvas start|stop" >&2
              exit 2
              ;;
          esac
        '';
      };
    in
    {
      environment.systemPackages = [ canvas ];

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
          }
        ];
      };
    };
}
