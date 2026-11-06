# Rootless Docker: runs as your user, so the docker socket is not a root shell.
# `docker`/`docker compose` work as usual.
{
  flake.modules.nixos.dev = {
    virtualisation.docker.rootless = {
      enable = true;
      setSocketVariable = true;
    };
  };
}
