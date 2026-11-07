# The Mac's apps, through Homebrew (nix-darwin writes the Brewfile and runs
# `brew bundle` on every `vx switch`). Command-line tools that Nix already
# provides (fish, starship, eza, git, gh, sops, opentofu, ...) are not
# listed here; see users/vainnor.nix and dev/tooling.nix.
#
# Nothing is ever removed automatically (cleanup = "none"): an app missing from
# these lists just stays installed. Set cleanup = "uninstall" once the lists are
# complete to make them exact.
#
# First time only: `vx mac adopt` hands the apps you installed by hand over
# to Homebrew, otherwise `brew bundle` stops at "already an App at ...".
#
# Not in Homebrew, so installed by hand: Cider, NetBird, DaVinci Resolve,
# BEACN, Orca, Adobe Creative Cloud, the Blackmagic tools, vATIS, YAAT, Xcode
# (App Store) and the JetBrains IDEs (through JetBrains Toolbox).
{
  flake.modules.darwin.mac = {
    homebrew = {
      enable = true;
      onActivation = {
        cleanup = "none";
        autoUpdate = false; # `vx mac upgrade` updates on purpose
        upgrade = false;
      };
      global.autoUpdate = false;

      taps = [
        "gcenx/wine"
        "infisical/get-cli"
        "siderolabs/tap"
      ];

      brews = [
        # Homelab / Kubernetes
        "ansible-lint"
        "argocd"
        "cilium-cli"
        "helm"
        "kubeconform"
        "kustomize"
        "siderolabs/tap/talosctl"
        "infisical/get-cli/infisical"
        # Dev
        "cloc"
        "composer"
        "docker" # CLI only; OrbStack is the engine
        "dotnet"
        "gitleaks"
        "mysql-client"
        "neovim"
        "node"
        "php@8.4"
        "pnpm"
        "python@3.12"
        "sccache"
        "shellcheck"
        # Misc
        "fonttools"
        "poppler"
        "qrencode"
        "winetricks"
      ];

      casks = [
        # Browsers
        "librewolf"
        "helium-browser"
        "google-chrome"
        # Terminal, editors, dev
        "ghostty"
        "zed"
        "jetbrains-toolbox"
        "orbstack"
        "herd"
        "virtualbox"
        "t3-code"
        # AI
        "claude"
        "chatgpt"
        # Chat
        "discord"
        "signal"
        "teamspeak-client"
        # Productivity
        "1password"
        "raycast"
        "fantastical"
        "affine"
        "shottr"
        # Creative
        "affinity"
        "blender"
        "bambu-studio"
        # Network / remote
        "rustdesk"
        "winbox"
        # System
        "aldente"
        "macs-fan-control"
        "mechvibes"
        # Windows apps
        "wine-stable"
        "wine@devel"
      ];
    };
  };
}
