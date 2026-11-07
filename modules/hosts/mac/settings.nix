# macOS settings, so a fresh Mac behaves the same after one `vx switch`.
# Change one here, run `vx switch`; some need a log out to show everywhere.
{ config, ... }:
let
  user = config.meta.username;
in
{
  flake.modules.darwin.mac = {
    system.defaults = {
      NSGlobalDomain = {
        AppleInterfaceStyle = "Dark";
        AppleShowAllExtensions = true;
        # Fast key repeat, and no accent popup when holding a key (vim keys repeat).
        KeyRepeat = 2;
        InitialKeyRepeat = 15;
        ApplePressAndHoldEnabled = false;
        # No auto-correct, auto-capitalise or "smart" punctuation (breaks code and commands).
        NSAutomaticSpellingCorrectionEnabled = false;
        NSAutomaticCapitalizationEnabled = false;
        NSAutomaticPeriodSubstitutionEnabled = false;
        NSAutomaticQuoteSubstitutionEnabled = false;
        NSAutomaticDashSubstitutionEnabled = false;
        AppleKeyboardUIMode = 3; # Tab moves through every control in dialogs
        NSDocumentSaveNewDocumentsToCloud = false;
        "com.apple.mouse.tapBehavior" = 1; # tap to click
      };

      dock = {
        autohide = true;
        autohide-delay = 0.0;
        autohide-time-modifier = 0.2;
        tilesize = 48;
        show-recents = false;
        mru-spaces = false; # keep Spaces in the order you made them
        minimize-to-application = true;
      };

      finder = {
        AppleShowAllExtensions = true;
        ShowPathbar = true;
        ShowStatusBar = true;
        FXPreferredViewStyle = "Nlsv"; # list view
        _FXShowPosixPathInTitle = true;
        _FXSortFoldersFirst = true;
        FXEnableExtensionChangeWarning = false;
        FXDefaultSearchScope = "SCcf"; # search the current folder
        NewWindowTarget = "Home";
      };

      trackpad.Clicking = true;

      screencapture = {
        location = "/Users/${user}/Pictures/Screenshots";
        type = "png";
        disable-shadow = true;
      };

      screensaver = {
        askForPassword = true;
        askForPasswordDelay = 0;
      };

      loginwindow.GuestEnabled = false;

      # Clicking the wallpaper no longer hides every window (Sonoma and later).
      WindowManager.EnableStandardClickToShowDesktop = false;

      CustomUserPreferences."com.apple.desktopservices" = {
        DSDontWriteNetworkStores = true; # no .DS_Store on network shares
        DSDontWriteUSBStores = true; # or USB drives
      };
    };

    networking.applicationFirewall = {
      enable = true;
      enableStealthMode = true; # don't answer pings or probes on closed ports
    };

    home-manager.users.${user} =
      { lib, ... }:
      {
        home.activation.screenshotsDir = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          run mkdir -p "$HOME/Pictures/Screenshots"
        '';
      };
  };
}
