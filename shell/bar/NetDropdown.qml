import QtQuick
import QtQuick.Layouts
import qs.common
import qs.services

// Under the bar's net module: link kind, interface, IPv4 (and NetBird's wt0),
// the last minute of traffic, totals, and link speed or Wi-Fi signal.
// Never the Wi-Fi network's name. Polls addresses and signal only while open;
// the traffic history comes from the bar's own 2 s sampling.
Dropdown {
    id: root
    name: "net"
    title: "network"
    subtitle: !Network.connected ? "offline" : (Network.wired ? "wired" : "wifi")

    readonly property color downColor: Theme.accent
    readonly property color upColor: Theme.cyan
    readonly property int slots: 30
    readonly property real peak: Math.max(1024, ...Network.history.map(s => Math.max(s.rx, s.tx)))

    Timer {
        interval: 5000
        repeat: true
        triggeredOnStart: true
        running: root.visible
        onTriggered: Network.detail()
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        StatRow {
            Layout.fillWidth: true
            visible: Network.device !== ""
            label: "interface"
            value: Network.device
        }
        Repeater {
            model: Network.addrs
            delegate: StatRow {
                required property var modelData
                Layout.fillWidth: true
                label: modelData.dev === "wt0" ? "netbird" : "ipv4"
                value: modelData.addr
            }
        }
        StatRow {
            Layout.fillWidth: true
            visible: Network.connected && Network.wired && Network.speed !== ""
            label: "link"
            value: Network.speed + "bit/s"
        }
        StatRow {
            Layout.fillWidth: true
            visible: Network.connected && !Network.wired && Network.signal >= 0
            label: "signal"
            value: Network.signal + "% · " + Network.signalDbm + " dbm"
            valueColor: Network.signal < 40 ? Theme.orange : Theme.text
        }
    }

    // Last 60 s: download up from the middle, upload down from it, one shared scale.
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        RowLayout {
            Layout.fillWidth: true
            Txt { Layout.fillWidth: true; text: "60s"; color: Theme.muted; font.pixelSize: Theme.fontSmall }
            Txt { text: "peak " + Network.fmtRate(root.peak).trim() + "/s"; color: Theme.muted; font.pixelSize: Theme.fontSmall }
        }
        Rectangle {
            id: graph
            Layout.fillWidth: true
            Layout.preferredHeight: 64
            color: Theme.surface

            Repeater {
                model: root.slots
                delegate: Item {
                    id: slot
                    required property int index
                    readonly property var s: Network.history[Network.history.length - root.slots + index] ?? null
                    readonly property real half: graph.height / 2 - 1
                    x: Math.round(index * graph.width / root.slots)
                    width: Math.round((index + 1) * graph.width / root.slots) - x - 2
                    height: graph.height
                    Rectangle {
                        anchors { left: parent.left; right: parent.right; bottom: parent.verticalCenter }
                        height: slot.s ? Math.ceil(slot.half * Math.min(1, slot.s.rx / root.peak)) : 0
                        color: root.downColor
                    }
                    Rectangle {
                        anchors { left: parent.left; right: parent.right; top: parent.verticalCenter }
                        height: slot.s ? Math.ceil(slot.half * Math.min(1, slot.s.tx / root.peak)) : 0
                        color: root.upColor
                    }
                }
            }
            Rectangle {
                anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
                height: 1
                color: Theme.raised
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        StatRow {
            Layout.fillWidth: true
            swatch: root.downColor
            label: "down"
            value: Network.fmtRate(Network.rx) + "/s · " + Network.fmtBytes(Network.rxTotal).padStart(6, " ")
        }
        StatRow {
            Layout.fillWidth: true
            swatch: root.upColor
            label: "up"
            value: Network.fmtRate(Network.tx) + "/s · " + Network.fmtBytes(Network.txTotal).padStart(6, " ")
        }
        Txt {
            Layout.alignment: Qt.AlignRight
            text: "rate · total since boot"
            color: Theme.muted
            font.pixelSize: Theme.fontSmall
        }
    }
}
