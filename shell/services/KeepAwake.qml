pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Wayland

// Keep the session from going idle (no lock, no screen-off).
// Uses the Wayland idle-inhibit protocol, which swayidle honours through the
// compositor (logind inhibitors would not stop swayidle's timeouts). Mango only
// honours inhibitors on visible surfaces, so while enabled we map a 1x1,
// transparent, click-through layer surface and attach the inhibitor to it.
Singleton {
    id: root
    property bool enabled: false

    function toggle() { root.enabled = !root.enabled; }

    PanelWindow {
        id: anchorWindow
        visible: root.enabled
        anchors { top: true; left: true }
        implicitWidth: 1
        implicitHeight: 1
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        mask: Region {}
        WlrLayershell.layer: WlrLayer.Background
        WlrLayershell.namespace: "vshell-keepawake"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }

    IdleInhibitor {
        window: anchorWindow
        enabled: root.enabled
    }
}
