# Hosts from folders: every modules/hosts/<name>/ with a host.json becomes
# nixosConfigurations.<name>. Nothing else needs editing to add a machine;
# `vx install` (new machine) writes the folder, commits and pushes it.
#
#   modules/hosts/<name>/host.json      name, system, stateVersion, aspects, gpu,
#                                       primaryMonitor, audioOutput, replayMonitor
#   modules/hosts/<name>/_hardware.nix  nixos-generate-config --no-filesystems output
#   modules/hosts/<name>/disks.json     the drives (hosts/_disko.nix; optional)
#   modules/hosts/<name>/*.nix          anything else for this machine only, as
#                                       `flake.modules.nixos.<name>` (included
#                                       automatically, never on another host)
#
# Each host gets: base, its aspects, the shared disk layout, its hardware file,
# flake.modules.nixos.<name> if any, hostname = folder name, and its
# stateVersion for both NixOS and home-manager. The per-host settings land in
# `vx.host.*` (system/host.nix).
{
  inputs,
  config,
  lib,
  ...
}:
let
  hostsDir = ../hosts;
  user = config.meta.username;
  nixos = config.flake.modules.nixos;

  # Shown by the installer when you pick a new machine's features. `requires`:
  # aspects that only make sense (or only evaluate) together with another.
  aspectInfo = {
    desktop = {
      description = "the Mango desktop with its shell, apps, audio channels and theme";
      default = true;
    };
    dev = {
      description = "git with signing, Zed, vis, direnv, rootless Docker, nix-ld";
      default = true;
      requires = [ "desktop" ];
    };
    privacy = {
      description = "the I2P router (i2pd) behind LibreWolf's i2p profile";
      default = true;
    };
    gaming = {
      description = "Steam with Proton, GameMode, MangoHud, controllers, Sunshine, instant replay";
      default = false;
      requires = [ "desktop" ];
    };
    streaming = {
      description = "OBS set up with the overlay pack, and the mic cleanup chain";
      default = false;
      requires = [ "desktop" ];
    };
    vm = {
      description = "libvirt, virt-manager and a light Windows VM (WinApps)";
      default = false;
      requires = [ "desktop" ];
    };
  };

  entries = builtins.readDir hostsDir;
  hostNames = builtins.filter (
    n: n != "mac" && entries.${n} == "directory" && builtins.pathExists (hostsDir + "/${n}/host.json")
  ) (builtins.attrNames entries);

  # Aspects a host can pick: every NixOS module but base and the hosts' own.
  aspectNames = builtins.filter (n: n != "base" && !(builtins.elem n hostNames)) (
    builtins.attrNames nixos
  );

  readHost =
    name:
    let
      file = "modules/hosts/${name}/host.json";
      raw = lib.importJSON (hostsDir + "/${name}/host.json");
      aspects = lib.unique ([ "base" ] ++ (raw.aspects or [ ]));
      fail = msg: throw "${file}: ${msg}";
      unknown = builtins.filter (a: a != "base" && !(builtins.elem a aspectNames)) aspects;
      missing = lib.concatMap (
        a:
        map (r: "${a} needs ${r}") (
          builtins.filter (r: !(builtins.elem r aspects)) (aspectInfo.${a}.requires or [ ])
        )
      ) aspects;
    in
    if builtins.match "[a-z][a-z0-9-]{0,30}" name == null then
      fail "the folder name must be a-z, 0-9 and -, starting with a letter (at most 31)"
    else if (raw.name or name) != name then
      fail "\"name\" is \"${raw.name}\" but the folder is \"${name}\""
    else if builtins.elem name aspectNames || name == "base" then
      fail "\"${name}\" is also an aspect's name; pick another host name"
    else if unknown != [ ] then
      fail "unknown aspects ${toString unknown} (known: ${toString aspectNames})"
    else if missing != [ ] then
      fail (lib.concatStringsSep ", " missing)
    else if !(raw ? stateVersion) then
      fail "no \"stateVersion\""
    else
      {
        inherit name aspects;
        inherit (raw) stateVersion;
        system = raw.system or "x86_64-linux";
        gpu = raw.gpu or "none";
        primaryMonitor = raw.primaryMonitor or null;
        audioOutput = raw.audioOutput or null;
        replayMonitor = raw.replayMonitor or "screen";
      };

  hosts = lib.genAttrs hostNames readHost;

  # The host's own module. It is merged into flake.modules.nixos.<name>
  # together with whatever the host's .nix files put there (arc: monitors,
  # fans, vfio), so those only ever reach this one host.
  hostModule =
    name: host:
    let
      dir = hostsDir + "/${name}";
      hardware = dir + "/_hardware.nix";
    in
    {
      imports = [
        inputs.disko.nixosModules.disko
        (import ../hosts/_disko.nix {
          inherit inputs user;
          disksFile = dir + "/disks.json";
        })
      ]
      ++ lib.optional (builtins.pathExists hardware) hardware;

      vx.host = {
        inherit (host)
          name
          aspects
          gpu
          primaryMonitor
          audioOutput
          replayMonitor
          ;
      };

      networking.hostName = name;
      nixpkgs.hostPlatform = host.system;
      # Set once at install time; never bump it casually (it is NOT the NixOS version you run).
      system.stateVersion = host.stateVersion;
      home-manager.users.${user}.home.stateVersion = host.stateVersion;
    };

  # Base and the aspects, then the host's own module.
  mkHost =
    name: host:
    inputs.nixpkgs.lib.nixosSystem {
      modules = map (a: nixos.${a}) host.aspects ++ [ nixos.${name} ];
    };
in
{
  options.vx = {
    hosts = lib.mkOption {
      type = lib.types.raw;
      readOnly = true;
      description = "Every host folder's parsed host.json (flake/hosts.nix).";
    };
    aspects = lib.mkOption {
      type = lib.types.raw;
      readOnly = true;
      description = "The aspects a host can pick, with a description and default (for the installer).";
    };
  };

  config = {
    vx.hosts = hosts;
    vx.aspects = map (name: {
      inherit name;
      description = aspectInfo.${name}.description or name;
      default = aspectInfo.${name}.default or false;
      requires = aspectInfo.${name}.requires or [ ];
    }) aspectNames;

    flake.modules.nixos = lib.mapAttrs hostModule hosts;
    flake.nixosConfigurations = lib.mapAttrs mkHost hosts;
  };
}
