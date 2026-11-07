//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Io
import qs.common
import qs.services
import qs.bar
import qs.notifications
import qs.control
import qs.launcher

// vshell: the desktop shell (bar, notifications, control center, OSD,
// launcher, power menu, lock screen). Square and flat; colours from Theme.
ShellRoot {
    Variants {
        model: Quickshell.screens
        delegate: Component {
            TopBar {
                required property var modelData
                screen: modelData
            }
        }
    }

    NotificationPopups {}
    NotificationCenter {}
    ControlCenter {}
    Osd {}
    Launcher {}
    PowerMenu {}
    LockScreen {}

    // Keybinds and scripts talk to the shell through these:
    //   vshell ipc call panels toggle control
    //   vshell ipc call panels toggleTab launcher clipboard
    IpcHandler {
        target: "panels"
        function toggle(name: string): void { Panels.toggle(name, ""); }
        function toggleTab(name: string, tab: string): void { Panels.toggle(name, tab); }
        function show(name: string): void { Panels.show(name, ""); }
        function close(): void { Panels.close(); }
    }
    IpcHandler {
        target: "audio"
        function raise(): void { Audio.raise(); }
        function lower(): void { Audio.lower(); }
        function mute(): void { Audio.toggleMute(); }
        function micMute(): void { Audio.toggleMicMute(); }
    }
    IpcHandler {
        target: "notifications"
        function clear(): void { Notifs.clear(); }
        function dnd(): void { Notifs.dnd = !Notifs.dnd; }
    }
    IpcHandler {
        target: "lock"
        function lock(): void { Session.lock(); }
    }
}
