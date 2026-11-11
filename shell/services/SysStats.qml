pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// CPU usage (percent, 0..100, also per core), memory and swap (GiB) and
// uptime, polled from /proc for the bar. The cpu and mem dropdowns call
// detail() while open for the rest: clock, temperature, load, top processes.
Singleton {
    id: root

    property real cpu: 0
    property var cores: []    // per-core usage, 0..100
    property real mem: 0      // percent in use
    property real memGb: 0    // GiB in use (total minus available)
    property real memTotalGb: 0
    property real memAvailGb: 0
    property real memCacheGb: 0   // buffers + page cache + reclaimable slab, like free(1)
    property real swapTotalGb: 0
    property real swapUsedGb: 0
    property string uptime: ""

    property real lastTotal: 0
    property real lastIdle: 0
    property var lastCores: []

    // ---- detail, only refreshed by detail()
    property real freqAvg: 0      // GHz over all cores
    property real freqMax: 0
    property real temp: -1        // °C, k10temp Tctl; -1 = unknown
    property string load: ""      // "1.20 0.98 0.75"
    property bool zram: false     // swap is on zram
    property var topCpu: []       // [{ pid, name, value }] value: % of the whole machine
    property var topMem: []       // value: resident bytes

    property string tempPath: ""  // found by name on the first run, not a fixed hwmonN
    property var lastTicks: ({})
    property real lastProcTotal: 0
    property real lastProcAt: 0

    function detail() {
        if (!detailProc.running) detailProc.running = true;
    }

    // One shot: T <tctl path> <millidegrees>, L <loadavg>, F <kHz per core>,
    // Z <zram swaps>, S <cpu line of /proc/stat>, then P <pid> <ticks> <rss pages> <comm>
    // for every process (cat skips the ones that exit mid-read).
    Process {
        id: detailProc
        command: ["sh", "-c", `
tp=$1
if [ -z "$tp" ]; then
  for h in /sys/class/hwmon/hwmon*; do
    [ "$(cat "$h/name" 2>/dev/null)" = k10temp ] || continue
    for l in "$h"/temp*_label; do [ "$(cat "$l")" = Tctl ] && tp=\${l%_label}_input; done
  done
fi
echo "T $tp $(cat "$tp" 2>/dev/null)"
echo "L $(cat /proc/loadavg)"
echo "F $(cat /sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq 2>/dev/null | tr '\\n' ' ')"
echo "Z $(grep -c zram /proc/swaps)"
echo "S $(head -1 /proc/stat)"
cat /proc/[0-9]*/stat 2>/dev/null | awk '{ match($0, /\\(.*\\)/); c = substr($0, RSTART + 1, RLENGTH - 2); split(substr($0, RSTART + RLENGTH + 1), f, " "); print "P", $1, f[12] + f[13], f[22], c }'
`, "sh", root.tempPath]
        stdout: StdioCollector {
            onStreamFinished: root.parseDetail(text)
        }
    }

    // A second sample soon after the first, so process usage shows right away.
    Timer {
        id: again
        interval: 600
        onTriggered: root.detail()
    }

    function parseDetail(text) {
        const now = Date.now();
        const procs = [];
        let total = 0;
        for (const line of text.split("\n")) {
            const sp = line.indexOf(" ");
            const tag = line.slice(0, sp), rest = line.slice(sp + 1);
            if (tag === "T") {
                const [path, milli] = rest.split(" ");
                root.tempPath = path ?? "";
                root.temp = milli ? Number(milli) / 1000 : -1;
            } else if (tag === "L") {
                root.load = rest.split(" ").slice(0, 3).join(" ");
            } else if (tag === "F") {
                const f = rest.trim().split(/\s+/).map(Number).filter(v => v > 0);
                if (f.length > 0) {
                    root.freqAvg = f.reduce((a, b) => a + b, 0) / f.length / 1e6;
                    root.freqMax = Math.max(...f) / 1e6;
                }
            } else if (tag === "Z") {
                root.zram = Number(rest) > 0;
            } else if (tag === "S") {
                total = rest.trim().split(/\s+/).slice(1, 9).map(Number).reduce((a, b) => a + b, 0);
            } else if (tag === "P") {
                const p = rest.split(" ");
                procs.push({ pid: p[0], ticks: Number(p[1]), rss: Number(p[2]) * 4096, name: p.slice(3).join(" ") });
            }
        }

        // Usage needs two samples close together; a stale one (dropdown was closed) starts over.
        const fresh = root.lastProcTotal > 0 && now - root.lastProcAt < 5000 && total > root.lastProcTotal;
        const dt = total - root.lastProcTotal;
        const ticks = {};
        const cpu = [];
        for (const p of procs) {
            ticks[p.pid] = p.ticks;
            const prev = root.lastTicks[p.pid];
            if (fresh && prev !== undefined && p.ticks > prev)
                cpu.push({ pid: p.pid, name: p.name, value: (p.ticks - prev) / dt * 100 });
        }
        cpu.sort((a, b) => b.value - a.value);
        root.topCpu = fresh ? cpu.slice(0, 5) : [];
        if (!fresh) again.restart();
        root.lastTicks = ticks;
        root.lastProcTotal = total;
        root.lastProcAt = now;

        procs.sort((a, b) => b.rss - a.rss);
        root.topMem = procs.slice(0, 5).map(p => ({ pid: p.pid, name: p.name, value: p.rss }));
    }

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
        // Then one "cpuN ..." line per core.
        onLoaded: {
            const lines = text().split("\n");
            const usage = (f, last) => {
                const idle = f[3] + (f[4] ?? 0);
                const total = f.slice(0, 8).reduce((a, b) => a + b, 0);
                const dt = total - (last?.total ?? 0), di = idle - (last?.idle ?? 0);
                const pct = last && dt > 0 ? Math.max(0, Math.min(100, (1 - di / dt) * 100)) : 0;
                return { total, idle, pct };
            };
            const f = lines[0].trim().split(/\s+/).slice(1).map(Number);
            if (f.length < 4) return;
            const all = usage(f, root.lastTotal > 0 ? { total: root.lastTotal, idle: root.lastIdle } : null);
            if (root.lastTotal > 0) root.cpu = all.pct;
            root.lastTotal = all.total;
            root.lastIdle = all.idle;

            const cores = [], last = [];
            for (let i = 1; i < lines.length && /^cpu\d/.test(lines[i]); i++) {
                const u = usage(lines[i].trim().split(/\s+/).slice(1).map(Number), root.lastCores[i - 1]);
                cores.push(u.pct);
                last.push({ total: u.total, idle: u.idle });
            }
            root.cores = cores;
            root.lastCores = last;
        }
    }

    FileView {
        id: meminfo
        path: "/proc/meminfo"
        onLoaded: {
            const t = text();
            const kb = key => Number((t.match(new RegExp(key + ":\\s+(\\d+)")) ?? [])[1] ?? 0);
            const total = kb("MemTotal"), avail = kb("MemAvailable");
            if (total > 0) {
                root.mem = (1 - avail / total) * 100;
                root.memGb = (total - avail) / 1048576;
                root.memTotalGb = total / 1048576;
                root.memAvailGb = avail / 1048576;
                root.memCacheGb = (kb("Buffers") + kb("Cached") + kb("SReclaimable")) / 1048576;
                root.swapTotalGb = kb("SwapTotal") / 1048576;
                root.swapUsedGb = (kb("SwapTotal") - kb("SwapFree")) / 1048576;
            }
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
