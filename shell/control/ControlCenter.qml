import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Bluetooth
import qs.common
import qs.services

// Control center: quick toggles, output/input, per-app volume, media and
// session buttons. Opens top-right under the bar.
Overlay {
    id: root
    name: "control"

    function run(cmd) {
        Panels.close();
        Quickshell.execDetached(cmd);
    }

    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property int btConnected: adapter ? adapter.devices.values.filter(d => d.connected).length : 0

    // MPRIS position isn't pushed; poll it while the panel shows a player.
    Timer {
        interval: 1000
        repeat: true
        running: root.visible && Media.player !== null && Media.player.isPlaying
        onTriggered: Media.player.positionChanged()
    }

    Panel {
        anchors { top: parent.top; right: parent.right; margins: Theme.panelGap }
        width: 400
        title: "control"
        subtitle: "up " + SysStats.uptime

        // 1. Tiles
        GridLayout {
            Layout.fillWidth: true
            columns: 3
            columnSpacing: Theme.gap
            rowSpacing: Theme.gap
            uniformCellWidths: true

            Tile {
                Layout.fillWidth: true
                title: "network"
                subtitle: !Network.connected ? "offline" : (Network.wired ? ("wired" + (Network.speed !== "" ? " · " + Network.speed : "")) : "wifi")
                active: Network.connected
                onClicked: root.run(["nm-connection-editor"])
            }
            Tile {
                Layout.fillWidth: true
                title: "bluetooth"
                subtitle: !root.adapter ? "none" : (!root.adapter.enabled ? "off" : (root.btConnected === 1 ? "1 device" : root.btConnected + " devices"))
                active: root.adapter?.enabled ?? false
                onClicked: if (root.adapter) root.adapter.enabled = !root.adapter.enabled
                onRightClicked: root.run(["blueman-manager"])
            }
            Tile {
                Layout.fillWidth: true
                title: "do not disturb"
                subtitle: Notifs.dnd ? "on" : "off"
                active: Notifs.dnd
                onClicked: Notifs.dnd = !Notifs.dnd
            }
            Tile {
                Layout.fillWidth: true
                title: "night light"
                subtitle: NightLight.enabled ? "on" : "off"
                active: NightLight.enabled
                onClicked: NightLight.toggle()
            }
            Tile {
                Layout.fillWidth: true
                title: "keep awake"
                subtitle: KeepAwake.enabled ? "on" : "off"
                active: KeepAwake.enabled
                onClicked: KeepAwake.toggle()
            }
            Tile {
                // Accent = the mic is muted (the state you need to notice).
                Layout.fillWidth: true
                title: Audio.micMuted ? "mic muted" : "mic"
                subtitle: Audio.micMuted ? "muted" : "live"
                active: Audio.micMuted
                onClicked: Audio.toggleMicMute()
            }
        }

        // 2. Output
        DevicePicker {
            Layout.fillWidth: true
            label: "output"
            current: Audio.sink
            groups: [{ title: "channels", nodes: Audio.channels }, { title: "devices", nodes: Audio.sinks }]
            onPicked: node => Audio.setDefaultSink(node)

            Txt {
                text: "[ mixer ]"
                color: mixerMouse.containsMouse ? Theme.text : Theme.muted
                font.pixelSize: Theme.fontSmall
                MouseArea {
                    id: mixerMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.run(["pwvucontrol"])
                }
            }
        }
        SliderRow {
            Layout.fillWidth: true
            name: "volume"
            value: Audio.volume
            muted: Audio.muted
            onMoved: v => Audio.setVolume(v)
            onNameClicked: Audio.toggleMute()
        }

        // Input
        DevicePicker {
            Layout.fillWidth: true
            label: "input"
            current: Audio.source
            groups: [{ title: "devices", nodes: Audio.sources }]
            onPicked: node => Audio.setDefaultSource(node)
        }
        SliderRow {
            Layout.fillWidth: true
            name: "mic"
            value: Audio.micVolume
            muted: Audio.micMuted
            onMoved: v => Audio.setMicVolume(v)
            onNameClicked: Audio.toggleMicMute()
        }

        // 3. Apps
        ColumnLayout {
            Layout.fillWidth: true
            visible: Audio.streams.length > 0
            spacing: 6
            Txt {
                text: "apps"
                color: Theme.muted
                font.pixelSize: Theme.fontSmall
            }
            Repeater {
                model: Audio.streams
                delegate: SliderRow {
                    required property var modelData
                    readonly property var target: Audio.streamTarget(modelData)
                    Layout.fillWidth: true
                    nameWidth: 90
                    name: Audio.appName(modelData)
                    tag: target && Audio.isChannel(target) ? Audio.nodeName(target) : ""
                    value: modelData.audio?.volume ?? 0
                    muted: modelData.audio?.muted ?? false
                    onMoved: v => Audio.setStreamVolume(modelData, v)
                    onNameClicked: Audio.toggleStreamMute(modelData)
                }
            }
        }

        // 4. Media
        Rectangle {
            id: media
            readonly property var p: Media.player
            Layout.fillWidth: true
            visible: p !== null
            implicitHeight: mediaRow.implicitHeight + Theme.padSmall * 2
            color: Theme.surface

            RowLayout {
                id: mediaRow
                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: Theme.padSmall }
                spacing: Theme.padSmall

                Rectangle {
                    Layout.preferredWidth: 38
                    Layout.preferredHeight: 38
                    color: Theme.raised
                    clip: true
                    Image {
                        anchors.fill: parent
                        source: media.p?.trackArtUrl ?? ""
                        fillMode: Image.PreserveAspectCrop
                        sourceSize { width: 76; height: 76 }
                        asynchronous: true
                        visible: status === Image.Ready
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Txt {
                        Layout.fillWidth: true
                        text: (media.p?.trackTitle || "nothing playing").toLowerCase()
                    }
                    Txt {
                        Layout.fillWidth: true
                        color: Theme.muted
                        text: [media.p?.trackArtist ?? "", (media.p?.identity ?? "")].filter(s => s !== "").join(" · ").toLowerCase()
                    }
                    Level {
                        Layout.fillWidth: true
                        Layout.topMargin: 2
                        value: Media.progress
                        interactive: media.p?.canSeek ?? false
                        onMoved: v => Media.seekTo(v)
                    }
                }

                Row {
                    spacing: Theme.gap
                    Btn { text: "‹"; onClicked: Media.previous() }
                    Btn {
                        text: media.p?.isPlaying ? "❚❚" : "▶"
                        active: media.p?.isPlaying ?? false
                        onClicked: Media.togglePlaying()
                    }
                    Btn { text: "›"; onClicked: Media.next() }
                }
            }
        }

        // 5. Wallpaper (also Super+W)
        Btn { Layout.fillWidth: true; text: "[ wallpaper ]"; onClicked: Panels.show("wallpaper") }

        // 6. Session
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            Btn { Layout.fillWidth: true; Layout.preferredWidth: 1; text: "[ lock ]"; onClicked: { Panels.close(); Session.lock(); } }
            Btn { Layout.fillWidth: true; Layout.preferredWidth: 1; text: "[ sleep ]"; onClicked: { Panels.close(); Session.suspend(); } }
            Btn { Layout.fillWidth: true; Layout.preferredWidth: 1; text: "[ reboot ]"; onClicked: { Panels.close(); Session.reboot(); } }
            Btn { Layout.fillWidth: true; Layout.preferredWidth: 1; text: "[ off ]"; danger: true; onClicked: { Panels.close(); Session.poweroff(); } }
        }
    }
}
