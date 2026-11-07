# The user account (NixOS half) and the base home-manager config (shell, CLI),
# shared with the Mac. The fish setup carries over your CachyOS-style config:
# fastfetch greeting, eza aliases, bat for man pages, !! / !$, atuin history.
{ config, ... }:
let
  inherit (config) meta;
in
{
  flake.modules.nixos.base =
    { pkgs, ... }:
    {
      programs.fish.enable = true;

      users.users.${meta.username} = {
        isNormalUser = true;
        description = meta.fullName;
        shell = pkgs.fish;
        extraGroups = [
          "wheel"
          "networkmanager"
          "video"
          "audio"
        ];
        # Password hash lives in /persistent/passwords (see system/impermanence.nix).
      };
    };

  flake.modules.homeManager.base =
    { pkgs, lib, ... }:
    let
      inherit (pkgs.stdenv.hostPlatform) isDarwin;
      eza = "eza --color=always --group-directories-first --icons=always";
    in
    {
      home.username = meta.username;
      home.homeDirectory = if isDarwin then "/Users/${meta.username}" else "/home/${meta.username}";

      programs = {
        home-manager.enable = true;
        fish = {
          enable = true;
          plugins = [
            {
              name = "bang-bang"; # !! and !$
              inherit (pkgs.fishPlugins.bang-bang) src;
            }
          ]
          # On the Mac, done.fish already sits in ~/.config/fish/conf.d.
          ++ lib.optional (!isDarwin) {
            name = "done"; # notification when a long command finishes
            inherit (pkgs.fishPlugins.done) src;
          };
          shellInit = lib.optionalString isDarwin ''
            # Nix before Homebrew, so the shared tools win.
            fish_add_path --prepend --move /etc/profiles/per-user/${meta.username}/bin /run/current-system/sw/bin /nix/var/nix/profiles/default/bin
          '';
          interactiveShellInit = ''
            function fish_greeting
                fastfetch
            end
            set -g __done_min_cmd_duration 10000
            set -g __done_notification_urgency_level low
            set -x MANROFFOPT -c
            set -x MANPAGER "sh -c 'col -bx | bat -l man -p'"
            fish_add_path ~/.local/bin ~/.cargo/bin

            function history
                builtin history --show-time='%F %T '
            end
            function backup --argument filename
                cp $filename $filename.bak
            end
          ''
          + lib.optionalString isDarwin ''
            test -f ~/.fish_profile; and source ~/.fish_profile
            fish_add_path ~/Applications/depot_tools
            source ~/.orbstack/shell/init2.fish 2>/dev/null; or true
          '';
          shellAliases = {
            ls = "${eza} -al";
            la = "${eza} -a";
            ll = "${eza} -l";
            lt = "${eza} -aT";
            ".." = "cd ..";
            "..." = "cd ../..";
            "...." = "cd ../../..";
            tarnow = "tar -acf ";
            untar = "tar -zxvf ";
            psmem = "ps auxf | sort -nr -k 4";
            psmem10 = "ps auxf | sort -nr -k 4 | head -10";
          }
          // lib.optionalAttrs (!isDarwin) {
            jctl = "journalctl -p 3 -xb"; # this boot's errors
          };
        };
        starship.enable = true; # no settings here, so your Mac's starship.toml stays yours
        atuin.enable = true; # searchable shell history (Ctrl+R / Up)
        zoxide.enable = true;
        eza = {
          enable = true;
          enableFishIntegration = false; # aliases above instead
        };
        bat.enable = true;
        fzf = {
          enable = true;
          historyWidget.command = ""; # Ctrl-R belongs to atuin
        };
        ripgrep.enable = true;
        fd.enable = true;
        btop.enable = true;
        yazi.enable = true;
      };

      home.packages = with pkgs; [
        unzip
        p7zip
        wget
        jq
        fastfetch
        nvd # what changed between two system generations (`vx diff`)
        nix-tree # what's taking space in a closure (`vx tree`)
      ];
    };
}
