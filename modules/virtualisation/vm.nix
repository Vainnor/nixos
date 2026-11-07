# VMs: libvirt + virt-manager for everyday VMs (Alpine to learn OpenRC, etc.)
# and a Windows 11 VM that takes over the RX 7900 XTX ("single-GPU passthrough").
#
# How the Windows VM works:
#   `just windows` starts it. A libvirt hook stops the Mango session, hands the
#   GPU (and its HDMI/DP audio) to the VM, reserves 16 GB of hugepages, and the
#   VM drives your monitors directly at near-native speed. The USB devices in
#   meta.vfio.usb (keyboard, mouse, BEACN, headset, webcam) move into the VM.
#   Shut Windows down and everything comes back to Linux at the login screen.
#
#   CPU: 7 of 8 cores (14 threads) pinned to the VM; core 0 stays with Linux.
#   Disk: a raw image on a no-copy-on-write folder, or (meta.vfio.windowsNvme)
#   the whole Windows NVMe so the VM boots your real Windows install.
#
# "win11-lite" is a second, everyday Windows VM for light Windows-only apps:
#   `just windows-lite` opens it in a window on your Linux desktop. No GPU
#   passthrough, 4 cores / 8 threads (half the CPU, shared with Linux), 8 GB
#   RAM, its own disk. Graphics are a virtio display, with Windows' built-in
#   software renderer handling Direct3D: fine for office tools, utilities and
#   2D apps; not for games or heavy 3D. Clipboard sharing and auto-resize come
#   from the SPICE agent, which the first-login script installs.
#   It's always available; it doesn't need meta.vfio.
#
# Both install hands-off from a new disk image: an answer file on an extra CD
# gives Windows 11 Pro with a local account and privacy settings turned down
# (see _unattend.nix).
#
# Games with kernel anti-cheat (Valorant, many shooters) refuse VMs: boot
# Windows natively for those (F11 or the power menu's "Reboot to Windows").
{ config, ... }:
let
  inherit (config) meta;
  vfio = meta.vfio;
  vmName = "win11";
