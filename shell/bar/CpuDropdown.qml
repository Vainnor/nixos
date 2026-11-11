import QtQuick
import QtQuick.Layouts
import qs.common
import qs.services

// Under the bar's cpu module: total and per-core usage, clock, Tctl
// temperature, load, uptime and the busiest processes. Polls only while open.
Dropdown {
    id: root
    name: "cpu"
    title: "cpu"
    subtitle: SysStats.cores.length + " threads"

    Timer {
        interval: 2000
        repeat: true
        triggeredOnStart: true
        running: root.visible
        onTriggered: SysStats.detail()
    }

    StatRow {
        Layout.fillWidth: true
        label: "usage"
        level: SysStats.cpu / 100
        value: (Math.round(SysStats.cpu) + "%").padStart(4, " ")
    }

    // One flat column per core, filled from the bottom.
    RowLayout {
        Layout.fillWidth: true
        Layout.preferredHeight: 36
        spacing: 3
        Repeater {
            model: SysStats.cores
            delegate: Rectangle {
                required property real modelData
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: Theme.raised
                Rectangle {
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                    height: Math.round(parent.height * Math.max(0, Math.min(1, parent.modelData / 100)))
                    color: parent.modelData >= 90 ? Theme.orange : Theme.accent
                }
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        StatRow {
            Layout.fillWidth: true
            label: "clock"
            value: SysStats.freqAvg > 0 ? SysStats.freqAvg.toFixed(2) + " avg · " + SysStats.freqMax.toFixed(2) + " max ghz" : "–"
        }
        StatRow {
            Layout.fillWidth: true
            label: "temp"
            value: SysStats.temp >= 0 ? SysStats.temp.toFixed(1) + "°c" : "–"
            valueColor: SysStats.temp >= 90 ? Theme.red : (SysStats.temp >= 80 ? Theme.orange : Theme.text)
        }
        StatRow {
            Layout.fillWidth: true
            label: "load"
            value: SysStats.load || "–"
        }
        StatRow {
            Layout.fillWidth: true
            label: "uptime"
            value: SysStats.uptime
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        Txt {
            text: "top"
            color: Theme.muted
            font.pixelSize: Theme.fontSmall
        }
        Repeater {
            model: SysStats.topCpu
            delegate: StatRow {
                required property var modelData
                Layout.fillWidth: true
                label: modelData.name
                value: modelData.value.toFixed(1).padStart(4, " ") + "%"
            }
        }
        Txt {
            visible: SysStats.topCpu.length === 0
            text: "…"
            color: Theme.muted
        }
    }
}
