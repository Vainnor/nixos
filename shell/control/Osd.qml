import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import qs.common
import qs.services

// Volume / mic OSD: bottom-centre of the focused screen, click-through,
// shown on Audio.osdRequested and hidden 1.5 s later. Skipped while the
// control center is open (its sliders already show the change).
PanelWindow {
    id: root

    property string kind: "volume"   // "volume" | "mic"
    property bool shown: false

    readonly property bool isMic: kind === "mic"
    readonly property bool off: isMic ? Audio.micMuted : Audio.muted
    readonly property real value: isMic ? Audio.micVolume : Audio.volume

    visible: shown
    screen: Panels.primary()
    anchors.bottom: true
    margins.bottom: 80
    implicitWidth: 260
    implicitHeight: box.implicitHeight
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    mask: Region {}
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "vshell-osd"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

    Connections {
        target: Audio
        function onOsdRequested(kind) {
            if (Panels.open === "control") return;
            root.kind = kind;
            if (!root.shown) root.screen = Panels.focusedScreen();
            root.shown = true;
            hideTimer.restart();
        }
    }

    Timer {
        id: hideTimer
        interval: 1500
        onTriggered: root.shown = false
    }

    Rectangle {
        id: box
        anchors.fill: parent
        implicitHeight: row.implicitHeight + 16
        color: Theme.bg
        border.color: Theme.border
        border.width: Theme.borderWidth

        RowLayout {
            id: row
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: Theme.pad; rightMargin: Theme.pad }
            spacing: Theme.padSmall

            Txt {
                text: root.isMic ? "mic" : "vol"
                color: root.off ? Theme.red : Theme.text
            }
            Level {
                Layout.fillWidth: true
                interactive: false
                value: root.off ? 0 : root.value
            }
            Txt {
                Layout.preferredWidth: 40
                horizontalAlignment: Text.AlignRight
                text: root.off ? (root.isMic ? "off" : "muted") : Math.round(root.value * 100)
                color: root.off ? Theme.red : Theme.text
            }
        }
    }
}
