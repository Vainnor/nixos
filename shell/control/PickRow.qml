import QtQuick
import qs.common

// A full-width selectable row in a device list; accent fill when selected.
Rectangle {
    id: root
    property string text: ""
    property string note: ""
    property bool selected: false
    signal clicked

    implicitHeight: label.implicitHeight + 10
    color: selected ? Theme.accent : (mouse.containsMouse ? Theme.raised : Theme.surface)

    Txt {
        id: label
        anchors { left: parent.left; right: noteTxt.left; leftMargin: Theme.padSmall; rightMargin: Theme.padSmall; verticalCenter: parent.verticalCenter }
        text: root.text
        font.bold: root.selected
        color: root.selected ? Theme.onAccent : Theme.text
    }
    Txt {
        id: noteTxt
        anchors { right: parent.right; rightMargin: Theme.padSmall; verticalCenter: parent.verticalCenter }
        text: root.note
        font.pixelSize: Theme.fontSmall
        color: root.selected ? Theme.onAccent : Theme.muted
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
