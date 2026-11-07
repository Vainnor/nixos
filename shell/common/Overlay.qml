import QtQuick
import Quickshell
import Quickshell.Wayland

// A full-screen, transparent layer window for one panel. Clicking outside the
// content or pressing Escape closes it. Put the panel inside and position it
// with anchors (e.g. anchors.top/right + margins Theme.panelGap).
//   Overlay { name: "control"; Panel { anchors.right: parent.right; ... } }
PanelWindow {
    id: root
    property string name
    property bool keyboard: true      // take keyboard focus while open
    default property alias content: holder.data

    visible: Panels.open === name
    screen: Panels.screen
    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Normal   // stays below the bar's reserved space
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "vshell-" + name
    WlrLayershell.keyboardFocus: visible && keyboard ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    MouseArea {
        anchors.fill: parent
        onClicked: Panels.close()
    }

    Item {
        id: holder
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: Panels.close()
    }
}
