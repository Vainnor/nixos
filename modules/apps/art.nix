# Aseprite, the pixel-art editor. nixpkgs builds it from source (its EULA
# allows that for your own use, and you own a licence anyway). It's unfree and
# not redistributable, so there's no binary cache: the first build compiles
# Aseprite and its Skia fork locally, which takes a while.
#
# Extensions and scripts come pinned from GitHub:
#   - thkwznk's extensions: Magic Pencil (pencil modes), FX (drop shadow,
#     outline, neon and more), Sprite Analyzer (live silhouette/value
#     preview), NxPA Studio (pixel-art upscaling, in-betweens, colour
#     analysis), Animation Suite (import and loop animations) and the dozen
#     Micro Extensions (centre image, randomize brush, playback shortcuts...)
#   - Lospec Palette Importer: palettes from lospec.com by name or URL
#   - behreajj's AsepriteAddons: ~100 scripts under File > Scripts
#     (gradients, dithering, colour curves, LCh picker, Tiled export...)
#
# Aseprite writes each extension's settings (__pref.lua) into the extension's
# own folder, so they're copied in writable rather than linked from the store.
# Disabling or reordering them in Preferences > Extensions sticks; a removed
# extension comes back on the next switch (remove it here instead).
{
  flake.modules.nixos.desktop = {
    unfree.allowed = [ "aseprite" ];
  };
  flake.modules.homeManager.desktop =
    { pkgs, lib, ... }:
    let
      thkwznk = pkgs.fetchFromGitHub {
        owner = "thkwznk";
        repo = "aseprite-scripts";
        rev = "057354687ff5be989f8a377fbe14d51f375f3261";
        hash = "sha256-rksuBb9szjz5EOKnkdaGVUUJkB7TB3wpssMQF4u8IrE=";
      };
      addons = pkgs.fetchFromGitHub {
        owner = "behreajj";
        repo = "AsepriteAddons";
        rev = "eaac63b3229d28e7d6e1bc81de6471b592e5caef";
        hash = "sha256-D7JMYq3kiUHBPNaHFCYYsnPhjV+L5OGrf8AiLMkaXZ8=";
      };
      lospec = pkgs.fetchzip {
        name = "lospec-palette-importer";
        url = "https://github.com/JRiggles/Lospec-Palette-Importer/releases/download/v1.6.0/lospec-palette-importer.aseprite-extension";
        extension = "zip";
        hash = "sha256-K1IV07yY7Q0XqJ808ZeQCmoosgOS4xsalccJvMVNGu8=";
      };

      thkwznkExtensions = [
        "Magic Pencil"
        "FX"
        "Sprite Analyzer"
        "NxPA Studio"
        "Animation Suite"
      ];

      # One folder per extension, named like Aseprite would name it on install.
      extensions = pkgs.runCommand "aseprite-extensions" { nativeBuildInputs = [ pkgs.jq ]; } ''
        mkdir -p $out
        add() { cp -r "$1" "$out/$(jq -r .name "$1/package.json")"; }
        ${lib.concatMapStrings (e: "add ${lib.escapeShellArg "${thkwznk}/${e}"}\n") thkwznkExtensions}
        for e in "${thkwznk}/Micro Extensions"/*/; do add "$e"; done
        add ${lospec}
        chmod -R u+w $out
        # It shells out to /usr/bin/curl, which NixOS doesn't have.
        substituteInPlace $out/lospec-palette-importer/lospec-palette-importer.lua \
          --replace-fail '/usr/bin/curl' '${lib.getExe pkgs.curl}'
        rm -rf $out/lospec-palette-importer/{MacOSHelper,WindowsHelper}
      '';
    in
    {
      home.packages = [ pkgs.aseprite ];

      # Read-only is fine for plain scripts.
      xdg.configFile."aseprite/scripts/addons".source = pkgs.runCommand "aseprite-addons" { } ''
        mkdir -p $out
        cp -r ${addons}/{commands,dialogs,support} $out/
      '';

      home.activation.asepriteExtensions = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        dir="''${XDG_CONFIG_HOME:-$HOME/.config}/aseprite/extensions"
        run mkdir -p "$dir"
        # Drop extensions this config used to install but no longer does.
        for old in "$dir"/*/.nix-managed; do
          [ -e "$old" ] || continue
          name="$(basename "$(dirname "$old")")"
          [ -e "${extensions}/$name" ] || run rm -rf "$dir/$name"
        done
        for ext in ${extensions}/*; do
          name="$(basename "$ext")"
          run ${lib.getExe pkgs.rsync} -r --delete --exclude=__pref.lua --chmod=u+w,Du+x \
            "$ext/" "$dir/$name/"
          run touch "$dir/$name/.nix-managed"
        done
      '';
    };
}
