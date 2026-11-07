import QtQuick
import qs.common

// One bar module: a flat surface block, moduleHeight tall, raised on hover.
// Shows `text`, or custom children (which replace the label) laid out in a row.
//   BarModule { text: "cpu 12%"; onClicked: ... }
Rectangle {
    id: root
    property string text: ""
    property color textColor: Theme.text
    property bool bold: false
    property bool active: false          // accent fill, bg-coloured bold text
    property bool interactive: true
    property alias hovered: mouse.containsMouse
    default property alias content: row.data
    signal clicked
    signal rightClicked
    signal scrolled(bool up)

    implicitHeight: Theme.moduleHeight
    implicitWidth: (row.children.length > 0 ? row.implicitWidth : label.implicitWidth) + Theme.padSmall * 2
    color: active ? Theme.accent : (interactive && mouse.containsMouse ? Theme.raised : Theme.surface)

    Txt {
        id: label
        visible: row.children.length === 0
        anchors.centerIn: parent
        text: root.text
        font.bold: root.bold || root.active
        color: root.active ? Theme.onAccent : root.textColor
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        enabled: root.interactive
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: e => e.button === Qt.RightButton ? root.rightClicked() : root.clicked()
        onWheel: e => {
            if (e.angleDelta.y !== 0) root.scrolled(e.angleDelta.y > 0);
        }
    }

    // Above the MouseArea so custom children (e.g. tray icons) get their own input.
    Row {
        id: row
        anchors.centerIn: parent
        spacing: Theme.gap
    }
}
