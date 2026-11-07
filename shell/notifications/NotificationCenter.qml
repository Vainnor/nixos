import QtQuick
import QtQuick.Layouts
import Quickshell
import qs.common
import qs.services

// The notification center: every stored notification, newest first, with
// do-not-disturb and clear. Opened with `panels toggle notifications`.
Overlay {
    id: root
    name: "notifications"

    Panel {
        id: panel
        anchors { top: parent.top; right: parent.right; margins: Theme.panelGap }
        width: 420
        title: "notifications " + Notifs.count
        actions: [
            Btn {
                text: "dnd"
                active: Notifs.dnd
                onClicked: Notifs.dnd = !Notifs.dnd
            },
            Btn {
                text: "[ clear ]"
                onClicked: Notifs.clear()
            }
        ]

        Txt {
            visible: Notifs.count === 0
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            horizontalAlignment: Text.AlignHCenter
            text: "no notifications"
            color: Theme.muted
        }

        Item {
            visible: Notifs.count > 0
            Layout.fillWidth: true
            // Panel chrome is ~34 (header) + 2 * pad + border; keep the whole panel under 70%.
            Layout.preferredHeight: Math.min(view.contentHeight, root.height * 0.7 - 34 - Theme.pad * 2 - Theme.borderWidth * 2)

            ListView {
                id: view
                anchors { fill: parent; rightMargin: scrollbar.visible ? scrollbar.width + Theme.gap : 0 }
                clip: true
                spacing: Theme.pad
                boundsBehavior: Flickable.StopAtBounds
                model: ScriptModel { values: Notifs.list }
                delegate: NotificationCard {
                    required property var modelData
                    width: view.width
                    notification: modelData
                }
            }

            // Thin square scrollbar, only when the list overflows.
            Rectangle {
                id: scrollbar
                visible: view.visibleArea.heightRatio < 1
                anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
                width: 3
                color: Theme.raised
                Rectangle {
                    width: parent.width
                    y: view.visibleArea.yPosition * parent.height
                    height: Math.max(12, view.visibleArea.heightRatio * parent.height)
                    color: Theme.border
                }
            }
        }
    }
}
