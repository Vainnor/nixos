# vZDC's dev shells (github.com/vzdc-artcc): `use flake $HOME/nixos#vzdc-<name>`
# in each checkout under ~/Programing/vZDC (dev/projects.nix clones them and
# writes the .envrc), so nothing lands in their repos.
#
# website, ids and asx are Next.js + Prisma 7 on npm, at the Node their
# Dockerfiles use (website's master: 24; its next branch, ids and asx: 22).
# Prisma has no engine download for NixOS, so its schema engine (migrate, db
# push, studio) is nixpkgs' prisma-engines_7; Prisma 7's client needs no
# engine. Postgres runs in docker compose as in the repos; psql is here.
#
# osmium is Rust on stable (as its CI), with openssl for sqlx's native-tls,
# sqlx-cli and psql. Its tests use the database only with DATABASE_URL set.
#
# discord-bot is Python 3.13 (pyproject.toml) with uv: the shell makes .venv
# from requirements.txt the first time and activates it.
#
# moodle builds a Docker image; the shell has PHP 8.3 with Moodle's extensions
# for linting and editing, Composer, shellcheck and hadolint. Its test stack
# (test/compose.yaml) runs in the system's rootless Docker.
{
  perSystem =
    { pkgs, lib, ... }:
    let
      # Rootless Docker (dev/containers.nix) listens on the user's socket; set
      # it here too, so agent sessions that load the shell find it.
      dockerHost = ''
        export DOCKER_HOST="''${DOCKER_HOST:-unix://''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/docker.sock}"
      '';

      next =
        nodejs:
        pkgs.mkShell {
          packages = [
            nodejs
            pkgs.openssl
            pkgs.postgresql_17 # psql, pg_dump
          ];
          env.PRISMA_SCHEMA_ENGINE_BINARY = "${pkgs.prisma-engines_7}/bin/schema-engine";
          shellHook = dockerHost;
        };

      php = pkgs.php83.buildEnv {
        extensions =
          { enabled, all }:
          lib.unique (
            enabled
            ++ (with all; [
              pgsql
              pdo_pgsql
              redis
              intl
              zip
              gd
              soap
              sodium
              exif
              opcache
            ])
          );
        extraConfig = "memory_limit = 1G";
      };
    in
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      devShells = {
        vzdc-website = next pkgs.nodejs_24;
        vzdc-website-next = next pkgs.nodejs_22;
        vzdc-ids = next pkgs.nodejs_22;
        vzdc-asx = next pkgs.nodejs_22;

        vzdc-osmium = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            pkg-config
            openssl
            sqlx-cli
            postgresql_17
          ];
          shellHook = dockerHost;
        };

        vzdc-discord-bot = pkgs.mkShell {
          packages = [
            pkgs.python313
            pkgs.uv
          ];
          env.UV_PYTHON_DOWNLOADS = "never";
          shellHook = ''
            if [ ! -d .venv ] && [ -f requirements.txt ]; then
              uv venv -q --python ${lib.getExe pkgs.python313} .venv &&
                uv pip install -q --python .venv/bin/python -r requirements.txt ||
                echo "discord-bot: could not set up .venv" >&2
            fi
            [ -f .venv/bin/activate ] && . .venv/bin/activate
            ${dockerHost}
          '';
        };

        vzdc-moodle = pkgs.mkShell {
          packages = [
            php
            php.packages.composer
            pkgs.shellcheck
            pkgs.hadolint
            pkgs.postgresql_17
            pkgs.valkey # redis-cli
          ];
          shellHook = dockerHost;
        };
      };
    };
}
