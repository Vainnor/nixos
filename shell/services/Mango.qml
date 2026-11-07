pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Mango (mangowm) state from `mmsg watch …` streams, plus dispatch helpers.
// Each watcher prints one JSON object per line; a watcher that exits is
// restarted after a short delay.
Singleton {
    id: root

    property string focusedMonitor: ""   // output name of the selected monitor
    property string focusedTitle: ""
    property string focusedAppId: ""
    property var monitors: []             // monitor names, as Mango reports them
    property var tags: ({})               // monitor name -> [{index, active, urgent, clients}]

    function tagsFor(monitorName) {
        return root.tags[monitorName] ?? [];
    }

    // The lowest active tag on a monitor (0 if none).
    function activeTag(monitorName) {
        for (const t of tagsFor(monitorName)) if (t.active) return t.index;
        return 0;
    }

    // Switch to a tag. With a monitor name other than the selected one, the
    // monitor is focused first (viewcrossmon). The trailing 0 mirrors the
    // user's binds (`view,1,0`).
    function view(tag, monitorName) {
        if (monitorName && monitorName !== root.focusedMonitor)
            dispatch("viewcrossmon," + tag + ",^" + monitorName + "$");
        else
            dispatch("view," + tag + ",0");
    }

    // Run any Mango function: dispatch("killclient") / dispatch("view,3,0").
    function dispatch(str) {
        Quickshell.execDetached(["mmsg", "dispatch", str]);
    }

    function parse(line) {
        try {
            return JSON.parse(line);
        } catch (e) {
            return null;
        }
    }

    component Watcher: Process {
        id: proc
        property string what
        signal json(var obj)
        command: ["mmsg", "watch", what]
        running: true
        stdout: SplitParser {
            onRead: data => {
                const obj = root.parse(data);
                if (obj) proc.json(obj);
            }
        }
        onExited: proc.restart.start()
        property Timer restart: Timer {
            interval: 2000
            onTriggered: proc.running = true
        }
    }

    Watcher {
        what: "all-tags"
        onJson: obj => {
            const out = {};
            for (const m of obj.all_tags ?? []) {
                out[m.monitor] = (m.tags ?? []).map(t => ({
                    index: t.index,
                    active: !!t.is_active,
                    urgent: !!t.is_urgent,
                    clients: t.client_count ?? 0
                }));
            }
            root.tags = out;
        }
    }

    Watcher {
        what: "focusing-client"
        onJson: obj => {
            root.focusedTitle = obj.title ?? "";
            root.focusedAppId = obj.appid ?? "";
        }
    }

    Watcher {
        what: "all-monitors"
        onJson: obj => {
            const names = [];
            for (const m of obj.monitors ?? []) {
                names.push(m.name);
                if (m.active) root.focusedMonitor = m.name;
            }
            root.monitors = names;
        }
    }
}
