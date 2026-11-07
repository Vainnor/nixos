# Media viewers, and the default apps for opening files and links.
{
  flake.modules.homeManager.desktop =
    { config, lib, ... }:
    let
      c = config.lib.stylix.colors;
      # Every MIME type in `types` opens with `app` (a .desktop file name).
      openWith = app: types: lib.genAttrs types (_: [ app ]);
    in
    {
      programs.mpv.enable = true;
      programs.zathura.enable = true;

      # imv in the palette (Stylix has no imv target).
      programs.imv = {
        enable = true;
        settings.options = {
          background = c.base00;
          overlay_font = "${config.stylix.fonts.monospace.name}:11";
          overlay_text_color = c.base05;
          overlay_background_color = c.base01;
          overlay_background_alpha = "ff";
        };
      };

      xdg.mimeApps = {
        enable = true;
        defaultApplications =
          openWith "imv.desktop" [
            "image/png"
            "image/jpeg"
            "image/gif"
            "image/webp"
            "image/avif"
            "image/bmp"
            "image/tiff"
            "image/svg+xml"
            "image/x-icon"
          ]
          // openWith "org.pwmt.zathura-pdf-mupdf.desktop" [
            "application/pdf"
            "application/epub+zip"
          ]
          // openWith "org.pwmt.zathura-djvu.desktop" [ "image/vnd.djvu" ]
          // openWith "mpv.desktop" [
            "video/mp4"
            "video/x-matroska"
            "video/webm"
            "video/quicktime"
            "video/x-msvideo"
            "video/mpeg"
            "video/ogg"
            "audio/mpeg"
            "audio/flac"
            "audio/ogg"
            "audio/opus"
            "audio/wav"
            "audio/x-wav"
            "audio/aac"
            "audio/mp4"
          ]
          // openWith "thunar.desktop" [ "inode/directory" ]
          // openWith "librewolf.desktop" [
            "text/html"
            "application/xhtml+xml"
            "x-scheme-handler/http"
            "x-scheme-handler/https"
          ];
      };
    };
}
