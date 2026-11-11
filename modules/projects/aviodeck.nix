# aviodeck's dev shell: `use flake $HOME/nixos#aviodeck` in
# ~/Programing/aviodeck/.envrc (dev/projects.nix writes it and keeps it out of
# git through .git/info/exclude), so nothing lands in aviodeck's repo.
#
# Laravel 12 on PHP 8.4 with the extensions composer.json and Laravel use,
# Composer, Node for Vite, and Chrome's driver for Dusk. MySQL and Redis
# (.env: 127.0.0.1) run in docker; the tests use sqlite.
{
  perSystem =
    { pkgs, lib, ... }:
    let
      php = pkgs.php84.buildEnv {
        extensions =
          { enabled, all }:
          lib.unique (
            enabled
            ++ (with all; [
              zip
              intl
              bcmath
              gd
              pdo_mysql
              pdo_sqlite
              redis
            ])
          );
        extraConfig = "memory_limit = 1G";
      };
    in
    lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
      devShells.aviodeck = pkgs.mkShell {
        packages = [
          php
          php.packages.composer
          pkgs.nodejs_22
          pkgs.chromedriver
          pkgs.mariadb.client # mysql
          pkgs.redis # redis-cli
        ];
      };
    };
}
