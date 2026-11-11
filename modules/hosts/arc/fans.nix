# Fan control for arc: CPU, pump and case fans on the MSI X670E Tomahawk.
#
# The board's fan chip (Nuvoton NCT6687) is read-only with the kernel's own
# driver, so the out-of-tree nct6687 driver replaces it. CoolerControl then
# draws the curves: open "CoolerControl" from the launcher, pick a fan, give
# it a curve against a sensor (CPU Tctl, GPU, liquid temp, ...). Profiles and
# modes (e.g. "quiet" / "gaming") switch in the app. The curves run in a
# background service, so the app can be closed.
#
# The GPU's fans stay with LACT (gaming/gpu.nix), so don't give them a curve
# in CoolerControl too; two programs fighting over one fan makes it pulse.
#
# The curves follow the CPU's temperature (Tctl), the case fans too: the board's
# own curve drove them from its "System" sensor, which barely moves when the
# CPU heats up, so they sat at 60% with the CPU at 90 °C. CPU fan (fan1):
# 40 °C 30%, 60 °C 45%, 70 °C 60%, 80 °C 85%, 85 °C+ 100%. Case fans (fan3-5,
# the connected ones): 30/40/55/80/100% at the same points. fan8 isn't
# PWM-controlled (it spins at 0%), so it's left to the board.
#
# Curves are kept on /persistent, so they survive reboots. `vx fans save`
# copies them into this repo (assets/coolercontrol/config.toml); a fresh
# install starts with that file instead of a blank setup.
{
  flake.modules.nixos.arc =
    { config, pkgs, ... }:
    {
      boot = {
        extraModulePackages = [ config.boot.kernelPackages.nct6687d ];
        kernelModules = [ "nct6687" ];
        blacklistedKernelModules = [ "nct6683" ]; # the read-only driver for the same chip
      };

      programs.coolercontrol.enable = true;
      environment.systemPackages = [ pkgs.lm_sensors ]; # `sensors` in a terminal

      preservation.preserveAt."/persistent".directories = [ "/etc/coolercontrol" ];

      # First boot after an install: start from the curves saved in the repo.
      # "C" only copies when the file doesn't exist yet, so it never
      # overwrites changes made in the app.
      systemd.tmpfiles.rules =
        let
          saved = ../../../assets/coolercontrol/config.toml;
        in
        if builtins.pathExists saved then
          [ "C /etc/coolercontrol/config.toml 0644 root root - ${saved}" ]
        else
          [ ];
    };
}
