# PipeWire. The Game/Chat/Music/Browser virtual outputs replace the BEACN app's
# channels and VB-Audio Cable: point apps at them in pwvucontrol and mix/route
# with qpwgraph. Mic cleanup goes through EasyEffects.
{
  flake.modules.nixos.desktop =
    { lib, pkgs, ... }:
    let
      channel = name: {
        name = "libpipewire-module-loopback";
        args = {
          "node.description" = name;
          "capture.props" = {
            "node.name" = "channel_${lib.toLower name}";
            "media.class" = "Audio/Sink";
            "audio.position" = [
              "FL"
              "FR"
            ];
          };
          "playback.props" = {
            "node.name" = "channel_${lib.toLower name}_out";
            "node.passive" = true;
            "audio.position" = [
              "FL"
              "FR"
            ];
          };
        };
      };
    in
    {
      services.pulseaudio.enable = false;
      security.rtkit.enable = true;
      services.pipewire = {
        enable = true;
        alsa.enable = true;
        alsa.support32Bit = true;
        pulse.enable = true;
        wireplumber.enable = true;
        extraConfig.pipewire."60-virtual-channels"."context.modules" = map channel [
          "Game"
          "Chat"
          "Music"
          "Browser"
        ];
      };

      # HyperX Cloud Alpha Wireless battery/sidetone (`headsetcontrol -b`).
      services.udev.packages = [ pkgs.headsetcontrol ];
      environment.systemPackages = [ pkgs.headsetcontrol ];
    };

  flake.modules.homeManager.desktop =
    { pkgs, ... }:
    {
      services.easyeffects.enable = true;
      home.packages = with pkgs; [
        pwvucontrol
        qpwgraph
        playerctl
      ];
    };
}
