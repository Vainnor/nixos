pragma Singleton
import QtQuick
import Quickshell
import qs.services

// Which overlay is open (one at a time) and on which screen.
// Names: "control", "notifications", "launcher", "power", and the bar
// dropdowns "calendar", "cpu", "mem", "net". Empty = none.
// Keybinds reach this through the IPC handlers in shell.qml:
//   qs ipc call panels toggle launcher
Singleton {
    id: root
    property string open: ""
    property string arg: ""          // e.g. the launcher tab to open on
    property var screen: null        // ShellScreen the overlay shows on
    property real anchorX: -1        // centre x of the bar module that opened a dropdown

    function screenByName(name) {
        for (const s of Quickshell.screens) if (s.name === name) return s;
        return null;
    }
    function primary() {
        for (const s of Quickshell.screens) if (Theme.primaryModel !== "" && s.model === Theme.primaryModel) return s;
        return Quickshell.screens[0] ?? null;
    }
    // The focused monitor (from Mango), else the primary one.
    function focusedScreen() {
        return screenByName(Mango.focusedMonitor) ?? primary();
    }

    function toggle(name, arg) {
        if (open === name && (arg ?? "") === root.arg) { close(); return; }
        show(name, arg);
    }
    function show(name, arg) {
        root.arg = arg ?? "";
        root.screen = focusedScreen();
        root.open = name;
    }
    function close() { open = ""; arg = ""; }
}
