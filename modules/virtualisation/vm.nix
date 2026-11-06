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
      hostdev = addr: ''
        <hostdev mode="subsystem" type="pci" managed="yes">
          <source>${pci addr}</source>
        </hostdev>'';
      usbdev = d: ''
        <hostdev mode="subsystem" type="usb" managed="yes">
          <source><vendor id="${d.vendor}"/><product id="${d.product}"/></source>
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

      disk =
        if vfio.windowsNvme or null != null then
          hostdev vfio.windowsNvme
        else
          ''
            <disk type="file" device="disk">
              <driver name="qemu" type="raw" cache="none" io="native" discard="unmap"/>
              <source file="/var/lib/libvirt/images/${vmName}.img"/>
              <target dev="sda" bus="scsi"/>
              <boot order="1"/>
            </disk>
            <disk type="file" device="cdrom">
              <driver name="qemu" type="raw"/>
              <source file="/var/lib/libvirt/images/Win11.iso"/>
              <target dev="sdb" bus="sata"/>
              <readonly/>
              <boot order="2"/>
            </disk>
            <disk type="file" device="cdrom">
              <driver name="qemu" type="raw"/>
              <source file="/var/lib/libvirt/images/virtio-win.iso"/>
              <target dev="sdc" bus="sata"/>
              <readonly/>
            </disk>'';

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
          <os firmware="efi">
            <type arch="x86_64" machine="q35">hvm</type>
            <firmware>
              <feature enabled="yes" name="secure-boot"/>
              <feature enabled="yes" name="enrolled-keys"/>
            </firmware>
          </os>
          <features>
            <acpi/><apic/>
            <hyperv mode="custom">
              <relaxed state="on"/><vapic state="on"/><spinlocks state="on" retries="8191"/>
              <vpindex state="on"/><runtime state="on"/><synic state="on"/>
              <stimer state="on"/><reset state="on"/><frequencies state="on"/>
              <tlbflush state="on"/><ipi state="on"/>
            </hyperv>
            <smm state="on"/>
          </features>
          <cpu mode="host-passthrough" check="none" migratable="off">
            <topology sockets="1" dies="1" clusters="1" cores="7" threads="2"/>
            <cache mode="passthrough"/>
            <feature policy="require" name="topoext"/>
          </cpu>
          <clock offset="localtime">
            <timer name="rtc" tickpolicy="catchup"/>
            <timer name="pit" tickpolicy="delay"/>
            <timer name="hpet" present="no"/>
            <timer name="hypervclock" present="yes"/>
          </clock>
          <pm><suspend-to-mem enabled="no"/><suspend-to-disk enabled="no"/></pm>
          <devices>
            <controller type="scsi" model="virtio-scsi"/>
            ${disk}
            <interface type="network">
              <source network="default"/>
              <model type="virtio"/>
            </interface>
            <tpm model="tpm-crb"><backend type="emulator" version="2.0"/></tpm>
            ${hostdev vfio.gpu}
            ${hostdev vfio.gpuAudio}
            ${lib.concatMapStrings usbdev (vfio.usb or [ ])}
            <video><model type="none"/></video>
            <memballoon model="none"/>
          </devices>
        </domain>
      '';

      # Runs as root from libvirtd around the VM's lifetime.
      hook = pkgs.writeShellScript "gpu-passthrough-hook" ''
        set -u
        [ "$1" = "${vmName}" ] || exit 0
        case "$2/$3" in
          prepare/begin)
            systemctl stop greetd lactd.service 2>/dev/null
            sleep 3
            for c in /sys/class/vtconsole/vtcon*/bind; do echo 0 > "$c" || true; done
            echo efi-framebuffer.0 > /sys/bus/platform/drivers/efi-framebuffer/unbind 2>/dev/null || true
            echo 8192 > /proc/sys/vm/nr_hugepages   # 16 GiB in 2 MiB pages
            ;;
          release/end)
            echo 0 > /proc/sys/vm/nr_hugepages
            for c in /sys/class/vtconsole/vtcon*/bind; do echo 1 > "$c" || true; done
            systemctl start lactd.service greetd
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
      ];
      users.users.${meta.username}.extraGroups = [
        "libvirtd"
        "kvm"
      ];
      networking.firewall.trustedInterfaces = [ "virbr0" ]; # DHCP/DNS for VMs

      boot.kernelParams = [
        "amd_iommu=on"
        "iommu=pt"
      ];
      boot.extraModprobeConfig = "options kvm ignore_msrs=1 report_ignored_msrs=0";

      environment.etc = lib.mkIf (vfio != null) { "libvirt-templates/${vmName}.xml".source = domainXml; };

      preservation.preserveAt."/persistent".directories = [ "/var/lib/libvirt" ];
      # VM disk images are rewritten in place; copy-on-write would fragment them.
      systemd.tmpfiles.rules = [
        "d /var/lib/libvirt/images 0711 root root -"
        "h /var/lib/libvirt/images - - - - +C"
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
