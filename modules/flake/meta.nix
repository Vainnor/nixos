# Values shared by every feature and every host. Read them from any
# flake-parts module via `config.meta.<name>` instead of threading specialArgs
# around. What differs per machine (GPU, monitors, audio output, GPU
# passthrough) lives in modules/hosts/<name>/ instead (see flake/hosts.nix).
#
# In a fork: the installer rewrites username, fullName, email, sshKeys,
# signingKey and repoUrl for you, turns secretsReady and resticReady off and
# sets binaryCache to null. The owner-only values (githubUser, macHost,
# macLibrewolfProfile) are harmless to keep: they only matter on the owner's
# Mac and to the installer's "who pushes where" check.
{ lib, ... }:
{
  options.meta = lib.mkOption {
    type = lib.types.attrsOf lib.types.anything;
    default = { };
    description = "Shared values for all features (user, theme, paths).";
  };

  config.meta = {
    username = "vainnor";
    fullName = "Vainnor";
    email = "me@carsonberget.com";

    timeZone = "America/Chicago";

    # GitHub repo (cloned into ~/nixos by the installer). Public is fine: no secrets live here.
    # A fork points this at itself; new machines are committed and pushed here.
    repoUrl = "https://github.com/vainnor/nixos";

    # The upstream owner's GitHub login. The installer pushes to repoUrl
    # directly only when you log in as this user; anyone else uses a fork.
    githubUser = "Vainnor";

    # Where the repo is cloned on the PC (nh uses this; the installer sets it).
    flakePath = "/home/vainnor/nixos";

    # Wallpaper and colours. `vx wallpaper <name>` changes this line and rebuilds.
    #   wallpaper: a folder in assets/wallpapers (`vx wallpapers` lists them),
    #              "dots" (flat dot grid), or a path like ../../assets/some.png
    #   theme:     "match" = the wallpaper's own palette; or "vainnor-slate" /
    #              "caelus" (assets/themes); or any base16 name
    wallpaper = "dome";
    theme = "match";
    # Colours laid over the active theme last, for quick experiments, e.g.
    # { base0D = "E0A040"; border = "3A3F3C"; } (see assets/themes/default.nix).
    # Lasting fixes go in the theme file's own `overrides`. `vx theme check` checks them.
    themeOverrides = { };

    # sops-nix secrets (secrets/secrets.yaml). Flip to true once the PC's host
    # key is a recipient in .sops.yaml and the file was re-encrypted
    # (`vx secrets host` on the PC, then `vx secrets add-host <key>` on the Mac).
    # false: nothing reads secrets.yaml (Wi-Fi, NetBird key, stream keys,
    # restic and the cache token are all skipped).
    secretsReady = false;

    # Your Windows account in the VMs (WinApps logs in as this; use a password, not a PIN).
    windowsUser = "vainnor";

    # Public SSH keys allowed to log in (over NetBird only). Paste yours here.
    # [ ] = nobody can log in over SSH.
    sshKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores vainnor@Carsons-MBP.home"
    ];

    # Git commits are signed with this key through the 1Password SSH agent
    # (the private half lives in 1Password, never on disk). null = unsigned commits.
    signingKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOC0Rp6DIXz+SMhHHpCCHb5S4Ni0gGBPIIVCevddores";

    # The Mac's hostname (`scutil --get LocalHostName`); darwin-rebuild picks
    # darwinConfigurations.<this> automatically. null = no Mac config.
    macHost = "Vainnors-MacBook-Pro";
    # The Mac's LibreWolf profile folder (about:support > Profile Folder); it gets
    # the same userChrome/userContent as the PC. null = leave LibreWolf alone.
    macLibrewolfProfile = "t3syv4fs.default-release";

    # Night light (wlsunset) follows sunset here. Chicago.
    location = {
      latitude = 41.88;
      longitude = -87.63;
    };

    # restic off-site backups. Turn on once restic/* is filled in secrets.yaml
    # (`vx secrets edit`) and secretsReady is true.
    resticReady = false;

    # Homelab binary cache (Attic). null = off. Fill in from the homelab, e.g.
    #   { endpoint = "https://cache.lab.vainnor.com"; cache = "nixos";
    #     publicKey = "nixos:AAAA..."; }
    # Both machines then download from it; the PC also pushes what it builds
    # (token in sops as attic/token, so only with secretsReady).
    binaryCache = null;
  };
}
