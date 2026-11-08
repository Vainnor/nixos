# OBS, pre-configured for the Vainnor overlay pack (assets/overlays).
#
# What you get on first launch:
#   - Profile "Vainnor": 1920x1080 @ 60, Advanced output, hardware encoding on
#     the host's GPU (H.264 CBR 6000 to Twitch, HEVC CQP for recordings; VA-API
#     on AMD/Intel, NVENC on NVIDIA, x264 without a GPU), Hybrid MP4,
#     stream on Track 1, Twitch VOD on Track 2, record Tracks 1-4.
#   - Scene collections "Vainnor Mono", "Vainnor Horror", "Vainnor Purple":
#     the eight overlay scenes per theme, already layered (game, webcam, page,
#     alerts) exactly like the pack's setup-obs.html builds them on Windows.
#   - "Vainnor Audio": the BEACN routing table rebuilt on PipeWire channels:
#
#       source   tracks   (1 Audience, 2 VOD no music, 3 voices, 4 game+other)
#       Mic      1 2 3
#       Chat     1 2 3
#       Music    1
#       Browser  1 2   4
#       Game     1 2   4
#       System   1 2   4
#
#   - WebSocket server on (random password, written into the overlays'
#     config.js so the webcam bezel follows the "Webcam" source).
#
# Hotkeys (anywhere, OBS doesn't need focus), through obs-websocket:
#   Super+F1..F8   switch to scene 1-8 of the current collection
#                  (Starting Soon, Gameplay, no cam, 16-9, Chatting, BRB, Ending, Offline)
#   Super+F9       start/stop recording
#   Super+F10      mute/unmute the mic in OBS
#   Super+Shift+F12  start/stop the STREAM (shift so it can't happen by accident)
#
# These files are only SEEDED: OBS owns them afterwards, so your changes in
# OBS stick. `vx obs reset` backs them up and seeds fresh copies.
#
# The look: a "Vainnor" variant of OBS's own Yami theme (themes/Vainnor.ovt),
# rebuilt from the palette on every switch: square corners and the theme's
# colours. It's picked in the seeded user.ini; on an existing setup choose it
# once in Settings > Appearance (or `vx obs reset`).
#
# The Twitch stream key and StreamElements token come from sops
# (secrets/secrets.yaml) once meta.secretsReady is on.
# Still yours to do in OBS: connect the Twitch account for VOD track (Settings > Stream),
# pick the Brio in "Webcam" if it isn't picked, and launch games with
# `obs-gamecapture %command%` in Steam so "Game Capture" sees them.
{ config, ... }:
let
  inherit (config) meta;
  home = "/home/${meta.username}";
  overlayDir = "${home}/.local/share/vainnor-overlays";
