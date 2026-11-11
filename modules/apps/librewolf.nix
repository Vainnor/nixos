# LibreWolf. Two profiles: `default` for normal browsing and `i2p`, which sends
# everything through the local i2pd HTTP proxy so .i2p traffic never mixes with
# your normal session. Launch it with `librewolf -P i2p` (or Super+Shift+B).
#
# Same look as the Mac: tabs live in Sidebery (vertical, in the sidebar), the
# tab strip and sidebar header are hidden, every corner is square, and the
# about: pages use the system palette (assets/librewolf).
#
# Sidebery's own setup comes from the Mac too (assets/librewolf/sidebery.json:
# settings and the seven panels; sidebery.css.in: its style, accent from the
# theme). The panels are the `sections` below (Personal, Media, VATSIM,
# Learning, Homelab, Dev, I2P), each with its own container: sites listed for
# a section reopen in its container and land in its panel; I2P's container
# goes through i2pd's proxy. Pinned tabs can't be declared; pin them yourself. LibreWolf only loads it into an empty Sidebery,
# so a switch that changes it empties Sidebery's store (with LibreWolf closed;
# otherwise it waits for the next switch). Your tabs stay, but they all land in
# the first panel again, so it only happens when the setup itself changed.
# Changes made in Sidebery's own settings are lost then: put them in the JSON.
{ config, lib, ... }:
let
  inherit (config) meta;
  palette = import ../../assets/themes meta;

  sidebery = "{3c078156-979c-498b-8990-85f7987dd929}";
  stylus = "{7a7a4a92-a2a0-41d1-9fd7-1e92480d612d}";

  userChrome = builtins.readFile ../../assets/librewolf/userChrome.css;

  # Fill the @baseXX@ / @border@ / @accent2@ placeholders from the theme.
  userContent =
    c:
    let
      names = [
        "base00"
        "base01"
        "base02"
        "base03"
        "base05"
        "base08"
        "base09"
        "base0A"
        "base0B"
        "base0C"
        "base0D"
        "base0E"
      ];
      extra = {
        border = if palette.extra.border != null then palette.extra.border else c.base02;
        accent2 = if palette.extra.accent2 != null then palette.extra.accent2 else c.base0F;
      };
      vals = (map (n: c.${n}) names) ++ [
        extra.border
        extra.accent2
      ];
      keys = map (n: "@${n}@") (
        names
        ++ [
          "border"
          "accent2"
        ]
      );
    in
    builtins.replaceStrings keys vals (builtins.readFile ../../assets/librewolf/userContent.css.in);

  # Sidebery's store from the Mac, with the sections written over its seven
  # panels (same ids and order, so tabs keep their panel) and containers.
  sideberyBase = builtins.fromJSON (builtins.readFile ../../assets/librewolf/sidebery.json);
  panelIds = lib.filter (id: sideberyBase.sidebar.panels ? ${id}) sideberyBase.sidebar.nav;
  sideberyData = lib.recursiveUpdate sideberyBase {
    sidebar.panels = lib.listToAttrs (
      lib.imap0 (i: s: {
        name = lib.elemAt panelIds i;
        value = sideberyBase.sidebar.panels.${lib.elemAt panelIds i} // {
          inherit (s) name;
          newTabCtx = ctx i;
          moveRules = [
            {
              id = "section-${toString i}";
              active = true;
              containerId = ctx i;
            }
          ]
          ++ lib.optional (i == 0) {
            id = "section-default";
            active = true;
            containerId = "firefox-default";
          };
        };
      }) sections
    );
    containers = lib.listToAttrs (
      lib.imap0 (i: s: {
        name = ctx i;
        value =
          sideberyBase.containers.${ctx i}
          // {
            inherit (s) name icon color;
            reopenRulesActive = s.sites != [ ];
            reopenRules = lib.imap0 (j: re: {
              id = "section-${toString i}-${toString j}";
              active = true;
              type = 1; # include: open matching pages in this container
              url = "/${re}/";
            }) s.sites;
          }
          // lib.optionalAttrs (s ? proxy) {
            proxified = true;
            inherit (s) proxy;
          };
      }) sections
    );
  };
  sideberyCSS =
    c:
    builtins.replaceStrings [ "@base0D@" ] [ c.base0D ] (
      builtins.readFile ../../assets/librewolf/sidebery.css.in
    );

  # The seven sections, in panel order. Each is a Sidebery panel with its own
  # container (ids 6..12). `sites` are regexes (Sidebery's /.../ form, without
  # the slashes): a page matching one reopens in that section's container, and
  # every tab in a container moves to its section's panel. Personal also takes
  # tabs in no container. Keep the sections' patterns disjoint: Sidebery checks
  # containers in id order (as sorted strings), so overlaps would pick by that.
  host = d: "^https?://([0-9A-Za-z-]+\\.)*${lib.escapeRegex d}(:[0-9]+)?([/?#]|$)";
  sections = [
    {
      name = "Personal";
      color = "red";
      icon = "fingerprint";
      sites = [ ];
    }
    {
      name = "Media";
      color = "orange";
      icon = "chill";
      sites =
        map host [
          "twitch.tv"
          "kick.com"
          "youtube.com"
          "youtu.be"
          "netflix.com"
          "disneyplus.com"
          "max.com"
          "hbomax.com"
          "primevideo.com"
          "hulu.com"
          "paramountplus.com"
          "crunchyroll.com"
          "plex.tv"
          "spotify.com"
          "music.apple.com"
          "tv.apple.com"
          "soundcloud.com"
        ]
        # Jellyfin wherever it's hosted (jellyfin.vainnor.com etc.).
        ++ [ "^https?://jellyfin\\.[^/?#]+([/?#]|$)" ];
    }
    {
      name = "VATSIM";
      color = "yellow";
      icon = "vacation";
      sites = map host [
        "vatsim.net"
        "vzdc.org"
        "vatusa.net"
      ];
    }
    {
      name = "Learning";
      color = "green";
      icon = "tree";
      sites = map host [
        "khanacademy.org"
        "coursera.org"
        "edx.org"
        "brilliant.org"
        "duolingo.com"
        "udemy.com"
        "codecademy.com"
        "freecodecamp.org"
        "ocw.mit.edu"
        "wolframalpha.com"
        "quizlet.com"
      ];
    }
    {
      name = "Homelab";
      color = "blue";
      icon = "fence";
      sites = [
        # Your domains, except Jellyfin (Media).
        "^https?://(?!jellyfin\\.)([0-9A-Za-z-]+\\.)*(vainnor|carsonberget)\\.com(:[0-9]+)?([/?#]|$)"
        (host "netbird.cloud")
        (host "app.netbird.io")
        # NetBird peers (100.64.0.0/10) and LAN addresses.
        "^https?://100\\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\\.[0-9]+\\.[0-9]+(:[0-9]+)?([/?#]|$)"
        "^https?://(10|192\\.168|172\\.(1[6-9]|2[0-9]|3[01]))\\.[0-9.]+(:[0-9]+)?([/?#]|$)"
      ];
    }
    {
      name = "Dev";
      color = "purple";
      icon = "briefcase";
      sites =
        map host [
          "github.com"
          "githubusercontent.com"
          "gitlab.com"
          "codeberg.org"
          "nixos.org"
          "nix.dev"
          "noogle.dev"
          "crates.io"
          "docs.rs"
          "rust-lang.org"
          "npmjs.com"
          "stackoverflow.com"
          "developer.mozilla.org"
          "hub.docker.com"
          "claude.ai"
          "t3.codes"
        ]
        # Anything on this machine, except the i2pd console (I2P).
        ++ [ "^https?://(localhost|127\\.0\\.0\\.1|\\[::1\\])(?!:7070([/?#]|$))(:[0-9]+)?([/?#]|$)" ];
    }
    {
      name = "I2P";
      color = "pink";
      icon = "circle";
      sites = [
        "^https?://[^/?#]+\\.i2p(:[0-9]+)?([/?#]|$)"
        "^https?://(localhost|127\\.0\\.0\\.1):7070([/?#]|$)"
      ];
      # Through the local i2pd HTTP proxy (privacy/i2p.nix).
      proxy = {
        type = "http";
        host = "127.0.0.1";
        port = "4444";
        username = "";
        password = "";
        proxyDNS = false;
      };
    }
  ];
  ctx = i: "firefox-container-${toString (6 + i)}";

  # The Firefox containers the sections open tabs in, at the Mac's ids.
  containers = lib.listToAttrs (
    lib.imap0 (i: s: {
      inherit (s) name;
      value = {
        id = 6 + i;
        inherit (s) color icon;
      };
    }) sections
  );

  amo = slug: {
    install_url = "https://addons.mozilla.org/firefox/downloads/latest/${slug}/latest.xpi";
    installation_mode = "force_installed";
  };
