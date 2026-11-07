pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// CPU and memory usage (percent, 0..100) and uptime, polled from /proc.
Singleton {
    id: root

    property real cpu: 0
    property real mem: 0
    property string uptime: ""

    property real lastTotal: 0
    property real lastIdle: 0

    Timer {
        interval: 2500
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            stat.reload();
            meminfo.reload();
            uptimeFile.reload();
        }
    }

    // First line: "cpu  user nice system idle iowait irq softirq steal ..."
    FileView {
        id: stat
        path: "/proc/stat"
        onLoaded: {
            const f = text().split("\n")[0].trim().split(/\s+/).slice(1).map(Number);
            if (f.length < 4) return;
            const idle = f[3] + (f[4] ?? 0);
            const total = f.slice(0, 8).reduce((a, b) => a + b, 0);
            const dt = total - root.lastTotal;
            const di = idle - root.lastIdle;
            if (root.lastTotal > 0 && dt > 0)
                root.cpu = Math.max(0, Math.min(100, (1 - di / dt) * 100));
            root.lastTotal = total;
            root.lastIdle = idle;
        }
    }

    FileView {
        id: meminfo
        path: "/proc/meminfo"
        onLoaded: {
            const t = text();
            const total = Number((t.match(/MemTotal:\s+(\d+)/) ?? [])[1] ?? 0);
            const avail = Number((t.match(/MemAvailable:\s+(\d+)/) ?? [])[1] ?? 0);
            if (total > 0) root.mem = (1 - avail / total) * 100;
        }
    }

    // "12345.67 ..." seconds since boot -> "3d 2h", "2h 14m", "5m"
    FileView {
        id: uptimeFile
        path: "/proc/uptime"
        onLoaded: {
            const s = Math.floor(Number(text().split(" ")[0]) || 0);
            const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), m = Math.floor(s % 3600 / 60);
            root.uptime = d > 0 ? d + "d " + h + "h" : (h > 0 ? h + "h " + m + "m" : m + "m");
        }
    }
}
