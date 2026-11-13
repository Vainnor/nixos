# Helium (helium.computer), the ungoogled-Chromium browser from imput.
# nixpkgs doesn't have it. The community flakes wrap the AppImage in
# bubblewrap, which sets no_new_privs, and that breaks 1Password's
# setgid BrowserSupport helper. So this patches the official tarball
# to run as a normal program, the way nixpkgs packages google-chrome.
#
# Update: bump `version`, then put the hash from
#   nix store prefetch-file https://github.com/imputnet/helium-linux/releases/download/<v>/helium-<v>-x86_64_linux.tar.xz
# in `hash`.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  wrapGAppsHook3,
  alsa-lib,
  at-spi2-atk,
  at-spi2-core,
  cairo,
  cups,
  dbus,
  expat,
  glib,
  gtk3,
  libdrm,
  libgbm,
  libGL,
  libpulseaudio,
  libva,
  libxkbcommon,
  libx11,
  libxcb,
  libxcomposite,
  libxdamage,
  libxext,
  libxfixes,
  libxrandr,
  libxshmfence,
  nspr,
  nss,
  pango,
  pipewire,
  systemd,
  vulkan-loader,
}:
let
  # Loaded with dlopen at runtime, so autoPatchelf can't see them.
  runtimeLibs = [
    libGL
    libva # hardware video decode
    libpulseaudio
    pipewire # screen sharing
    systemd # libudev
    vulkan-loader
    gtk3
  ];
in
stdenv.mkDerivation rec {
  pname = "helium";
  version = "0.21.2.1";

  src = fetchurl {
    url = "https://github.com/imputnet/helium-linux/releases/download/${version}/helium-${version}-x86_64_linux.tar.xz";
    hash = "sha256-5VwlPggvePta4JTKRlpZ4GiC/XyH6/ODFlvrNWDgtRk=";
  };

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
    wrapGAppsHook3
  ];

  buildInputs = [
    alsa-lib
    at-spi2-atk
    at-spi2-core
    cairo
    cups
    dbus
    expat
    glib
    gtk3
    libdrm
    libgbm
    libxkbcommon
    libx11
    libxcb
    libxcomposite
    libxdamage
    libxext
    libxfixes
    libxrandr
    libxshmfence
    nspr
    nss
    pango
  ];

  # The Qt shims are only for KDE theming and would pull in Qt.
  autoPatchelfIgnoreMissingDeps = [
    "libQt5*"
    "libQt6*"
  ];

  dontConfigure = true;
  dontBuild = true;
  dontWrapGApps = true; # one wrapper, below

  installPhase = ''
    runHook preInstall
    mkdir -p $out/opt/helium $out/bin
    cp -r . $out/opt/helium
    rm $out/opt/helium/libqt5_shim.so $out/opt/helium/libqt6_shim.so

    install -Dm644 product_logo_256.png $out/share/icons/hicolor/256x256/apps/helium.png
    install -Dm644 helium.desktop -t $out/share/applications
    runHook postInstall
  '';

  postFixup = ''
    makeWrapper $out/opt/helium/helium $out/bin/helium \
      "''${gappsWrapperArgs[@]}" \
      --prefix LD_LIBRARY_PATH : "$out/opt/helium:${lib.makeLibraryPath runtimeLibs}" \
      --set CHROME_WRAPPER $out/bin/helium \
      --set CHROME_VERSION_EXTRA nixos \
      --add-flags "\''${NIXOS_OZONE_WL:+\''${WAYLAND_DISPLAY:+--ozone-platform-hint=auto --enable-wayland-ime}}"
  '';

  meta = {
    description = "Private, fast and honest web browser based on ungoogled-chromium";
    homepage = "https://helium.computer";
    license = with lib.licenses; [
      gpl3Only
      bsd3
    ];
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    mainProgram = "helium";
    platforms = [ "x86_64-linux" ];
  };
}