in
{
  flake.modules.homeManager.desktop =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      sideberySettings = sideberyData // {
        sidebarCSS = sideberyCSS config.lib.stylix.colors;
      };
      # The "v2" re-applies once, for the reset that also restores storage.js.
      sideberyHash = builtins.hashString "sha256" ("v2" + builtins.toJSON sideberySettings);
      sideberyStorage = pkgs.writeText "sidebery-storage.js" (builtins.toJSON sideberySettings);
      sideberyApply = pkgs.writeShellScript "sidebery-apply" ''
        for prof in ${lib.concatStringsSep " " (lib.attrNames config.programs.librewolf.profiles)}; do
          dir="${config.home.homeDirectory}/${config.programs.librewolf.profilesPath}/$prof"
          stamp="$dir/.sidebery-applied"
          [ -f "$dir/prefs.js" ] || continue
          [ "$(cat "$stamp" 2>/dev/null)" = "${sideberyHash}" ] && continue
          if ${pkgs.procps}/bin/pgrep -u "$(id -u)" -f '/bin/\.?librewolf' >/dev/null; then
            echo "LibreWolf is open: Sidebery's new setup goes in at the next login or switch with it closed" >&2
            continue
          fi
          uuid=$(${pkgs.gnused}/bin/sed -n 's/.*\\"{3c078156-979c-498b-8990-85f7987dd929}\\":\\"\([0-9a-f-]*\)\\".*/\1/p' "$dir/prefs.js")
          if [ -n "$uuid" ]; then
            rm -rf "$dir/storage/default/moz-extension+++$uuid^userContextId=4294967295"
          fi
          # LibreWolf rewrites storage.js with its live state when it closes,
          # so put the Nix one back, and drop the "already migrated" mark it
          # saved in prefs.js, so the next start loads it into the empty store.
          data="$dir/browser-extension-data/${sidebery}"
          mkdir -p "$data"
          rm -f "$data/storage.js"
          install -m644 ${sideberyStorage} "$data/storage.js"
          ${pkgs.gnused}/bin/sed -i '/ExtensionStorageIDB\.migrated\.{3c078156-979c-498b-8990-85f7987dd929}/d' "$dir/prefs.js"
          printf "%s\n" ${sideberyHash} > "$stamp"
        done
      '';
      look = {
        inherit userChrome;
        userContent = userContent config.lib.stylix.colors;
        inherit containers;
        containersForce = true;
        extensions = {
          force = true; # storage.js below is ours
          settings.${sidebery}.settings = sideberySettings;
        };
      };
    in
    {
      programs.librewolf = {
        enable = true;
        settings = {
          "privacy.resistFingerprinting" = true;
          "privacy.clearOnShutdown.history" = false;
          "privacy.clearOnShutdown.downloads" = false;
          # Keep cookies and logins across restarts, as on the Mac (i2p clears).
          "privacy.sanitize.sanitizeOnShutdown" = false;
          # Load the Nix-written Sidebery setup into an emptied store.
          "extensions.webextensions.ExtensionStorageIDB.migrated.${sidebery}" = false;
          "webgl.disabled" = false;
          "media.ffmpeg.vaapi.enabled" = true; # hardware video decode (any VA-API driver, see system/gpu.nix)

          # Load chrome/userChrome.css and userContent.css.
          "toolkit.legacyUserProfileCustomizations.stylesheets" = true;
          # New sidebar without the icon strip, opened on Sidebery.
          "sidebar.revamp" = true;
          "sidebar.visibility" = "hide-launcher";
          "sidebar.new-sidebar.has-used" = true;
          "sidebar.main.tools" = "history,${sidebery},${stylus}";
          "sidebar.backupState" = builtins.toJSON {
            command = "_3c078156-979c-498b-8990-85f7987dd929_-sidebar-action";
            panelOpen = true;
            panelWidth = 388;
          };
        };
        policies.ExtensionSettings = {
          ${sidebery} = amo "sidebery";
          ${stylus} = amo "styl-us";
        };
        profiles = {
          default = look // {
            id = 0;
            isDefault = true;
          };
          i2p = look // {
            id = 1;
            settings = {
              "network.proxy.type" = 1;
              "network.proxy.http" = "127.0.0.1";
              "network.proxy.http_port" = 4444;
              "network.proxy.ssl" = "127.0.0.1";
              "network.proxy.ssl_port" = 4444;
              "network.proxy.no_proxies_on" = "";
              "network.proxy.allow_hijacking_localhost" = true;
              "media.peerconnection.enabled" = false; # no WebRTC IP leaks
              "browser.startup.homepage" = "http://127.0.0.1:7070";
              "privacy.sanitize.sanitizeOnShutdown" = true;
            };
          };
        };
      };

      # Empty Sidebery's store when its setup changed, so it loads the new one.
      # A profile LibreWolf hasn't created yet loads it on its own. Runs on
      # each switch and at login, so one skipped while LibreWolf was open
      # goes in at the next login.
      home.activation.sideberyReset = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        run ${sideberyApply}
      '';
      systemd.user.services.sidebery-apply = {
        Unit.Description = "Load the Nix-written Sidebery setup into LibreWolf";
        Service = {
          Type = "oneshot";
          ExecStart = "${sideberyApply}";
        };
        Install.WantedBy = [ "default.target" ];
      };

      wayland.windowManager.mango.settings.bind = [ "SUPER+SHIFT,b,spawn,librewolf -P i2p --no-remote" ];
    };

  # The Mac keeps its own LibreWolf install and profile (Home Manager leaves
  # profiles.ini alone); only the two stylesheets come from here.
  # macLibrewolfProfile = null: leave the Mac's LibreWolf alone.
  flake.modules.darwin.base.home-manager.sharedModules =
    lib.optional (meta.macLibrewolfProfile or null != null)
      (
        { config, ... }:
        let
          chrome = "Library/Application Support/librewolf/Profiles/${meta.macLibrewolfProfile}/chrome";
        in
        {
          home.file."${chrome}/userChrome.css".text = userChrome;
          home.file."${chrome}/userContent.css".text = userContent config.lib.stylix.colors;
        }
      );
}
