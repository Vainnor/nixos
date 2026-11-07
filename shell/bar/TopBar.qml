import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Widgets
import Quickshell.Services.SystemTray
import qs.common
import qs.services

// The top bar, one per screen (shell.qml sets `screen`).
// left: tags of this screen's monitor + focused title · center: clock ·
// right: tray, vol, net, cpu, mem, notifications, power.
PanelWindow {
    id: root

    readonly property string monitor: screen?.name ?? ""
    readonly property bool focused: Mango.focusedMonitor === monitor
    readonly property var tags: Mango.tagsFor(monitor)

    anchors { top: true; left: true; right: true }
    implicitHeight: Theme.barHeight
    exclusionMode: ExclusionMode.Auto
    color: Theme.bg
    WlrLayershell.namespace: "vshell-bar"
    WlrLayershell.layer: WlrLayer.Top

    // Open a panel on the screen whose bar was clicked.
    function toggle(name) {
        Panels.toggle(name);
        if (Panels.open === name) Panels.screen = root.screen;
    }

    // Next/previous tag on this monitor, wrapping 1..9.
    function stepTag(up) {
        const n = root.tags.length || 9;
        const cur = Mango.activeTag(root.monitor) || 1;
        const next = up ? (cur - 2 + n) % n + 1 : cur % n + 1;
        Mango.view(next, root.monitor);
    }

    // One calendar overlay for the whole shell (it follows Panels.screen).
    LazyLoader {
        active: root.screen === Panels.primary()
        Calendar {}
    }

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    Item {
        id: content
        anchors { fill: parent; bottomMargin: Theme.borderWidth; leftMargin: Theme.gap; rightMargin: Theme.gap }

        // ---- left: tags + title
        Row {
            id: tagRow
            anchors { left: parent.left; verticalCenter: parent.verticalCenter }
            spacing: Theme.gap

            Repeater {
                model: root.tags
                delegate: BarModule {
                    required property var modelData
                    readonly property bool shown: modelData.index <= 4 || modelData.active || modelData.urgent || modelData.clients > 0
                    visible: shown
                    width: Math.max(implicitWidth, Theme.moduleHeight)
                    text: String(modelData.index)
                    active: modelData.active
                    textColor: modelData.urgent ? Theme.onAccent : (modelData.clients > 0 ? Theme.text : Theme.muted)
                    color: modelData.active ? Theme.accent
                        : modelData.urgent ? Theme.red
                        : hovered ? Theme.raised : Theme.surface
                    bold: modelData.urgent
                    onClicked: Mango.view(modelData.index, root.monitor)
                    onScrolled: up => root.stepTag(up)
                }
            }
        }

        Txt {
            id: title
            visible: root.focused && text !== ""
            anchors { left: tagRow.right; leftMargin: Theme.gap + 2; verticalCenter: parent.verticalCenter }
            width: Math.max(0, Math.min(implicitWidth, clockModule.x - x - Theme.gap * 3))
            text: Mango.focusedTitle
            color: Theme.dim
        }

        // ---- center: clock
        BarModule {
            id: clockModule
            anchors { horizontalCenter: parent.horizontalCenter; verticalCenter: parent.verticalCenter }
            text: clock.date.toLocaleString(Qt.locale("en_US"), "ddd dd MMM · hh:mm").toLowerCase()
            textColor: Theme.accent
            onClicked: root.toggle("calendar")
        }

        // ---- right: tray + status modules
        Row {
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: Theme.gap

            BarModule {
                visible: SystemTray.items.values.length > 0
                interactive: false

                Repeater {
                    model: SystemTray.items
                    delegate: Item {
                        id: trayItem
                        required property var modelData
                        width: 16
                        height: 16
                        anchors.verticalCenter: parent?.verticalCenter

                        IconImage {
                            id: icon
                            anchors.fill: parent
                            implicitSize: 16
                            source: trayItem.modelData.icon
                        }

                        QsMenuAnchor {
                            id: menu
                            menu: trayItem.modelData.menu
                            anchor.window: root
                        }

                        function openMenu() {
                            if (!modelData.hasMenu) return;
                            const p = trayItem.mapToItem(null, 0, 0);
                            menu.anchor.rect.x = p.x;
                            menu.anchor.rect.y = Theme.barHeight;
                            menu.anchor.rect.width = trayItem.width;
                            menu.anchor.rect.height = 1;
                            menu.open();
                        }

                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -3
                            acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                            cursorShape: Qt.PointingHandCursor
                            onClicked: e => {
                                if (e.button === Qt.RightButton || trayItem.modelData.onlyMenu) trayItem.openMenu();
                                else if (e.button === Qt.MiddleButton) trayItem.modelData.secondaryActivate();
                                else trayItem.modelData.activate();
                            }
                            onWheel: e => trayItem.modelData.scroll(e.angleDelta.y, false)
                        }
                    }
                }
            }

            BarModule {
                text: Audio.muted ? "vol muted" : "vol " + Math.round(Audio.volume * 100) + "%"
                textColor: Audio.muted ? Theme.red : Theme.text
                onClicked: root.toggle("control")
                onScrolled: up => up ? Audio.raise() : Audio.lower()
            }

            BarModule {
                text: Network.label
                textColor: Network.connected ? Theme.text : Theme.red
                onClicked: root.toggle("control")
            }

            BarModule {
                text: "cpu " + Math.round(SysStats.cpu) + "%"
                interactive: false
            }

            BarModule {
                text: "mem " + Math.round(SysStats.mem) + "%"
                interactive: false
            }

            BarModule {
                text: Notifs.dnd ? "[ dnd ]" : (Notifs.count > 0 ? "[ " + Notifs.count + " ]" : "[ ]")
                textColor: Theme.accent
                onClicked: root.toggle("notifications")
                onRightClicked: Notifs.dnd = !Notifs.dnd
            }

            BarModule {
                text: "[ off ]"
                textColor: Theme.red
                onClicked: root.toggle("power")
            }
        }
    }

    // 2px bottom line
    Rectangle {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Theme.borderWidth
        color: Theme.border
    }
}
