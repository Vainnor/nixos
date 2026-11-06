# Mic chain in EasyEffects, loaded at login so the mic sounds the same every
# time: noise removal (RNNoise) -> gate (silence between words) -> compressor
# (even level) -> limiter (no clipping). Apps record the cleaned mic from
# "Easy Effects Source"; OBS's Mic source already uses it.
# Tune by ear in EasyEffects, then save over the "Vainnor Mic" preset.
{
  flake.modules.homeManager.streaming = {
    services.easyeffects = {
      preset = "Vainnor Mic";
      extraPresets."Vainnor Mic".input = {
        blocklist = [ ];
        plugins_order = [
          "rnnoise#0"
          "gate#0"
          "compressor#0"
          "limiter#0"
        ];
        "rnnoise#0" = {
          bypass = false;
          input-gain = 0.0;
          output-gain = 0.0;
          enable-vad = true;
          vad-thres = 50.0;
          wet = 0.0;
          release = 20.0;
          use-standard-model = true;
        };
        "gate#0" = {
          bypass = false;
          input-gain = 0.0;
          output-gain = 0.0;
          attack = 5.0;
          release = 250.0;
          curve-threshold = -45.0;
          curve-zone = -6.0;
          reduction = -30.0;
          makeup = 0.0;
        };
        "compressor#0" = {
          bypass = false;
          input-gain = 0.0;
          output-gain = 0.0;
          attack = 10.0;
          release = 120.0;
          threshold = -20.0;
          ratio = 3.0;
          knee = -6.0;
          makeup = 4.0;
        };
        "limiter#0" = {
          bypass = false;
          input-gain = 0.0;
          output-gain = 0.0;
          lookahead = 5.0;
          attack = 5.0;
          release = 20.0;
          threshold = -1.5;
        };
      };
    };
  };
}
