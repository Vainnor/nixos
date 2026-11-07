import QtQuick
import QtQuick.Layouts
import qs.common
import qs.services

// "output"/"input" block: small label, the current device row with a muted
// `change ▾`, and an inline list of choices when expanded.
// groups: [{ title: "channels", nodes: [...] }, { title: "devices", nodes: [...] }]
ColumnLayout {
    id: root
    property string label: ""
    property var current: null
    property var groups: []
    property bool expanded: false
    default property alias extra: labelRow.data   // extra items on the label row's right
    signal picked(var node)

    spacing: 4

    RowLayout {
        id: labelRow
        Layout.fillWidth: true
        spacing: Theme.gap
        Txt {
            Layout.fillWidth: true
            text: root.label
            color: Theme.muted
            font.pixelSize: Theme.fontSmall
        }
    }

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: cur.implicitHeight + 10
        color: devMouse.containsMouse ? Theme.raised : Theme.surface
        Txt {
            id: cur
            anchors { left: parent.left; right: change.left; leftMargin: Theme.padSmall; rightMargin: Theme.padSmall; verticalCenter: parent.verticalCenter }
            text: Audio.nodeName(root.current)
        }
        Txt {
            id: change
            anchors { right: parent.right; rightMargin: Theme.padSmall; verticalCenter: parent.verticalCenter }
            text: root.expanded ? "close ▴" : "change ▾"
            color: Theme.muted
        }
        MouseArea {
            id: devMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.expanded = !root.expanded
        }
    }

    Repeater {
        model: root.expanded ? root.groups.filter(g => g.nodes.length > 0) : []
        delegate: ColumnLayout {
            id: group
            required property var modelData
            Layout.fillWidth: true
            spacing: 2
            Txt {
                visible: root.groups.filter(g => g.nodes.length > 0).length > 1
                text: group.modelData.title
                color: Theme.muted
                font.pixelSize: Theme.fontSmall
                Layout.topMargin: 2
            }
            Repeater {
                model: group.modelData.nodes
                delegate: PickRow {
                    required property var modelData
                    Layout.fillWidth: true
                    text: Audio.nodeName(modelData)
                    selected: modelData === root.current
                    onClicked: {
                        root.picked(modelData);
                        root.expanded = false;
                    }
                }
            }
        }
    }
}
