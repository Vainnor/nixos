import QtQuick
import QtQuick.Layouts
import qs.common
import qs.services

// Under the bar's mem module: used / total, available, cache, swap (zram)
// and the processes holding the most resident memory. Polls only while open.
Dropdown {
    id: root
    name: "mem"
    title: "memory"
    subtitle: Math.round(SysStats.mem) + "%"

    // 1288490188 -> "1.20g", 52428800 -> " 50m"
    function fmt(bytes) {
        const g = bytes / 1073741824;
        return g >= 1 ? g.toFixed(2) + "g" : (Math.round(bytes / 1048576) + "m").padStart(5, " ");
    }

    Timer {
        interval: 2000
        repeat: true
        triggeredOnStart: true
        running: root.visible
        onTriggered: SysStats.detail()
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        StatRow {
            Layout.fillWidth: true
            label: "used"
            level: SysStats.mem / 100
            value: SysStats.memGb.toFixed(1) + " / " + SysStats.memTotalGb.toFixed(1) + "g"
        }
        StatRow {
            Layout.fillWidth: true
            visible: SysStats.swapTotalGb > 0
            label: SysStats.zram ? "zram" : "swap"
            level: SysStats.swapTotalGb > 0 ? SysStats.swapUsedGb / SysStats.swapTotalGb : 0
            value: SysStats.swapUsedGb.toFixed(1) + " / " + SysStats.swapTotalGb.toFixed(1) + "g"
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 4
        StatRow {
            Layout.fillWidth: true
            label: "available"
            value: SysStats.memAvailGb.toFixed(1) + "g"
        }
        StatRow {
            Layout.fillWidth: true
            label: "buff/cache"
            value: SysStats.memCacheGb.toFixed(1) + "g"
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
            model: SysStats.topMem
            delegate: StatRow {
                required property var modelData
                Layout.fillWidth: true
                label: modelData.name
                value: root.fmt(modelData.value)
            }
        }
    }
}