in
{
  flake.modules.nixos.vm =
    { lib, pkgs, ... }:
    let
      pci =
        addr:
        let
          m = builtins.match "([0-9a-f]{4}):([0-9a-f]{2}):([0-9a-f]{2})\\.([0-7])" addr;
        in
        ''<address domain="0x${builtins.elemAt m 0}" bus="0x${builtins.elemAt m 1}" slot="0x${builtins.elemAt m 2}" function="0x${builtins.elemAt m 3}"/>'';
      # managed="no": the hook moves devices to vfio-pci itself, and nothing
      # ever moves them back (see the hook for why).
      hostdev = addr: ''
        <hostdev mode="subsystem" type="pci" managed="no">
          <source>${pci addr}</source>
        </hostdev>'';
      # The GPU with its video BIOS from a file (meta.vfio.gpuRom): without it
      # the VM's firmware can't drive the card and the screens stay black.
      # GPU and its audio as functions .0 and .1 of one slot, like on the real
      # card: AMD's driver stops with code 43 when they're separate devices.
      gpuHostdev = ''
        <controller type="pci" index="16" model="pcie-root-port"/>
        <hostdev mode="subsystem" type="pci" managed="no">
          <source>${pci vfio.gpu}</source>
          ${lib.optionalString (vfio.gpuRom or null != null) ''<rom bar="on" file="${vfio.gpuRom}"/>''}
          <address type="pci" domain="0x0000" bus="0x10" slot="0x00" function="0x0" multifunction="on"/>
        </hostdev>
        <hostdev mode="subsystem" type="pci" managed="no">
          <source>${pci vfio.gpuAudio}</source>
          <address type="pci" domain="0x0000" bus="0x10" slot="0x00" function="0x1"/>
        </hostdev>'';
      usbdev = d: ''
        <hostdev mode="subsystem" type="usb" managed="yes">
          <source startupPolicy="optional"><vendor id="${d.vendor}"/><product id="${d.product}"/></source>
        </hostdev>'';

      # vCPU pairs -> host cores 1-7 with their SMT siblings 9-15.
      vcpupins =
        lib.concatImapStrings (i: c: ''<vcpupin vcpu="${toString (i - 1)}" cpuset="${toString c}"/>'')
          (
            lib.concatMap (n: [
              n
              (n + 8)
            ]) (lib.range 1 7)
          );

      imagesDir = "/var/lib/libvirt/images";

      # Answer file + first-login script for a hands-off install (see _unattend.nix).
      unattendIso =
        name: winapps:
        import ./_unattend.nix {
          inherit pkgs winapps;
          # Both VMs' only disk is their own image (the gaming VM's USB
          # passthrough list has no storage devices).
          autoDisk = true;
          user = meta.windowsUser;
          computerName = name;
        };
      unattendCd = dev: name: winapps: ''
        <disk type="file" device="cdrom">
          <driver name="qemu" type="raw"/>
          <source file="${unattendIso name winapps}"/>
          <target dev="${dev}" bus="sata"/>
          <readonly/>
        </disk>'';
      installMedia = ''
        <disk type="file" device="cdrom">
          <driver name="qemu" type="raw"/>
          <source file="${imagesDir}/Win11.iso"/>
          <target dev="sdb" bus="sata"/>
          <readonly/>
          <boot order="2"/>
        </disk>
        <disk type="file" device="cdrom">
          <driver name="qemu" type="raw"/>
          <source file="${imagesDir}/virtio-win.iso"/>
          <target dev="sdc" bus="sata"/>
          <readonly/>
        </disk>'';
      imageDisk = name: ''
        <disk type="file" device="disk">
          <driver name="qemu" type="raw" cache="none" io="native" discard="unmap"/>
          <source file="${imagesDir}/${name}.img"/>
          <target dev="sda" bus="scsi"/>
          <boot order="1"/>
        </disk>'';

      # Shared by both Windows VMs: UEFI with Secure Boot, Hyper-V enlightenments, clock.
      # The Secure Boot firmware that ships with QEMU has no Microsoft keys loaded
      # ("enrolled-keys"), and asking for them makes `virsh define` fail with
      # "Unable to find 'efi' firmware". Windows 11 only needs Secure Boot to be
      # supported, so it installs and runs fine without the keys.
      firmware = ''
        <os firmware="efi">
          <type arch="x86_64" machine="q35">hvm</type>
          <firmware>
            <feature enabled="yes" name="secure-boot"/>
            <feature enabled="no" name="enrolled-keys"/>
          </firmware>
        </os>
        <features>
          <acpi/><apic/>
          <hyperv mode="custom">
            <relaxed state="on"/><vapic state="on"/><spinlocks state="on" retries="8191"/>
            <vpindex state="on"/><runtime state="on"/><synic state="on"/>
            <stimer state="on"/><reset state="on"/><frequencies state="on"/>
            <tlbflush state="on"/><ipi state="on"/>
            <vendor_id state="on" value="1234567890ab"/>
          </hyperv>
          <smm state="on"/>
        </features>
        <clock offset="localtime">
          <timer name="rtc" tickpolicy="catchup"/>
          <timer name="pit" tickpolicy="delay"/>
          <timer name="hpet" present="no"/>
          <timer name="hypervclock" present="yes"/>
        </clock>
        <pm><suspend-to-mem enabled="no"/><suspend-to-disk enabled="no"/></pm>'';
      commonDevices = ''
        <controller type="scsi" model="virtio-scsi"/>
        <interface type="network">
          <source network="default"/>
          <model type="virtio"/>
        </interface>
        <tpm model="tpm-crb"><backend type="emulator" version="2.0"/></tpm>'';

      disk =
        if vfio.windowsNvme or null != null then
          hostdev vfio.windowsNvme
        else
          imageDisk vmName + installMedia + unattendCd "sdd" vmName false;

      domainXml = pkgs.writeText "${vmName}.xml" ''
        <domain type="kvm">
          <name>${vmName}</name>
          <memory unit="GiB">16</memory>
          <memoryBacking><hugepages/></memoryBacking>
          <vcpu placement="static">14</vcpu>
          <cputune>
            ${vcpupins}
            <emulatorpin cpuset="0,8"/>
          </cputune>
          ${firmware}
          <cpu mode="host-passthrough" check="none" migratable="off">
            <topology sockets="1" dies="1" clusters="1" cores="7" threads="2"/>
            <cache mode="passthrough"/>
            <feature policy="require" name="topoext"/>
          </cpu>
          <devices>
            ${commonDevices}
            ${disk}
            ${gpuHostdev}
            <controller type="usb" model="qemu-xhci" ports="15"/>
            <channel type="unix"><target type="virtio" name="org.qemu.guest_agent.0"/></channel>
            ${lib.concatMapStrings usbdev (vfio.usb or [ ])}
            ${
              if vfio.installDisplay or false then
                # Temporary virtual screen (VNC, this PC only) until Windows has
                # the AMD driver; the card shows nothing before that.
                ''
                  <video><model type="virtio" heads="1" primary="yes"/></video>
                                  <graphics type="vnc" port="5909" autoport="no" listen="127.0.0.1"/>
                                  <input type="tablet" bus="usb"/>''
              else
                ''<video><model type="none"/></video>''
            }
            <memballoon model="none"/>
          </devices>
        </domain>
      '';

      liteName = "win11-lite";
      liteXml = pkgs.writeText "${liteName}.xml" ''
        <domain type="kvm">
          <name>${liteName}</name>
          <memory unit="GiB">8</memory>
          <vcpu placement="static">8</vcpu>
          ${firmware}
          <cpu mode="host-passthrough" check="none" migratable="off">
            <topology sockets="1" dies="1" clusters="1" cores="4" threads="2"/>
            <cache mode="passthrough"/>
            <feature policy="require" name="topoext"/>
          </cpu>
          <devices>
            ${commonDevices}
            ${imageDisk liteName}
            ${installMedia}
            <disk type="file" device="cdrom">
              <driver name="qemu" type="raw"/>
              <source file="/etc/libvirt-templates/winapps-oem.iso"/>
              <target dev="sdd" bus="sata"/>
              <readonly/>
            </disk>
            ${unattendCd "sde" liteName true}
            <video><model type="virtio" heads="1" primary="yes"><resolution x="3440" y="1440"/></model></video>
            <graphics type="spice" autoport="yes">
              <listen type="none"/>
              <image compression="off"/>
            </graphics>
            <channel type="spicevmc"><target type="virtio" name="com.redhat.spice.0"/></channel>
            <channel type="unix"><target type="virtio" name="org.qemu.guest_agent.0"/></channel>
            <input type="tablet" bus="usb"/>
            <sound model="ich9"><audio id="1"/></sound>
            <audio id="1" type="spice"/>
            <redirdev bus="usb" type="spicevmc"/>
            <redirdev bus="usb" type="spicevmc"/>
            <controller type="usb" model="qemu-xhci" ports="15"/>
            <memballoon model="virtio"/>
          </devices>
        </domain>
      '';

      # Runs as root from libvirtd around the VM's lifetime.
      #
      # The GPU is never handed back to Linux: on current kernels, unbinding an
      # AMD card from vfio-pci crashes the kernel (NULL dereference in
      # vfio_pci_core_sriov_configure, seen on RDNA3/RDNA4 from 6.18 to 7.3).
      # So when Windows shuts down, the PC reboots straight back into Linux.
      passthrough = [
        vfio.gpu
        vfio.gpuAudio
      ]
      ++ lib.optional (vfio.windowsNvme or null != null) vfio.windowsNvme;
      reserve = vfio != null && vfio.reserveGpu or false;
      used = "/run/${vmName}-gpu-used";
      # Runs instead of `modprobe amdgpu` (amdgpu loads for the CPU's
      # graphics): give the 7900 XTX to vfio-pci before amdgpu can grab it.
      # Its BARs stay at the firmware's sizes (Re-Size BAR off in the BIOS:
      # 256 MB / 2 MB, the setup known to work with this card in Windows).
      claimGpu = pkgs.writeShellScript "claim-gpu-for-vfio" ''
        d=/sys/bus/pci/devices/${vfio.gpu}
        if [ ! -e "$d/driver" ]; then
          echo vfio-pci > "$d/driver_override"
          ${pkgs.kmod}/bin/modprobe vfio-pci
          echo ${vfio.gpu} > /sys/bus/pci/drivers_probe
        fi
        exec ${pkgs.kmod}/bin/modprobe --ignore-install amdgpu "$@"
      '';
      # Lend the reserved card to Linux until the next reboot (for games or
      # anything heavy on the Linux side). The gaming VM can still take it
      # after that, but then the PC reboots when Windows shuts down.
      gpuLinux = pkgs.writeShellScriptBin "gpu-linux" ''
        set -eu
        if virsh -c qemu:///system domstate ${vmName} 2>/dev/null | grep -q running; then
          echo "${vmName} is using the card; shut Windows down first." >&2
          exit 1
        fi
        # Unbinding the card from vfio-pci after a VM used it crashes the
        # kernel (see below); a fresh boot is the only clean way back.
        if [ -e ${used} ]; then
          echo "Windows used the card since boot; reboot first, then run gpu-linux." >&2
          exit 1
        fi
        for d in ${vfio.gpuAudio} ${vfio.gpu}; do
          p=/sys/bus/pci/devices/$d
          [ "$(basename "$(readlink "$p/driver" 2>/dev/null)")" = vfio-pci ] || continue
          echo "Handing $d to Linux"
          sudo sh -c "echo > $p/driver_override; echo $d > /sys/bus/pci/drivers/vfio-pci/unbind"
          # Full-size BARs again for Linux (resizable BAR).
          [ "$d" = ${vfio.gpu} ] && sudo sh -c "echo 15 > $p/resource0_resize; echo 8 > $p/resource2_resize" 2>/dev/null || true
          sudo sh -c "echo $d > /sys/bus/pci/drivers_probe"
        done
        sudo systemctl restart lactd.service
        echo "Done. The card is Linux's until you reboot."
      '';
      hook = pkgs.writeShellScript "gpu-passthrough-hook" ''
        set -u
        [ "$1" = "${vmName}" ] || exit 0
        PATH=${
          lib.makeBinPath [
            pkgs.coreutils
            pkgs.kmod
            pkgs.systemd
            pkgs.psmisc
            pkgs.efibootmgr
            pkgs.gnused
          ]
        }
        taken=/run/${vmName}-gpu-taken
        to_vfio() {
          d=/sys/bus/pci/devices/$1
          # Already there (VM restarted without a reboot): leave it. Unbinding
          # from vfio-pci is exactly what crashes the kernel.
          if [ "$(basename "$(readlink "$d/driver" 2>/dev/null)")" = vfio-pci ]; then return 0; fi
          echo vfio-pci > "$d/driver_override"
          if [ -e "$d/driver" ]; then echo "$1" > "$d/driver/unbind"; fi
          # Resizable BAR: the card's 32 GB BAR gives code 43 in Windows; 8 GB
          # (2^13 MB) works. Only possible while no driver owns the card.
          if [ -e "$d/resource0_resize" ] && [ "$1" = "${vfio.gpu}" ]; then
            echo 13 > "$d/resource0_resize" 2>/dev/null || echo "BAR resize failed; continuing" >&2
            echo 3 > "$d/resource2_resize" 2>/dev/null || true # BAR 2 over 8 MB: code 43 too
          fi
          echo "$1" > /sys/bus/pci/drivers_probe
        }
        restore_desktop() {
          echo 0 > /proc/sys/vm/nr_hugepages
          for c in /sys/class/vtconsole/vtcon*/bind; do echo 1 > "$c" || true; done
          systemctl start lactd.service greetd
        }
        case "$2/$3" in
          prepare/begin)
            # 16 GiB in 2 MiB pages, before touching the desktop (compact first
            # so fragmented RAM doesn't make it fall short).
            sync; echo 3 > /proc/sys/vm/drop_caches; echo 1 > /proc/sys/vm/compact_memory
            echo 8192 > /proc/sys/vm/nr_hugepages
            if [ "$(cat /proc/sys/vm/nr_hugepages)" -lt 8192 ]; then
              echo "Only $(cat /proc/sys/vm/nr_hugepages) of 8192 hugepages free; close some apps and try again." >&2
              echo 0 > /proc/sys/vm/nr_hugepages
              exit 1
            fi
            modprobe vfio-pci
            touch ${used}
            # Card reserved from boot (meta.vfio.reserveGpu): the desktop runs
            # on the CPU's graphics and stays up. Otherwise take the card from
            # Linux, which means stopping the desktop on it.
            if [ "$(basename "$(readlink /sys/bus/pci/devices/${vfio.gpu}/driver 2>/dev/null)")" != vfio-pci ]; then
              systemctl stop greetd lactd.service 2>/dev/null
              sleep 3
              # Anything still holding the card (user services kept alive by an
              # SSH login, for example) would make the handoff hang.
              fuser -k /dev/dri/by-path/pci-${vfio.gpu}-* 2>/dev/null || true
              sleep 1
              for c in /sys/class/vtconsole/vtcon*/bind; do echo 0 > "$c" || true; done
              echo efi-framebuffer.0 > /sys/bus/platform/drivers/efi-framebuffer/unbind 2>/dev/null || true
              touch "$taken"
            fi
            ${lib.concatMapStrings (a: "to_vfio ${a}\n            ") passthrough}
            ;;
          release/end)
            # Windows shut down (or failed to start). If the GPU went to the VM,
            # reboot into Linux; otherwise just bring the desktop back.
            if [ -e "$taken" ]; then
              # Come back to NixOS even if the firmware's default is Windows:
              # boot next from the entry this boot came from.
              entry=$(efibootmgr | sed -n 's/^BootCurrent: *\([0-9A-Fa-f]\{4\}\).*/\1/p')
              if [ -n "$entry" ]; then efibootmgr --bootnext "$entry" >/dev/null || true; fi
              systemctl --no-block reboot
            else
              restore_desktop
            fi
            ;;
        esac
        exit 0
      '';
    in
    {
      virtualisation.libvirtd = {
        enable = true;
        onBoot = "ignore";
        onShutdown = "shutdown";
        qemu.swtpm.enable = true; # TPM 2.0 for Windows 11
        hooks.qemu = lib.mkIf (vfio != null) { gpu-passthrough = hook; };
      };
      programs.virt-manager.enable = true;
      environment.systemPackages = with pkgs; [
        pciutils # lspci
        usbutils # lsusb
        qemu-utils # qemu-img
        virt-viewer # window for win11-lite
      ]
      ++ lib.optional reserve gpuLinux;
      users.users.${meta.username}.extraGroups = [
        "libvirtd"
        "kvm"
      ];
      networking.firewall.trustedInterfaces = [ "virbr0" ]; # DHCP/DNS for VMs

      boot.kernelParams = [
        "amd_iommu=on"
        "iommu=pt"
      ]
      ++ lib.optional reserve "vfio-pci.ids=${vfio.gpuAudioId}"; # the card's audio, before the sound driver
      boot.extraModprobeConfig = ''
        options kvm ignore_msrs=1 report_ignored_msrs=0
      ''
      + lib.optionalString reserve ''
        install amdgpu ${claimGpu} $CMDLINE_OPTS
      '';
      # amdgpu in the initrd for the boot splash, unless the card is reserved
      # (then vfio-pci has to get to it first).
      boot.initrd.kernelModules =
        if reserve then
          [
            "vfio_pci"
            "vfio"
            "vfio_iommu_type1"
          ]
        else
          [ "amdgpu" ];

      environment.etc = {
        "libvirt-templates/${liteName}.xml".source = liteXml;
      }
      // lib.optionalAttrs (vfio != null) { "libvirt-templates/${vmName}.xml".source = domainXml; };

      preservation.preserveAt."/persistent".directories = [ "/var/lib/libvirt" ];
      # VM disk images are rewritten in place; copy-on-write would fragment them.
      # Their own subvolume ("v") also keeps them out of the hourly snapshots.
      systemd.tmpfiles.rules = [
        "v /var/lib/libvirt/images 0711 root root -"
        "h /var/lib/libvirt/images - - - - +C"
        # swtpm signs each new VM's TPM with a local CA kept here; NixOS doesn't
        # create it, and without it `virsh start` fails in swtpm_setup.
        "d /var/lib/swtpm-localca 0750 tss tss -"
      ];

      assertions = lib.optional (vfio != null) {
        assertion =
          builtins.all (a: builtins.match "[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\\.[0-7]" a != null)
            (
              [
                vfio.gpu
                vfio.gpuAudio
              ]
              ++ lib.optional (vfio.windowsNvme or null != null) vfio.windowsNvme
            );
        message = "meta.vfio PCI addresses must look like 0000:03:00.0 (see `just vm-gpu`).";
      };
    };
}
