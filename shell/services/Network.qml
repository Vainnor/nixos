pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// NetworkManager state from nmcli: polled every 5 s and refreshed on
// `nmcli monitor` events. Wired wins over Wi-Fi. Download and upload rates
// (bytes/s) of the device in use, from its kernel counters every 2 s, with
// the last minute of them for the net dropdown. That dropdown calls detail()
// while open for addresses and Wi-Fi signal.
Singleton {
    id: root

    property bool connected: false
    property bool wired: false
    property string device: ""
    property string ssid: ""        // Wi-Fi connection name (usually the SSID)
    property string speed: ""       // link speed of the wired device, e.g. "2.5g"
    property real rx: 0             // download, bytes per second
    property real tx: 0             // upload, bytes per second
    property real rxTotal: 0        // bytes received by the device since it came up
    property real txTotal: 0
    property var history: []        // last 30 samples (60 s), oldest first: [{ rx, tx }]

    // ---- detail, only refreshed by detail()
    property var addrs: []          // [{ dev, addr }] IPv4 of the device in use, then wt0 (NetBird)
    property int signal: -1         // Wi-Fi link quality, percent; -1 = unknown
    property int signalDbm: 0
    // The bar's text: the link kind and its traffic, never the network's name.
    readonly property string label: !connected ? "offline"
        : (wired ? "eth" : "wifi") + " " + fmtRate(rx) + "↓ " + fmtRate(tx) + "↑"

    // 1234567 -> "1.2m", 34000 -> " 33k" (at most 4 characters), fixed width so the bar doesn't jitter.
    function fmtRate(b) {
        let v = b, u = "b";
        for (const next of ["k", "m", "g"]) {
            if (v < 1000) break;
            v /= 1024; u = next;
        }
        const n = v < 10 && u !== "b" ? v.toFixed(1) : Math.round(v).toString();
        return (n + u).padStart(4, " ");
    }

    // 12345678901 -> "11.5g", 52428800 -> "50.0m"
    function fmtBytes(b) {
        let v = b, u = "b";
        for (const next of ["k", "m", "g", "t"]) {
            if (v < 1024) break;
            v /= 1024; u = next;
        }
        return (u === "b" ? Math.round(v).toString() : v.toFixed(1)) + u;
    }

    property real lastRx: -1
    property real lastTx: -1
    property real lastAt: 0
    property string countersOf: ""

    function sample() {
        if (device === "") { rx = 0; tx = 0; lastRx = -1; return; }
        const r = Number(rxFile.text().trim()), t = Number(txFile.text().trim());
        const now = Date.now();
        if (isNaN(r) || isNaN(t)) return;
        if (countersOf === device && lastRx >= 0 && now > lastAt) {
            const dt = (now - lastAt) / 1000;
            rx = Math.max(0, (r - lastRx) / dt);
            tx = Math.max(0, (t - lastTx) / dt);
        } else {
            rx = 0; tx = 0;
        }
        countersOf = device;
        lastRx = r; lastTx = t; lastAt = now;
        rxTotal = r; txTotal = t;
        history = history.concat([{ rx: rx, tx: tx }]).slice(-30);
    }

    function detail() {
        if (!detailProc.running) detailProc.running = true;
    }

    // "4: wlp15s0    inet 100.70.21.135/25 brd ..." and the /proc/net/wireless table
    // ("wlp15s0: 0000   54.  -56.  -256 ..."): quality out of 70, level in dBm.
    Process {
        id: detailProc
        command: ["sh", "-c", "ip -4 -o addr show 2>/dev/null; cat /proc/net/wireless 2>/dev/null"]
        stdout: StdioCollector {
            onStreamFinished: {
                const mine = [], wt = [];
                let q = -1, dbm = 0;
                for (const line of text.split("\n")) {
                    const f = line.trim().split(/\s+/);
                    if (f[2] === "inet") {
                        const a = { dev: f[1], addr: f[3].split("/")[0] };
                        if (f[1] === root.device) mine.push(a);
                        else if (f[1] === "wt0") wt.push(a);
                    } else if (f[0] === root.device + ":" && f.length > 3) {
                        q = Math.round(parseFloat(f[2]) / 70 * 100);
                        dbm = Math.round(parseFloat(f[3]));
                    }
                }
                root.addrs = mine.concat(wt);
                root.signal = root.wired || isNaN(q) ? -1 : Math.max(0, Math.min(100, q));
                root.signalDbm = isNaN(dbm) ? 0 : dbm;
            }
        }
    }

    // Read in place on each tick (blockLoading), so both counters are current.
    FileView {
        id: rxFile
        blockLoading: true
        path: root.device === "" ? "" : "/sys/class/net/" + root.device + "/statistics/rx_bytes"
    }
    FileView {
        id: txFile
        blockLoading: true
        path: root.device === "" ? "" : "/sys/class/net/" + root.device + "/statistics/tx_bytes"
    }

    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: {
            rxFile.reload();
            txFile.reload();
            root.sample();
        }
    }

    // Split one `nmcli -t` line on unescaped colons.
    function fields(line) {
        const out = [];
        let cur = "";
        for (let i = 0; i < line.length; i++) {
            const c = line[i];
            if (c === "\\" && i + 1 < line.length) { cur += line[++i]; continue; }
            if (c === ":") { out.push(cur); cur = ""; continue; }
            cur += c;
        }
        out.push(cur);
        return out;
    }

    function fmtSpeed(mbit) {
        const v = parseInt(mbit);
        if (isNaN(v) || v <= 0) return "";
        if (v >= 1000) return (v / 1000).toString().replace(/\.0+$/, "") + "g";
        return v + "m";
    }

    function parse(text) {
        let eth = null, wifi = null;
        const speeds = {};
        for (const line of text.split("\n")) {
            if (line === "") continue;
            const f = fields(line);
            if (f[0] === "SPEED") { speeds[f[1]] = f[2]; continue; }
            const [type, state, conn, dev] = f;
            if (!state || !state.startsWith("connected")) continue;
            if (type === "ethernet" && !eth) eth = { conn, dev };
            else if (type === "wifi" && !wifi) wifi = { conn, dev };
        }
        const pick = eth ?? wifi;
        root.connected = pick !== null;
        root.wired = eth !== null;
        root.device = pick?.dev ?? "";
        root.ssid = wifi?.conn ?? "";
        root.speed = eth ? fmtSpeed(speeds[eth.dev]) : "";
    }

    Process {
        id: poll
        command: ["sh", "-c",
            "nmcli -t -f TYPE,STATE,CONNECTION,DEVICE device 2>/dev/null; " +
            "for d in /sys/class/net/*; do printf 'SPEED:%s:%s\\n' \"${d##*/}\" \"$(cat \"$d/speed\" 2>/dev/null)\"; done"]
        stdout: StdioCollector {
            onStreamFinished: root.parse(text)
        }
    }

    function refresh() { if (!poll.running) poll.running = true; }

    Timer {
        interval: 5000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.refresh()
    }

    // Coalesce bursts of monitor lines into one refresh.
    Timer {
        id: soon
        interval: 300
        onTriggered: root.refresh()
    }

    Process {
        id: monitor
        command: ["nmcli", "monitor"]
        running: true
        stdout: SplitParser {
            onRead: soon.restart()
        }
        onExited: restartMonitor.start()
    }
    Timer {
        id: restartMonitor
        interval: 3000
        onTriggered: monitor.running = true
    }
}
