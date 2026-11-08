# Per-host settings: what differs between machines that run the same aspects.
# flake/hosts.nix fills these from modules/hosts/<name>/host.json; anything
# deeper (monitor layouts, fans, GPU passthrough) goes in that host's own
# .nix files. Home-manager modules read them as `osConfig.vx.host.<name>`.
{
  flake.modules.nixos.base =
    { lib, ... }:
    let
      inherit (lib) mkOption types;
    in
    {
      options.vx.host = {
        name = mkOption {
          type = types.str;
          description = "The host's folder name under modules/hosts (also its hostname).";
        };
        aspects = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Aspects (flake.modules.nixos.*) this host was built from.";
        };
        gpu = mkOption {
          type = types.enum [
            "amd"
            "nvidia"
            "intel"
            "none"
          ];
          default = "none";
          description = "The graphics card's vendor; picks the driver (system/gpu.nix).";
        };
        primaryMonitor = mkOption {
          type = types.nullOr types.str;
          default = null;
          example = "VG34VQL3A";
          description = ''
            EDID model of the main screen (pop-ups, the OSD, the light Windows
            VM). `wlr-randr` shows it. null = the first screen.
          '';
        };
        audioOutput = mkOption {
          type = types.nullOr types.str;
          default = null;
          example = "alsa_output.usb-....analog-stereo";
          description = ''
            PipeWire name of the real output (`vx audio devices` lists them).
            null = the virtual channels exist but System isn't the default.
          '';
        };
        replayMonitor = mkOption {
          type = types.str;
          default = "screen";
          description = ''
            Screen the instant replay records. "screen" = the first one found;
            `gpu-screen-recorder --list-monitors` shows names like "DP-1".
          '';
        };
        vfio = mkOption {
          type = types.nullOr (types.attrsOf types.anything);
          default = null;
          description = ''
            GPU passthrough to the Windows VM (virtualisation/vm.nix). Set in a
            host's own file (see hosts/arc/vfio.nix); null = libvirt and the
            light VM only.
          '';
        };
      };
    };
}
