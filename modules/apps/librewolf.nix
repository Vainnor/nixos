# LibreWolf. Two profiles: `default` for normal browsing and `i2p`, which sends
# everything through the local i2pd HTTP proxy so .i2p traffic never mixes with
# your normal session. Launch it with `librewolf -P i2p` (or Super+Shift+B).
#
# Same look as the Mac: tabs live in Sidebery (vertical, in the sidebar), the
# tab strip and sidebar header are hidden, every corner is square, and the
# about: pages use the system palette (assets/librewolf).
#
# Sidebery's own setup comes from the Mac too (assets/librewolf/sidebery.json:
# settings, the seven colour panels and their containers; sidebery.css.in: its
# style, accent from the theme). LibreWolf only loads it into an empty Sidebery,
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

  sideberyData = builtins.fromJSON (builtins.readFile ../../assets/librewolf/sidebery.json);
  sideberyCSS =
    c:
    builtins.replaceStrings [ "@base0D@" ] [ c.base0D ] (
      builtins.readFile ../../assets/librewolf/sidebery.css.in
    );

  # The containers Sidebery's panels open tabs in (newTabCtx), at the Mac's ids.
  containers = lib.listToAttrs (
    lib.imap0
      (i: name: {
        inherit name;
        value = {
          id = 6 + i;
          color = lib.toLower name;
          icon = "circle";
        };
      })
      [
        "Red"
        "Orange"
        "Yellow"
        "Green"
        "Blue"
        "Purple"
        "Pink"
      ]
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
      sideberyHash = builtins.hashString "sha256" (builtins.toJSON sideberySettings);
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
      # A profile LibreWolf hasn't created yet loads it on its own.
      home.activation.sideberyReset = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        for prof in ${lib.concatStringsSep " " (lib.attrNames config.programs.librewolf.profiles)}; do
          dir="${config.home.homeDirectory}/${config.programs.librewolf.profilesPath}/$prof"
          stamp="$dir/.sidebery-applied"
          [ -f "$dir/prefs.js" ] || continue
          [ "$(cat "$stamp" 2>/dev/null)" = "${sideberyHash}" ] && continue
          if ${pkgs.procps}/bin/pgrep -u "$(id -u)" -f '/bin/\.?librewolf' >/dev/null; then
            echo "LibreWolf is open: Sidebery's new setup goes in on the next switch with it closed" >&2
            continue
          fi
          uuid=$(sed -n 's/.*\\"{3c078156-979c-498b-8990-85f7987dd929}\\":\\"\([0-9a-f-]*\)\\".*/\1/p' "$dir/prefs.js")
          if [ -n "$uuid" ]; then
            run rm -rf "$dir/storage/default/moz-extension+++$uuid^userContextId=4294967295"
          fi
          run sh -c 'printf "%s\n" "$1" > "$2"' sh ${sideberyHash} "$stamp"
        done
      '';

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
