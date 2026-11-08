# The graphics driver, from `gpu` in the host's host.json:
#
#   amd     open amdgpu + Mesa (RADV, radeonsi VA-API), amdgpu in the initrd
#           for the boot splash, and LACT for fan curves / power limit /
#           undervolt (overdrive unlocked).
#   nvidia  the proprietary driver with NVIDIA's open kernel module (Turing /
#           GTX 16xx and newer only), kernel modesetting, VA-API through
#           nvidia-vaapi-driver, early KMS for the boot splash, and the
#           environment wlroots compositors (Mango) want on NVIDIA.
#   intel   Mesa (iris/ANV) plus intel-media-driver for VA-API (Broadwell and
#           newer).
#   none    nothing (a VM, a server, or software rendering).
{
  flake.modules.nixos.base =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      inherit (config.vx.host) gpu vfio;
      # arc's passthrough can keep the card away from amdgpu from boot (virtualisation/vm.nix).
      reserved = vfio != null && (vfio.reserveGpu or false);
    in
    lib.mkMerge [
      (lib.mkIf (gpu != "none") { hardware.graphics.enable = true; })

      (lib.mkIf (gpu == "amd") {
        hardware.amdgpu.overdrive.enable = true;
        services.lact.enable = true;
        # amdgpu in the initrd for the boot splash, unless the card is reserved
        # for the VM (then vfio-pci has to get to it first).
        boot.initrd.kernelModules = lib.mkIf (!reserved) [ "amdgpu" ];
      })

      (lib.mkIf (gpu == "nvidia") {
        services.xserver.videoDrivers = [ "nvidia" ]; # loads the driver, X11 or not
        hardware.nvidia = {
          open = true; # the open kernel module (needs Turing or newer)
          modesetting.enable = true; # also gives nvidia-drm.fbdev=1 (driver 545+)
          powerManagement.enable = true; # keeps video memory across suspend
          videoAcceleration = true; # nvidia-vaapi-driver
          nvidiaSettings = false; # an X11 tool; not useful on Wayland
        };
        # Early KMS: the splash and the greeter come up on the NVIDIA driver.
        boot.initrd.kernelModules = [
          "nvidia"
          "nvidia_modeset"
          "nvidia_uvm"
          "nvidia_drm"
        ];
        environment.sessionVariables = {
          LIBVA_DRIVER_NAME = "nvidia";
          NVD_BACKEND = "direct"; # nvidia-vaapi-driver's backend on current drivers
          __GLX_VENDOR_LIBRARY_NAME = "nvidia"; # XWayland apps (Steam) get NVIDIA's GL
        };
        unfree.allowed = [
          "nvidia-x11"
          "nvidia-settings"
          "nvidia-persistenced"
        ];
      })

      (lib.mkIf (gpu == "intel") {
        hardware.graphics.extraPackages = [ pkgs.intel-media-driver ];
        hardware.graphics.extraPackages32 = [ pkgs.pkgsi686Linux.intel-media-driver ];
        environment.sessionVariables.LIBVA_DRIVER_NAME = "iHD";
      })
    ];
}