in
{
  flake.modules.nixos.streaming =
    { pkgs, lib, ... }:
    {
      # Decrypted to /run/secrets/... (owned by you) once secrets are set up.
      sops.secrets = lib.mkIf meta.secretsReady (
        lib.genAttrs
          [
            "twitch/stream_key"
            "streamelements/channel_id"
            "streamelements/jwt"
          ]
          (_: {
            owner = meta.username;
          })
      );

      programs.obs-studio = {
        enable = true;
        enableVirtualCamera = true;
        plugins = with pkgs.obs-studio-plugins; [
          obs-vkcapture # "Game Capture" for Vulkan/OpenGL games
          obs-pipewire-audio-capture # per-application audio capture
          wlrobs # direct Wayland screen capture
        ];
      };
      # `obs-gamecapture` wrapper for Steam launch options.
      environment.systemPackages = [ pkgs.obs-studio-plugins.obs-vkcapture ];
    };

  flake.modules.homeManager.streaming =
    {
      config,
      osConfig,
      pkgs,
      lib,
      ...
    }:
    let
      json = pkgs.formats.json { };

      # Hardware encoders for the host's GPU (host.json `gpu`): VA-API on AMD and
      # Intel, NVENC on NVIDIA, x264 on the CPU without a GPU.
      enc =
        {
          nvidia = {
            stream = "obs_nvenc_h264_tex";
            record = "obs_nvenc_hevc_tex";
            cqp = {
              cqp = 20;
            };
          };
          none = {
            stream = "obs_x264";
            record = "obs_x264";
            cqp = {
              rate_control = "CRF";
              crf = 20;
            };
          };
        }
        .${osConfig.vx.host.gpu} or {
          stream = "ffmpeg_vaapi_tex";
          record = "hevc_ffmpeg_vaapi_tex";
          cqp = { };
        };
      ini = pkgs.formats.ini { };

      themes = {
        mono = "Mono";
        horror = "Horror";
        purple = "Purple";
      };
      defaultTheme = "Purple";

      # Mirrors SCENES in assets/overlays/setup-obs.html.
      scenes = [
        {
          base = "Starting Soon";
          file = "starting-soon.html";
          game = false;
          cam = null;
        }
        {
          base = "Gameplay";
          file = "gameplay.html";
          game = true;
          cam = {
            x = 64;
            y = 622;
            w = 480;
            h = 270;
          };
        }
        {
          base = "Gameplay (no cam)";
          file = "gameplay-nocam.html";
          game = true;
          cam = null;
        }
        {
          base = "Gameplay 16-9";
          file = "gameplay-16x9.html";
          game = true;
          cam = {
            x = 64;
            y = 650;
            w = 480;
            h = 270;
          };
        }
        {
          base = "Just Chatting";
          file = "chatting.html";
          game = false;
          cam = {
            x = 90;
            y = 170;
            w = 1180;
            h = 664;
          };
        }
        {
          base = "BRB";
          file = "brb.html";
          game = false;
          cam = null;
        }
        {
          base = "Ending";
          file = "ending.html";
          game = false;
          cam = null;
        }
        {
          base = "Offline";
          file = "offline.html";
          game = false;
          cam = null;
        }
      ];

      # OBS mixer bitmask: track N = 1 << (N-1).
      tracks = ts: lib.foldl' (acc: t: acc + builtins.elemAt [ 1 2 4 8 16 32 ] (t - 1)) 0 ts;
      audioSources = [
        {
          name = "Mic";
          id = "pulse_input_capture";
          device = "easyeffects_source"; # the BEACN mic after the EasyEffects chain (streaming/mic.nix)
          mixers = tracks [
            1
            2
            3
          ];
        }
        {
          name = "Chat";
          id = "pulse_output_capture";
          device = "channel_chat.monitor";
          mixers = tracks [
            1
            2
            3
          ];
        }
        {
          name = "Music";
          id = "pulse_output_capture";
          device = "channel_music.monitor";
          mixers = tracks [ 1 ];
        }
        {
          name = "Browser";
          id = "pulse_output_capture";
          device = "channel_browser.monitor";
          mixers = tracks [
            1
            2
            4
          ];
        }
        {
          name = "Game";
          id = "pulse_output_capture";
          device = "channel_game.monitor";
          mixers = tracks [
            1
            2
            4
          ];
        }
        {
          name = "System";
          id = "pulse_output_capture";
          device = "channel_system.monitor";
          mixers = tracks [
            1
            2
            4
          ];
        }
      ];
      audioScene = "Vainnor Audio";

      source =
        id: name: settings: extra:
        {
          inherit id name settings;
          versioned_id = id;
          enabled = true;
          muted = false;
          volume = 1.0;
          mixers = 0;
          monitoring_type = 0;
        }
        // extra;

      item =
        n: name:
        {
          pos ? {
            x = 0;
            y = 0;
          },
          bounds ? null,
        }:
        {
          inherit name pos;
          id = n;
          visible = true;
          locked = false;
          rot = 0.0;
          scale = {
            x = 1.0;
            y = 1.0;
          };
          align = 5; # top-left
          bounds_type = if bounds == null then 0 else 2; # 2 = scale to inner bounds
          bounds_align = 0; # centred inside the bounds (letterboxes the ultrawide game)
          bounds =
            if bounds == null then
              {
                x = 0;
                y = 0;
              }
            else
              bounds;
          crop_left = 0;
          crop_top = 0;
          crop_right = 0;
          crop_bottom = 0;
          group_item_backup = false;
          private_settings = { };
        };

      scene =
        name: names:
        source "scene" name {
          id_counter = lib.length names;
          custom_size = false;
          items = lib.imap1 (n: i: item n i.name (removeAttrs i [ "name" ])) names;
        } { };

      browser =
        name: path:
        source "browser_source" name {
          is_local_file = true;
          local_file = path;
          width = 1920;
          height = 1080;
          fps_custom = true;
          fps = 60;
          restart_when_active = true;
          shutdown = true;
          reroute_audio = false;
        } { };

      collection =
        key: label:
        let
          sceneName = s: "Vainnor ${label} ${s.base}";
          alerts = "Vainnor ${label} Alerts overlay";
          page = s: "${overlayDir}/themes/${key}/${s.file}";
          layers =
            s:
            [ { name = audioScene; } ]
            ++ lib.optional s.game {
              name = "Game Capture";
              bounds = {
                x = 1920;
                y = 1080;
              };
            }
            ++ lib.optional (s.cam != null) {
              name = "Webcam";
              pos = {
                inherit (s.cam) x y;
              };
              bounds = {
                x = s.cam.w;
                y = s.cam.h;
              };
            }
            ++ [
              { name = "${sceneName s} (page)"; }
              { name = alerts; }
            ];
        in
        {
          name = "Vainnor ${label}";
          current_scene = sceneName (lib.head scenes);
          current_program_scene = sceneName (lib.head scenes);
          scene_order = map (s: { name = sceneName s; }) scenes ++ [ { name = audioScene; } ];
          sources =
            map (
              a: source a.id "Audio: ${a.name}" { device_id = a.device; } { inherit (a) mixers; }
            ) audioSources
            ++ [
              (scene audioScene (map (a: { name = "Audio: ${a.name}"; }) audioSources))
              (source "vkcapture-source" "Game Capture" { } { })
              (source "v4l2_input" "Webcam" { } { })
              (browser alerts "${overlayDir}/themes/${key}/alerts.html")
            ]
            ++ map (s: browser "${sceneName s} (page)" (page s)) scenes
            ++ map (s: scene (sceneName s) (layers s)) scenes;
          groups = [ ];
          transitions = [ ];
          current_transition = "Fade";
          transition_duration = 300;
          quick_transitions = [ ];
          saved_projectors = [ ];
          preview_locked = false;
          scaling_enabled = false;
          modules = { };
        };

      sceneFiles = lib.mapAttrs' (
        key: label:
        lib.nameValuePair "basic/scenes/Vainnor_${label}.json" (
          json.generate "Vainnor_${label}.json" (collection key label)
        )
      ) themes;

      profile = {
        "basic/profiles/Vainnor/basic.ini" = ini.generate "basic.ini" {
          General.Name = "Vainnor";
          Video = {
            BaseCX = 1920;
            BaseCY = 1080;
            OutputCX = 1920;
            OutputCY = 1080;
            FPSType = 0;
            FPSCommon = 60;
          };
          Audio = {
            SampleRate = 48000;
            ChannelSetup = "Stereo";
          };
          Output.Mode = "Advanced";
          AdvOut = {
            Encoder = enc.stream;
            TrackIndex = 1;
            VodTrackEnabled = true;
            VodTrackIndex = 2;
            RecType = "Standard";
            RecFilePath = "${home}/Videos/OBS";
            RecFormat2 = "hybrid_mp4";
            RecEncoder = enc.record;
            RecTracks = tracks [
              1
              2
              3
              4
            ];
            Track1Name = "Audience";
            Track2Name = "VOD (no music)";
            Track3Name = "Voices";
            Track4Name = "Game + other";
            Track1Bitrate = 192;
            Track2Bitrate = 192;
            Track3Bitrate = 192;
            Track4Bitrate = 192;
          };
        };
        "basic/profiles/Vainnor/streamEncoder.json" = json.generate "streamEncoder.json" {
          rate_control = "CBR";
          bitrate = 6000;
          keyint_sec = 2;
          bf = 2;
        };
        "basic/profiles/Vainnor/recordEncoder.json" = json.generate "recordEncoder.json" (
          {
            rate_control = "CQP";
            qp = 20;
            keyint_sec = 2;
          }
          // enc.cqp
        );
        "user.ini" = ini.generate "user.ini" {
          General.FirstRun = true; # skip the auto-configuration wizard
          Appearance.Theme = obsThemeId;
          Basic = {
            Profile = "Vainnor";
            ProfileDir = "Vainnor";
            SceneCollection = "Vainnor ${defaultTheme}";
            SceneCollectionFile = "Vainnor_${defaultTheme}";
            ConfigOnNewProfile = false;
          };
        };
      };

      seeds = sceneFiles // profile;

      # OBS 30+ themes: a variant (.ovt) only overrides Yami's variables, so
      # it survives OBS updates as long as the variable names stay. A few
      # widgets in Yami hard-code a 2-4px radius and keep it.
      obsThemeId = "com.vainnor.Yami.Flat";
      obsTheme =
        let
          c = config.lib.stylix.colors;
        in
        ''
          @OBSThemeMeta {
              name: 'Vainnor';
              id: '${obsThemeId}';
              extends: 'com.obsproject.Yami';
              author: 'vainnor';
              dark: 'true';
          }

          @OBSThemeVars {
              --grey1: #${c.base03};
              --grey2: #${c.base03};
              --grey3: #${c.base02};
              --grey4: #${c.base02};
              --grey5: #${c.base01};
              --grey6: #${c.base01};
              --grey7: #${c.base00};
              --grey8: #${c.base00};

              --bg_window: #${c.base00};
              --bg_base: #${c.base01};
              --bg_preview: #${c.base00};

              /* Filled "selected" states stay a surface colour so white text
                 stays readable; the accent is for outlines and focus. */
              --primary: #${c.base02};
              --primary_light: #${c.base03};
              --primary_lighter: #${c.base0D};
              --primary_dark: #${c.base01};
              --primary_darker: #${c.base01};

              --warning: #${c.base0A};
              --danger: #${c.base08};

              --text: #${c.base05};
              --text_light: #${c.base06};
              --text_muted: #${c.base04};

              --border_color: #${c.base02};
              --input_border_focus: #${c.base0D};
              --tab_border_selected: #${c.base0D};
              --highlight_color: #${c.base0D};
              --palette_link: #${c.base0D};
              --palette_linkVisited: #${c.base0E};

              --border_radius: 0px;
              --border_radius_small: 0px;
              --border_radius_large: 0px;
              --toggle_radius: 0px;
              --action_row_collapse_radius: 0px;
          }
        '';

      # Overlay settings (js/config.js), regenerated on every switch. Edit here,
      # not in the file. Secrets and the WebSocket password are added at activation.
      overlaySettings = {
        channel = "vainnor"; # Twitch name for chat captions
        minutes = 5; # Starting Soon countdown
        endHold = 20; # seconds on the Ending card
        nowPlaying = "";
        discord = "";
        startTime = "";
        gameplayCrawl = false;
        camera = true;
        cameraSource = "Webcam"; # bezel follows this OBS source
        obsUrl = "ws://127.0.0.1:4455";
        chatMax = 6;
        chatTtl = 60;
        hideUsers = [
          "nightbot"
          "streamelements"
          "streamlabs"
          "moobot"
        ];
        debug = false;
      };

      # The overlay pack, minus config.js (generated below, keeps secrets out of the store).
      overlays = lib.fileset.toSource {
        root = ../../assets/overlays;
        fileset = lib.fileset.difference ../../assets/overlays ../../assets/overlays/js/config.js;
      };

      jq = lib.getExe pkgs.jq;
      obsDir = "$HOME/.config/obs-studio";
      wsConfig = "${obsDir}/plugin_config/obs-websocket/config.json";

      # `obs-ctl scene Gameplay`, `obs-ctl recording toggle`, ... (any obs-cmd command).
      obsCtl = pkgs.writeShellApplication {
        name = "obs-ctl";
        runtimeInputs = with pkgs; [
          obs-cmd
          jq
          gnused
          libnotify
        ];
        text = ''
          pw=$(jq -r .server_password "${wsConfig}" 2>/dev/null) || { notify-send -a obs "obs websocket isn't set up yet"; exit 1; }
          export OBS_WEBSOCKET_URL="obsws://127.0.0.1:4455/$pw"
          if [ "''${1:-}" = scene ]; then
            # "Vainnor Purple Gameplay" -> theme "Purple"; scenes are per theme.
            theme=$(obs-cmd scene current | sed -n 's/^Current scene: Vainnor \(Mono\|Horror\|Purple\) .*/\1/p')
            target="Vainnor ''${theme:-${defaultTheme}} $2"
            obs-cmd scene switch "$target" >/dev/null && notify-send -a obs -t 1500 "scene" "$2"
          else
            out=$(obs-cmd "$@") && notify-send -a obs -t 2000 "obs" "$out"
          fi
        '';
      };
    in
    {
      home.packages = [
        pkgs.chatterino2 # Twitch chat
        obsCtl
      ];

      wayland.windowManager.mango.settings.bind =
        lib.imap1 (n: s: "SUPER,F${toString n},spawn_shell,obs-ctl scene '${s.base}'") scenes
        ++ [
          "SUPER,F9,spawn_shell,obs-ctl recording toggle"
          "SUPER,F10,spawn_shell,obs-ctl audio toggle 'Audio: Mic'"
          "SUPER+SHIFT,F12,spawn_shell,obs-ctl streaming toggle"
        ];

      xdg.configFile."obs-studio/themes/Vainnor.ovt".text = obsTheme;

      home.file.".local/share/vainnor-overlays" = {
        source = overlays;
        recursive = true;
      };

      home.activation.seedObs = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        seed() {
          if [ ! -e "$2" ]; then
            run mkdir -p "$(dirname "$2")"
            run install -m 0644 "$1" "$2"
          fi
        }
        ${lib.concatStrings (lib.mapAttrsToList (dst: src: "seed ${src} \"${obsDir}/${dst}\"\n") seeds)}
        run mkdir -p "$HOME/Videos/OBS"

        # WebSocket: on, with a random password generated once.
        if [ ! -e "${wsConfig}" ]; then
          run mkdir -p "$(dirname "${wsConfig}")"
          pw="$(${pkgs.coreutils}/bin/head -c 32 /dev/urandom | ${pkgs.coreutils}/bin/base64 | ${pkgs.coreutils}/bin/tr -dc 'A-Za-z0-9')"
          ${jq} -n --arg pw "$pw" \
            '{first_load: false, server_enabled: true, server_port: 4455, alerts_enabled: false, auth_required: true, server_password: $pw}' \
            > "${wsConfig}"
          chmod 0600 "${wsConfig}"
        fi

        secret() { if [ -r "/run/secrets/$1" ]; then cat "/run/secrets/$1"; fi; }

        # Overlay settings: rebuilt every switch from overlaySettings + secrets.
        cfg="$HOME/.local/share/vainnor-overlays/js/config.js"
        ${jq} -n \
          --argjson base ${lib.escapeShellArg (builtins.toJSON overlaySettings)} \
          --arg pw "$(${jq} -r .server_password "${wsConfig}")" \
          --arg seId "$(secret streamelements/channel_id)" \
          --arg seJwt "$(secret streamelements/jwt)" \
          '$base + {obsPassword: $pw, seChannelId: $seId, seJwt: $seJwt}' > "$cfg.tmp"
        { printf 'window.VAINNOR = '; cat "$cfg.tmp"; printf ';\n'; } > "$cfg"
        rm -f "$cfg.tmp"
        chmod 0600 "$cfg"

        # Twitch stream key from sops, seeded into the profile once.
        svc="${obsDir}/basic/profiles/Vainnor/service.json"
        if [ ! -e "$svc" ] && [ -r /run/secrets/twitch/stream_key ]; then
          ${jq} -n --arg key "$(secret twitch/stream_key)" \
            '{type: "rtmp_common", settings: {service: "Twitch", server: "auto", key: $key, bwtest: false}}' > "$svc"
          chmod 0600 "$svc"
        fi
      '';
    };
}
