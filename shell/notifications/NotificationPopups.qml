import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.common
import qs.services

// Pop-up notifications, top-right of the primary screen below the bar. The
// window is sized to its cards and masked to them, so it never eats clicks.
// Hovering a card pauses its timeout. Hidden while the notification center
// is open.
PanelWindow {
    id: root

    screen: Panels.primary()
    visible: Notifs.popups.length > 0 && Panels.open !== "notifications"
    anchors { top: true; right: true }
    margins { top: Theme.panelGap; right: Theme.panelGap }
    exclusiveZone: 0
    focusable: false
    color: "transparent"
    implicitWidth: 400
    implicitHeight: Math.max(1, cards.implicitHeight)
    mask: Region { item: cards }
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "vshell-notifications"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

    Column {
        id: cards
        width: parent.width
        spacing: Theme.gap

        Repeater {
            model: ScriptModel {
                values: Notifs.popups.slice(0, Notifs.maxPopups)
            }
            delegate: Rectangle {
                id: frame
                required property var modelData
                width: cards.width
                height: card.implicitHeight + Theme.borderWidth * 2
                color: Theme.bg
                border.color: Theme.border
                border.width: Theme.borderWidth

                NotificationCard {
                    id: card
                    anchors { fill: parent; margins: Theme.borderWidth }
                    notification: frame.modelData
                    onHoveredChanged: Notifs.hold(frame.modelData, hovered)
                }

                transform: Translate { id: shift }
                Component.onCompleted: enter.start()
                ParallelAnimation {
                    id: enter
                    NumberAnimation { target: shift; property: "x"; from: 40; to: 0; duration: Theme.anim; easing.type: Easing.OutCubic }
                    NumberAnimation { target: frame; property: "opacity"; from: 0; to: 1; duration: Theme.anim }
                }
            }
        }
    }
}
