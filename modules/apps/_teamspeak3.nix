# TeamSpeak 3 (the classic client). nixpkgs dropped it in 2026 because it was
# built against nixpkgs' Qt 5 WebEngine, which is EOL and gone. The official
# tarball ships its own Qt 5 (WebEngine included), so this keeps those and runs
# the client in a small FHS environment that provides the rest of the system
# libraries. X11 only (TS3 never worked on Wayland): it runs under XWayland.
#
# identityFile: a file (a sops secret) with a TS3 identity. Each launch puts it
# into ~/.ts3client/settings.db as the default identity, if it isn't there yet
# (see _ts3-identity.py). nickname: used when the file doesn't name one.
{
  lib,
  stdenvNoCC,
  fetchurl,
  fetchzip,
  perl,
  buildFHSEnv,
  makeDesktopItem,
  xkeyboard_config,
  python3,
  writeShellScript,
  identityFile ? null,
  nickname ? "TeamSpeakUser",
}:
let
  version = "3.6.2";

  client = stdenvNoCC.mkDerivation {
    pname = "teamspeak3-client-unwrapped";
    inherit version;

    src = fetchurl {
      url = "https://files.teamspeak-services.com/releases/client/${version}/TeamSpeak3-Client-linux_amd64-${version}.run";
      hash = "sha256-WfEQQ4lxoj+QSnAOfdCoEc+Z1Oa5dbo6pFli1DsAZCI=";
    };
    # The logo, from the plugin SDK (the client tarball has no icon).
    sdk = fetchzip {
      url = "https://files.teamspeak-services.com/releases/sdk/3.3.1/ts_sdk_3.3.1.zip";
      hash = "sha256-wx4pBZHpFPoNvEe4xYE80KnXGVda9XcX35ho4R8QxrQ=";
    };

    nativeBuildInputs = [ perl ]; # the installer checks itself with shasum

    # The .run is a self-extracting script: accept the licence non-interactively.
    unpackPhase = ''
      echo -e '\ny' | PAGER=cat sh -e $src >/dev/null
    '';
    dontConfigure = true;
    dontBuild = true;
    dontPatchELF = true;
    dontStrip = true;

    installPhase = ''
      mkdir -p $out/opt
      mv TeamSpeak3-Client-linux_amd64 $out/opt/teamspeak3
      # The bundled libGL would shadow the real (driver) one.
      rm $out/opt/teamspeak3/libGL.so
      install -Dm644 $sdk/doc/_static/logo.png $out/share/icons/hicolor/64x64/apps/teamspeak3.png
    '';
  };

  desktopItem = makeDesktopItem {
    name = "teamspeak3";
    exec = "ts3client %u";
    icon = "teamspeak3";
    desktopName = "TeamSpeak 3";
    genericName = "Voice chat";
    comment = "The TeamSpeak 3 voice communication client";
    categories = [
      "Network"
      "Chat"
    ];
    mimeTypes = [ "x-scheme-handler/ts3server" ];
  };
in
buildFHSEnv {
  pname = "ts3client";
  inherit version;

  targetPkgs =
    pkgs: with pkgs; [
      # Qt 5 (bundled) and its xcb platform plugin
      glib
      dbus
      fontconfig
      freetype
      zlib
      libGL
      libxkbcommon
      libx11
      libxext
      libxrender
      libxi
      libxfixes
      libxrandr
      libxcomposite
      libxdamage
      libxcursor
      libxtst
      libxscrnsaver
      libxshmfence
      libxcb
      libxkbfile
      libsm
      libice
      libxcb-util
      libxcb-image
      libxcb-keysyms
      libxcb-render-util
      libxcb-wm
      # WebEngine (Chromium)
      nss
      nspr
      expat
      alsa-lib
      libdrm
      libgbm
      libpulseaudio
      udev # Chromium aborts without libudev
      libevent
      lcms2
      pciutils
      libxml2_13 # its Qt wants libxml2.so.2
      libxslt
      # sound backends, file dialogs
      pipewire
      gtk3
    ];

  # TS3 can't do Wayland, and its Qt looks for xkb data at the FHS path.
  # Chromium's own sandbox can't start inside the FHS env (bubblewrap already
  # confines it) and WebEngine crashes without this.
  profile = ''
    export QT_QPA_PLATFORM=xcb
    export QT_XKB_CONFIG_ROOT=${xkeyboard_config}/share/X11/xkb
    export QTWEBENGINE_DISABLE_SANDBOX=1
    export QTWEBENGINE_LOCALES_PATH=${client}/opt/teamspeak3/qtwebengine_locales
  '';

  # The upstream run script points LD_LIBRARY_PATH and Qt at the install dir,
  # which is what the bundled Qt needs.
  runScript = writeShellScript "ts3client-run" ''
    ${lib.optionalString (identityFile != null) ''
      ${lib.getExe python3} ${./_ts3-identity.py} ${lib.escapeShellArg identityFile} \
        "$HOME/.ts3client/settings.db" ${lib.escapeShellArg nickname} || true
    ''}
    exec ${client}/opt/teamspeak3/ts3client_runscript.sh "$@"
  '';

  extraInstallCommands = ''
    mkdir -p $out/share/applications
    cp ${desktopItem}/share/applications/* $out/share/applications/
    cp -r ${client}/share/icons $out/share/
  '';

  meta = {
    description = "TeamSpeak 3 voice communication client";
    homepage = "https://teamspeak.com/";
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    license = lib.licenses.unfree;
    mainProgram = "ts3client";
    platforms = [ "x86_64-linux" ];
  };
}
