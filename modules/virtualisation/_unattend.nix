# Hands-off Windows 11 install for the VMs: an answer file (autounattend.xml)
# plus a first-login script (setup.ps1), both on a small CD that Windows Setup
# finds by itself.
#
#   - Windows 11 Pro, US English, Chicago time zone.
#   - A local account, no Microsoft account screen. The first password is
#     "changeme" (this repo is public, so it can't be a real one); Windows asks
#     for a new one at the next login.
#   - No network during setup (the virtio network driver comes afterwards),
#     which also keeps Setup from pushing the Microsoft account.
#   - Telemetry at the lowest level Pro allows, no ads, suggestions, Copilot,
#     Recall, web results in Start, activity history or location; OneDrive and
#     preinstalled store apps removed.
#   - The virtio drivers + guest agent + SPICE agent from the virtio CD.
#   - win11-lite also runs the WinApps guest setup from the WINAPPS CD.
#
# Not a module (the leading _ keeps import-tree away); vm.nix calls it.
{
  pkgs,
  user,
  computerName,
  winapps ? false,
  # Wipe disk 0 and install there without asking. Only for a VM whose sole
  # disk is its own image (win11-lite); USB drives passed into the gaming VM
  # could also show up as disks, so that one still asks.
  autoDisk ? false,
}:
let
  # Windows' generic Pro *install* key: picks the edition, doesn't activate it.
  proKey = "VK7JG-NPHTM-C97JM-9MPGT-3V66T";
  intl = pass: ''
    <component name="Microsoft-Windows-International-Core${
      if pass == "windowsPE" then "-WinPE" else ""
    }" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      ${
        if pass == "windowsPE" then
          "<SetupUILanguage><UILanguage>en-US</UILanguage></SetupUILanguage>"
        else
          ""
      }
      <InputLocale>0409:00000409</InputLocale>
      <SystemLocale>en-US</SystemLocale>
      <UILanguage>en-US</UILanguage>
      <UserLocale>en-US</UserLocale>
    </component>'';
  # The virtio CD's letter depends on how many discs there are; missing paths are skipped.
  driverPaths = builtins.concatStringsSep "\n" (
    builtins.genList (
      i:
      let
        letter = builtins.elemAt [ "D" "E" "F" "G" "H" "I" ] i;
      in
      ''<PathAndCredentials wcm:action="add" wcm:keyValue="${toString (i + 1)}"><Path>${letter}:\vioscsi\w11\amd64</Path></PathAndCredentials>''
    ) 6
  );

  diskConfig = ''
    <DiskConfiguration>
      <Disk wcm:action="add">
        <DiskID>0</DiskID>
        <WillWipeDisk>true</WillWipeDisk>
        <CreatePartitions>
          <CreatePartition wcm:action="add"><Order>1</Order><Type>EFI</Type><Size>300</Size></CreatePartition>
          <CreatePartition wcm:action="add"><Order>2</Order><Type>MSR</Type><Size>16</Size></CreatePartition>
          <CreatePartition wcm:action="add"><Order>3</Order><Type>Primary</Type><Extend>true</Extend></CreatePartition>
        </CreatePartitions>
        <ModifyPartitions>
          <ModifyPartition wcm:action="add"><Order>1</Order><PartitionID>1</PartitionID><Format>FAT32</Format><Label>System</Label></ModifyPartition>
          <ModifyPartition wcm:action="add"><Order>2</Order><PartitionID>2</PartitionID></ModifyPartition>
          <ModifyPartition wcm:action="add"><Order>3</Order><PartitionID>3</PartitionID><Format>NTFS</Format><Label>Windows</Label><Letter>C</Letter></ModifyPartition>
        </ModifyPartitions>
      </Disk>
    </DiskConfiguration>
    <ImageInstall>
      <OSImage>
        <InstallTo><DiskID>0</DiskID><PartitionID>3</PartitionID></InstallTo>
      </OSImage>
    </ImageInstall>'';

  xml = ''
    <?xml version="1.0" encoding="utf-8"?>
    <unattend xmlns="urn:schemas-microsoft-com:unattend" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
      <settings pass="windowsPE">
        ${intl "windowsPE"}
        <component name="Microsoft-Windows-PnpCustomizationsWinPE" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
          <DriverPaths>
            ${driverPaths}
          </DriverPaths>
        </component>
        <component name="Microsoft-Windows-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
          <UserData>
            <ProductKey><Key>${proKey}</Key><WillShowUI>OnError</WillShowUI></ProductKey>
            <AcceptEula>true</AcceptEula>
          </UserData>
            ${if autoDisk then diskConfig else ""}
        </component>
      </settings>
      <settings pass="specialize">
        <component name="Microsoft-Windows-Deployment" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
          <RunSynchronous>
            <RunSynchronousCommand wcm:action="add">
              <Order>1</Order>
              <Path>reg.exe add "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\OOBE" /v BypassNRO /t REG_DWORD /d 1 /f</Path>
            </RunSynchronousCommand>
          </RunSynchronous>
        </component>
        <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
          <ComputerName>${computerName}</ComputerName>
          <TimeZone>Central Standard Time</TimeZone>
        </component>
      </settings>
      <settings pass="oobeSystem">
        ${intl "oobeSystem"}
        <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
          <OOBE>
            <HideEULAPage>true</HideEULAPage>
            <HideOEMRegistrationScreen>true</HideOEMRegistrationScreen>
            <HideOnlineAccountScreens>true</HideOnlineAccountScreens>
            <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
            <HideLocalAccountScreen>true</HideLocalAccountScreen>
            <ProtectYourPC>3</ProtectYourPC>
          </OOBE>
          <UserAccounts>
            <LocalAccounts>
              <LocalAccount wcm:action="add">
                <Name>${user}</Name>
                <DisplayName>${user}</DisplayName>
                <Group>Administrators</Group>
                <Password><Value>changeme</Value><PlainText>true</PlainText></Password>
              </LocalAccount>
            </LocalAccounts>
          </UserAccounts>
          <FirstLogonCommands>
            <SynchronousCommand wcm:action="add">
              <Order>1</Order>
              <Description>Privacy settings, drivers, cleanup</Description>
              <CommandLine>cmd /c for %d in (D E F G H I J K) do @if exist %d:\setup.ps1 powershell -NoProfile -ExecutionPolicy Bypass -File %d:\setup.ps1</CommandLine>
            </SynchronousCommand>
          </FirstLogonCommands>
        </component>
      </settings>
    </unattend>
  '';

  ps1 = ''
    # First login: runs once, elevated, as ${user}. Log: C:\Windows\Temp\vm-setup.log
    Start-Transcript -Path "$env:SystemRoot\Temp\vm-setup.log"
    $ErrorActionPreference = 'Continue'

    function Set-Reg($Path, $Name, $Value) {
      if (-not (Test-Path $Path)) { New-Item -Path $Path -Force | Out-Null }
      New-ItemProperty -Path $Path -Name $Name -Value $Value -PropertyType DWord -Force | Out-Null
    }
    function Find-OnDiscs($File) {
      Get-PSDrive -PSProvider FileSystem | ForEach-Object { Join-Path $_.Root $File } |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    }

    # ── Telemetry, ads, AI (machine-wide policies) ──
    $p = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows'
    Set-Reg "$p\DataCollection" AllowTelemetry 0                 # Pro treats 0 as "Required only"
    Set-Reg "$p\DataCollection" AllowDeviceNameInTelemetry 0
    Set-Reg "$p\DataCollection" DoNotShowFeedbackNotifications 1
    Set-Reg "$p\AdvertisingInfo" DisabledByGroupPolicy 1
    Set-Reg "$p\System" EnableActivityFeed 0
    Set-Reg "$p\System" PublishUserActivities 0
    Set-Reg "$p\System" UploadUserActivities 0
    Set-Reg "$p\CloudContent" DisableWindowsConsumerFeatures 1
    Set-Reg "$p\CloudContent" DisableConsumerAccountStateContent 1
    Set-Reg "$p\CloudContent" DisableCloudOptimizedContent 1
    Set-Reg "$p\WindowsCopilot" TurnOffWindowsCopilot 1
    Set-Reg "$p\WindowsAI" DisableAIDataAnalysis 1
    Set-Reg "$p\WindowsAI" AllowRecallEnablement 0
    Set-Reg "$p\Windows Search" AllowCortana 0
    Set-Reg "$p\Windows Search" DisableWebSearch 1
    Set-Reg "$p\Windows Search" ConnectedSearchUseWeb 0
    Set-Reg "$p\LocationAndSensors" DisableLocation 1
    Set-Reg "$p\Windows Error Reporting" Disabled 1
    Set-Reg "$p\DeliveryOptimization" DODownloadMode 0           # no sharing updates with other PCs
    Set-Reg "$p\TabletPC" PreventHandwritingDataSharing 1
    Set-Reg "$p\OneDrive" DisableFileSyncNGSC 1
    Set-Reg 'HKLM:\SOFTWARE\Policies\Microsoft\Dsh' AllowNewsAndInterests 0
    Set-Reg 'HKLM:\SOFTWARE\Policies\Microsoft\InputPersonalization' AllowInputPersonalization 0

    # ── This user ──
    $u = 'HKCU:\Software\Microsoft\Windows\CurrentVersion'
    Set-Reg "$u\AdvertisingInfo" Enabled 0
    Set-Reg "$u\Privacy" TailoredExperiencesWithDiagnosticDataEnabled 0
    foreach ($k in 'SilentInstalledAppsEnabled', 'SystemPaneSuggestionsEnabled', 'SoftLandingEnabled',
                   'SubscribedContent-338388Enabled', 'SubscribedContent-338389Enabled',
                   'SubscribedContent-353694Enabled', 'SubscribedContent-353696Enabled') {
      Set-Reg "$u\ContentDeliveryManager" $k 0
    }
    Set-Reg "$u\Explorer\Advanced" Start_TrackProgs 0
    Set-Reg "$u\Explorer\Advanced" ShowCopilotButton 0
    Set-Reg 'HKCU:\Software\Policies\Microsoft\Windows\Explorer' DisableSearchBoxSuggestions 1
    Set-Reg 'HKCU:\Software\Policies\Microsoft\Windows\WindowsCopilot' TurnOffWindowsCopilot 1

    # ── Telemetry services and tasks ──
    foreach ($s in 'DiagTrack', 'dmwappushservice') {
      Stop-Service $s -Force -ErrorAction SilentlyContinue
      Set-Service $s -StartupType Disabled -ErrorAction SilentlyContinue
    }
    foreach ($t in '\Microsoft\Windows\Application Experience\', '\Microsoft\Windows\Customer Experience Improvement Program\', '\Microsoft\Windows\Feedback\Siuf\') {
      Get-ScheduledTask -TaskPath $t -ErrorAction SilentlyContinue | Disable-ScheduledTask -ErrorAction SilentlyContinue | Out-Null
    }

    # ── Preinstalled apps (Xbox apps stay, for Game Pass in the gaming VM) ──
    $apps = 'Microsoft.Copilot', 'Microsoft.BingNews', 'Microsoft.BingWeather', 'Microsoft.BingSearch',
            'Microsoft.GetHelp', 'Microsoft.Getstarted', 'Microsoft.MicrosoftOfficeHub',
            'Microsoft.MicrosoftSolitaireCollection', 'Microsoft.People', 'Microsoft.PowerAutomateDesktop',
            'Microsoft.Todos', 'Microsoft.WindowsFeedbackHub', 'Microsoft.YourPhone', 'Microsoft.ZuneMusic',
            'Microsoft.ZuneVideo', 'Clipchamp.Clipchamp', 'MicrosoftCorporationII.QuickAssist',
            'Microsoft.OutlookForWindows', 'Microsoft.Windows.DevHome', 'MSTeams', 'Microsoft.549981C3F5F10'
    foreach ($a in $apps) {
      Get-AppxPackage -AllUsers -Name $a | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue
      Get-AppxProvisionedPackage -Online | Where-Object DisplayName -eq $a |
        Remove-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue | Out-Null
    }
    $od = "$env:SystemRoot\System32\OneDriveSetup.exe"
    if (Test-Path $od) { Start-Process $od '/uninstall' -Wait }

    # ── virtio drivers (network, display, ...), QEMU guest agent, SPICE agent ──
    $gt = Find-OnDiscs 'virtio-win-guest-tools.exe'
    if ($gt) { Start-Process $gt '/install /quiet /norestart' -Wait }
    ${
      if winapps then
        ''

          # ── WinApps: RDP RemoteApp settings from the WINAPPS CD ──
          $wa = Find-OnDiscs 'RDPApps.reg'
          if ($wa) { Start-Process cmd.exe -ArgumentList '/c', (Join-Path (Split-Path $wa) 'install.bat') -Wait }
        ''
      else
        ""
    }
    # Own password at the next login, then restart to finish the drivers.
    net user "${user}" /logonpasswordchg:yes
    Stop-Transcript
    shutdown /r /t 60 /c "Setup finished. Restarting in a minute; at the next login you pick your own password."
  '';
in
pkgs.runCommand "unattend-${computerName}.iso" { nativeBuildInputs = [ pkgs.xorriso ]; } ''
  mkdir cd
  cp ${pkgs.writeText "autounattend.xml" xml} cd/autounattend.xml
  cp ${pkgs.writeText "setup.ps1" ps1} cd/setup.ps1
  xorriso -as mkisofs -quiet -J -R -V UNATTEND -o $out cd
''
