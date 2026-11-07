import QtQuick

// A flat level bar (0..1): raised track, accent fill. Draggable/scrollable
// when `interactive`; emits moved(v) and leaves storing the value to the owner.
Item {
    id: root
    property real value: 0
    property bool interactive: true
    property color fill: Theme.accent
    property real step: 0.05
    signal moved(real v)

    implicitHeight: 14
    implicitWidth: 120

    Rectangle {
        id: track
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
        height: 6
        color: Theme.raised
        Rectangle {
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            width: parent.width * Math.max(0, Math.min(1, root.value))
            color: root.fill
        }
    }

    MouseArea {
        anchors.fill: parent
        enabled: root.interactive
        cursorShape: Qt.PointingHandCursor
        function set(x) { root.moved(Math.max(0, Math.min(1, x / width))) }
        onPressed: e => set(e.x)
        onPositionChanged: e => { if (pressed) set(e.x) }
        onWheel: e => root.moved(Math.max(0, Math.min(1, root.value + (e.angleDelta.y > 0 ? root.step : -root.step))))
    }
}
