import QtQuick

// Quick-settings tile: title + small status line; accent fill when on.
Rectangle {
    id: root
    property string title: ""
    property string subtitle: ""
    property bool active: false
    signal clicked
    signal rightClicked

    implicitHeight: col.implicitHeight + 14
    color: active ? Theme.accent : (mouse.containsMouse ? Theme.raised : Theme.surface)

    Column {
        id: col
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: Theme.padSmall }
        spacing: 2
        Txt { width: parent.width; text: root.title; color: root.active ? Theme.onAccent : Theme.text }
        Txt {
            width: parent.width; text: root.subtitle; font.pixelSize: Theme.fontSmall
            color: root.active ? Theme.onAccent : Theme.muted; opacity: root.active ? 0.75 : 1
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: e => e.button === Qt.RightButton ? root.rightClicked() : root.clicked()
    }
}
