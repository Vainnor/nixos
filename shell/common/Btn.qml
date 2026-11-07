import QtQuick

// Flat square button: surface fill, accent fill when active, red text when danger.
// Usage: Btn { text: "[ clear ]"; onClicked: ... }
Rectangle {
    id: root
    property string text: ""
    property bool active: false
    property bool danger: false
    property bool bold: active
    property int padX: Theme.padSmall
    property int padY: 4
    property alias hovered: mouse.containsMouse
    signal clicked
    signal rightClicked

    implicitWidth: label.implicitWidth + padX * 2
    implicitHeight: label.implicitHeight + padY * 2
    color: active ? Theme.accent : (mouse.containsMouse ? Theme.raised : Theme.surface)

    Txt {
        id: label
        anchors.centerIn: parent
        text: root.text
        font.bold: root.bold
        color: root.active ? Theme.onAccent : (root.danger ? Theme.red : Theme.text)
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
