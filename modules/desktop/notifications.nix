{
  flake.modules.homeManager.desktop = {
    services.mako = {
      enable = true;
      settings = {
        default-timeout = 5000;
        border-radius = 8;
        anchor = "top-right";
      };
    };
  };
}
