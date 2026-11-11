# Screen sharing (Vesktop, browsers, OBS's PipeWire source) goes through
# xdg-desktop-portal-wlr, which Mango's portal config picks for ScreenCast.
# NixOS starts the portal with its own --config file, so the settings go there
# (a ~/.config one is never read).
#
# Vesktop asks the portal more than once per share, so a picker would pop up
# each time. Instead the portal shares the main monitor (host.json
# primaryMonitor, found by model, so a changed port doesn't matter) without
# asking. With no primaryMonitor, or that monitor unplugged, you click the
# monitor to share (slurp, in the palette).
{
  flake.modules.nixos.desktop =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      c = config.lib.stylix.colors;
      primary = config.vx.host.primaryMonitor;
      slurp = "${lib.getExe pkgs.slurp} -f %o -or -b '#${c.base00}88' -c '#${c.base0D}ff' -w 2";
      chooser = pkgs.writeShellScript "screenshare-output" ''
        ${lib.optionalString (primary != null) ''
          out=$(${lib.getExe pkgs.wlr-randr} --json 2>/dev/null \
            | ${lib.getExe pkgs.jq} -r --arg m ${lib.escapeShellArg primary} \
              'first(.[] | select(.enabled and ((.model // "") | contains($m))) | .name) // empty')
          if [ -n "$out" ]; then echo "$out"; exit 0; fi
        ''}
        exec ${slurp}
      '';
    in
    {
      xdg.portal.wlr.settings.screencast = {
        chooser_type = "simple";
        chooser_cmd = "${chooser}";
        max_fps = 60;
      };
    };
}
