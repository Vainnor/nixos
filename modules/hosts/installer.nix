# Custom installer ISO: the minimal NixOS ISO that starts `vx install` by
# itself, full screen (cage + foot, Vainnor slate colours), with everything
# the installer shells out to, and this flake plus all its inputs inside, so
# working out the system needs no download of nixpkgs.
#
#   Build (any x86_64 Linux, WSL works, or the GitHub Action): `vx iso`, or
#     nix build .#nixosConfigurations.installer.config.system.build.isoImage
#   Write it to a USB stick: `vx iso --flash /dev/sdX`.
#
# Boot it (UEFI). The installer opens on its own; plug in Ethernet first, or
# quit it and run `nmtui` for Wi-Fi, then `vx-installer` to start it again.
# Without working graphics it runs straight on the console instead.
{
  inputs,
  self,
  ...
}:
let
  slate = (import ../../assets/themes/vainnor-slate.nix).scheme;

  # Every flake input's source, nested inputs too (deduplicated by store path).
  inputSources =
    let
      go = input: [ input.outPath ] ++ builtins.concatMap go (builtins.attrValues (input.inputs or { }));
    in
    inputs.nixpkgs.lib.unique (
      builtins.concatMap go (builtins.attrValues (removeAttrs inputs [ "self" ]))
    );
in
{
  flake.nixosConfigurations.installer = inputs.nixpkgs.lib.nixosSystem {
    modules = [
      "${inputs.nixpkgs}/nixos/modules/installer/cd-dvd/installation-cd-minimal.nix"
      (
        { pkgs, lib, ... }:
        let
          system = "x86_64-linux";
          installer = self.packages.${system}.vx-installer;

          footIni = pkgs.writeText "vx-installer-foot.ini" ''
            [main]
            font=JetBrains Mono:size=13
            pad=16x12
            initial-color-theme=dark

            [cursor]
            style=block

            [colors-dark]
            foreground=${slate.base05}
            background=${slate.base00}
            selection-foreground=${slate.base00}
            selection-background=${slate.base0D}
            cursor=${slate.base00} ${slate.base0D}
            regular0=${slate.base00}
            regular1=${slate.base08}
            regular2=${slate.base0B}
            regular3=${slate.base0A}
            regular4=${slate.base0D}
            regular5=${slate.base0E}
            regular6=${slate.base0C}
            regular7=${slate.base05}
            bright0=${slate.base03}
            bright1=${slate.base08}
            bright2=${slate.base0B}
            bright3=${slate.base0A}
            bright4=${slate.base0D}
            bright5=${slate.base0E}
            bright6=${slate.base0C}
            bright7=${slate.base07}
          '';

          # Full screen in cage + foot; on the console when there's no
          # graphics or cage gives up before the installer even started.
          session = pkgs.writeShellApplication {
            name = "vx-installer-session";
            runtimeInputs = [
              pkgs.cage
              pkgs.foot
              pkgs.coreutils
            ];
            text = ''
              started="''${XDG_RUNTIME_DIR:-/tmp}/vx-installer.started"
              rm -f "$started"
              if compgen -G '/dev/dri/card*' >/dev/null; then
                # pixman: software rendering, needs no GPU driver beyond DRM.
                WLR_RENDERER=pixman cage -s -- \
                  foot --config=${footIni} --title=vx-installer \
                  sh -c "touch '$started'; exec ${lib.getExe installer}" \
                  2>/tmp/vx-installer-cage.log || true
              fi
              if [ ! -e "$started" ]; then
                echo "no full-screen window (see /tmp/vx-installer-cage.log); starting the installer here"
                ${lib.getExe installer} || true
              fi
              rm -f "$started"
            '';
          };
        in
        {
          nixpkgs.hostPlatform = system;
          nix.settings.experimental-features = [
            "nix-command"
            "flakes"
          ];
          boot.kernelPackages = pkgs.linuxPackages_latest; # Zen 5 / RDNA3 / MT7922 support
          boot.supportedFilesystems.zfs = lib.mkForce false; # ZFS lags the latest kernel; not needed

          environment.systemPackages = [
            installer # vx-installer: `vx install --data <install-data>`, as root
            session
            self.packages.${system}.vx
            inputs.disko.packages.${system}.disko
          ]
          ++ (with pkgs; [
            nixos-install-tools
            cryptsetup
            btrfs-progs
            util-linux
            gptfdisk
            dosfstools
            exfatprogs
            ntfs3g # ntfsfix / ntfslabel for the Windows data drives
            e2fsprogs
            rsync
            git
            gh # GitHub login (device code), forks and pushes for new machines
            curl # GitHub device login and SSH keys
            mkpasswd
            openssh
            ssh-to-age
            jq
            pciutils # lspci: the installer detects the GPU
            coreutils
          ]);
          # Also for `vx install` run by hand (sudo drops it, so vx-installer passes --data itself).
          environment.variables.VX_INSTALL_DATA = "${self.packages.${system}.install-data}";

          fonts.fontconfig.enable = lib.mkForce true;
          fonts.packages = [ pkgs.jetbrains-mono ];

          # The flake inputs' sources, so evaluating arc needs no download.
          # The channel copy of nixpkgs isn't needed (installs use the flake).
          system.extraDependencies = inputSources ++ [ self.outPath ];
          system.installer.channel.enable = false;

          # Start the installer once per boot on tty1, after the ISO's autologin.
          programs.bash.loginShellInit = ''
            if [ "$(tty)" = /dev/tty1 ] && [ "$(id -un)" = nixos ] && [ ! -e /tmp/.vx-installer-autostarted ]; then
              touch /tmp/.vx-installer-autostarted
              vx-installer-session
              echo
              echo "installer closed. vx-installer starts it again; nmtui connects wi-fi."
              echo
            fi
          '';

          image.baseName = lib.mkForce "nixos-arc-installer";
          services.getty.helpLine = ''

            ===  arc installer  ===
            The installer starts by itself on tty1.
            Wi-Fi: nmtui.  Start the installer again: vx-installer
          '';
        }
      )
    ];
  };
}
