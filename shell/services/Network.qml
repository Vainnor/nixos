pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// NetworkManager state from nmcli: polled every 5 s and refreshed on
// `nmcli monitor` events. Wired wins over Wi-Fi.
Singleton {
    id: root

    property bool connected: false
    property bool wired: false
    property string device: ""
    property string ssid: ""        // Wi-Fi connection name (usually the SSID)
    property string speed: ""       // link speed of the wired device, e.g. "2.5g"
    readonly property string label: !connected ? "offline" : (wired ? "eth" : ("wifi " + ssid).trim())

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
