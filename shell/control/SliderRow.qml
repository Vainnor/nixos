import QtQuick
import QtQuick.Layouts
import qs.common

// One mixer row: name, level bar, number. Muted rows show a muted-colour
// fill and a red `mute`. `tag` is an optional small note (e.g. the channel
// an app plays into) between the bar and the number.
RowLayout {
    id: root
    property string name: ""
    property string tag: ""
    property real value: 0
    property bool muted: false
    property bool interactive: true
    property int nameWidth: 46
    signal moved(real v)
    signal nameClicked

    spacing: Theme.padSmall

    Txt {
        Layout.preferredWidth: root.nameWidth
        text: root.name
        color: nameMouse.containsMouse ? Theme.text : Theme.dim
        MouseArea {
            id: nameMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.nameClicked()
        }
    }
    Level {
        Layout.fillWidth: true
        value: root.value
        interactive: root.interactive
        fill: root.muted ? Theme.muted : Theme.accent
        onMoved: v => root.moved(v)
    }
    Txt {
        visible: root.tag !== ""
        Layout.preferredWidth: 48
        text: root.tag
        color: Theme.muted
        font.pixelSize: Theme.fontSmall
    }
    Txt {
        Layout.preferredWidth: 30
        horizontalAlignment: Text.AlignRight
        text: root.muted ? "mute" : Math.round(root.value * 100)
        color: root.muted ? Theme.red : Theme.text
    }
}
