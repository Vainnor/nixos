import QtQuick
import Quickshell
import qs.common
import qs.services

// Power menu: a centered row of large square buttons. Left/Right or h/l move,
// Enter runs, 1–6 run directly, Escape closes.
Overlay {
    id: root
    name: "power"

    readonly property var actions: [
        { label: "lock", danger: false },
        { label: "sleep", danger: false },
        { label: "logout", danger: false },
        { label: "reboot", danger: false },
        { label: "windows", danger: false },
        { label: "off", danger: true }
    ]
    property int current: 0

    function run(i) {
        Panels.close();
        switch (i) {
        case 0: return Session.lock();
        case 1: return Session.suspend();
        case 2: return Session.logout();
        case 3: return Session.reboot();
        case 4: return Session.rebootToWindows();
        case 5: return Session.poweroff();
        }
    }

    Connections {
        target: Panels
        function onOpenChanged() {
            if (Panels.open === root.name) {
                root.current = 0;
                keys.forceActiveFocus();
            }
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: row.implicitWidth + Theme.pad * 2 + Theme.borderWidth * 2
        height: row.implicitHeight + Theme.pad * 2 + Theme.borderWidth * 2
        color: Theme.bg
        border.color: Theme.border
        border.width: Theme.borderWidth

        // Swallow clicks on the box so only clicks outside close the overlay.
        MouseArea { anchors.fill: parent }

        Item {
            id: keys
            anchors.fill: parent
            focus: true
            Keys.onPressed: event => {
                const n = root.actions.length;
                if (event.key === Qt.Key_Left || event.key === Qt.Key_H || event.key === Qt.Key_Backtab) {
                    root.current = (root.current - 1 + n) % n;
                } else if (event.key === Qt.Key_Right || event.key === Qt.Key_L || event.key === Qt.Key_Tab) {
                    root.current = (root.current + 1) % n;
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                    root.run(root.current);
                } else if (event.key >= Qt.Key_1 && event.key < Qt.Key_1 + n) {
                    root.run(event.key - Qt.Key_1);
                } else {
                    return;      // Escape → Overlay closes
                }
                event.accepted = true;
            }
        }

        Row {
            id: row
            anchors.centerIn: parent
            spacing: Theme.gap
            Repeater {
                model: root.actions
                delegate: Btn {
                    required property var modelData
                    required property int index
                    width: 76
                    height: 64
                    text: modelData.label
                    danger: modelData.danger
                    active: index === root.current
                    onHoveredChanged: if (hovered) root.current = index
                    onClicked: root.run(index)

                    Txt {
                        anchors { right: parent.right; top: parent.top; margins: 4 }
                        text: index + 1
                        font.pixelSize: Theme.fontSmall
                        color: parent.active ? Theme.onAccent : Theme.muted
                    }
                }
            }
        }
    }
}
