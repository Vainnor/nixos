import QtQuick
import QtQuick.Layouts

// One line of a detail panel: dim label, value on the right (the font is
// monospaced, so numbers keep their place). With `level` (0..1) a flat bar
// sits between them; `swatch` puts a colour key before the label.
//   StatRow { label: "load"; value: "1.20 0.98 0.75" }
//   StatRow { label: "used"; level: 0.4; value: "12.1 / 31.2g" }
RowLayout {
    id: root
    property string label: ""
    property string value: ""
    property color valueColor: Theme.text
    property real level: -1
    property color fill: Theme.accent
    property color swatch: "transparent"
    property int labelWidth: 64      // with a level bar

    spacing: Theme.padSmall

    Rectangle {
        visible: root.swatch.a > 0
        implicitWidth: 8
        implicitHeight: 8
        color: root.swatch
    }
    Txt {
        Layout.fillWidth: root.level < 0
        Layout.preferredWidth: root.level < 0 ? -1 : root.labelWidth
        text: root.label
        color: Theme.dim
    }
    Level {
        visible: root.level >= 0
        Layout.fillWidth: true
        interactive: false
        value: root.level
        fill: root.fill
    }
    Txt {
        horizontalAlignment: Text.AlignRight
        text: root.value
        color: root.valueColor
    }
}
