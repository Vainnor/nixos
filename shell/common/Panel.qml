import QtQuick
import QtQuick.Layouts

// A bordered panel with a header row (accent title on the left, `actions` on
// the right) and a padded column body. Children go into the body.
//   Panel { title: "control"; actions: [ Btn { text: "[ clear ]" } ]; Txt { text: "..." } }
Rectangle {
    id: root
    property string title: ""
    property string subtitle: ""   // dim text on the right when there are no actions
    property alias actions: actionRow.data
    default property alias body: bodyCol.data
    property int bodySpacing: Theme.pad

    color: Theme.bg
    border.color: Theme.border
    border.width: Theme.borderWidth
    implicitWidth: 380
    implicitHeight: header.height + bodyCol.implicitHeight + Theme.pad * 2 + Theme.borderWidth

    // Swallow clicks on the panel itself, so only clicks outside close an Overlay.
    MouseArea { anchors.fill: parent; z: -1 }

    Item {
        id: header
        visible: root.title !== ""
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.borderWidth }
        height: visible ? 34 : 0
        Txt { anchors { left: parent.left; leftMargin: Theme.pad; verticalCenter: parent.verticalCenter } text: root.title; color: Theme.accent; font.bold: true }
        Row {
            id: actionRow
            anchors { right: parent.right; rightMargin: Theme.pad - 4; verticalCenter: parent.verticalCenter }
            spacing: Theme.gap
        }
        Txt { visible: actionRow.children.length === 0; anchors { right: parent.right; rightMargin: Theme.pad; verticalCenter: parent.verticalCenter } text: root.subtitle; color: Theme.muted }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: Theme.borderWidth; color: Theme.border; anchors.leftMargin: -Theme.borderWidth; anchors.rightMargin: -Theme.borderWidth }
    }

    ColumnLayout {
        id: bodyCol
        anchors { left: parent.left; right: parent.right; top: header.bottom; margins: Theme.pad }
        spacing: root.bodySpacing
    }
}
