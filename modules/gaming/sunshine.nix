# Sunshine: stream games (or the whole desktop) from the PC to Moonlight on
# the Mac, a phone or a TV, at home or over NetBird when you're away.
# First run: open https://localhost:47990 on the PC, create the admin login,
# then pair each Moonlight client with the PIN it shows.
{
  flake.modules.nixos.gaming = {
    services.sunshine = {
      enable = true;
      autoStart = true;
      capSysAdmin = true; # KMS capture on Mango (wlroots has no capture portal for it)
      openFirewall = true; # pairing needs the PIN, the web UI only answers the LAN
    };
  };
}
