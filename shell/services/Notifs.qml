pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Services.Notifications

// The notification daemon. Every notification is tracked and kept in `list`
// (newest first, capped at `maxHistory`) until dismissed or cleared. New ones
// also go to `popups` for a while, unless do-not-disturb is on (critical ones
// still pop up).
Singleton {
    id: root

    property var list: []      // Notification objects, newest first
    readonly property int count: list.length
    property alias dnd: persist.dnd
    property var popups: []    // currently shown as pop-ups, newest first
    readonly property int maxPopups: 4
    readonly property int maxHistory: 50

    // Updated every 30 s so relative times ("2m") refresh.
    property double now: Date.now()

    // id -> arrival time (ms), id -> remaining pop-up time (ms), id -> hovered
    property var times: ({})
    property var remaining: ({})
    property var held: ({})

    PersistentProperties {
        id: persist
        reloadableId: "vshell-notifs"
        property bool dnd: false
        property string times: "{}"   // arrival times survive reloads
        // Merge the saved times over any recorded before the reload finished.
        onLoaded: {
            try {
                const old = JSON.parse(persist.times);
                for (const id in old) root.times[id] = old[id];
            } catch (e) {}
            root.saveTimes();
            root.now = Date.now();
        }
    }

    NotificationServer {
        id: server
        keepOnReload: true
        persistenceSupported: true
        bodySupported: true
        bodyMarkupSupported: false
        actionsSupported: true
        imageSupported: true

        onNotification: n => root.add(n)
    }

    Timer {
        interval: 30000
        running: true
        repeat: true
        onTriggered: root.now = Date.now()
    }

    // Counts down the visible pop-ups; hovered ones are paused.
    Timer {
        id: ticker
        interval: 250
        repeat: true
        running: root.popups.length > 0
        onTriggered: {
            const done = [];
            const shown = root.popups.slice(0, root.maxPopups);
            for (const n of shown) {
                if (root.held[n.id] || root.remaining[n.id] === undefined) continue;
                root.remaining[n.id] -= interval;
                if (root.remaining[n.id] <= 0) done.push(n);
            }
            for (const n of done) root.unpop(n, true);
        }
    }

    function add(n) {
        n.tracked = true;
        if (times[n.id] === undefined) {
            times[n.id] = Date.now();
            saveTimes();
        }
        n.closed.connect(() => root.forget(n));

        // Highest id = newest; ids only grow, so this also orders notifications
        // carried over from before a reload.
        const l = list.filter(x => x !== n);
        let i = 0;
        while (i < l.length && l[i].id > n.id) i++;
        l.splice(i, 0, n);
        const extra = l.splice(maxHistory);
        list = l;
        for (const old of extra) old.expire();

        if (n.lastGeneration) return;
        const critical = n.urgency === NotificationUrgency.Critical;
        if (dnd && !critical) {
            if (n.transient) n.expire();
            return;
        }
        const t = timeoutFor(n);
        if (t > 0) remaining[n.id] = t;
        popups = [n].concat(popups.filter(x => x !== n));
    }

    // Pop-up time in ms, or 0 for "until dismissed".
    function timeoutFor(n) {
        if (n.expireTimeout > 0) return n.expireTimeout * 1000;
        if (n.urgency === NotificationUrgency.Critical) return 0;
        if (n.urgency === NotificationUrgency.Low) return 3000;
        return 5000;
    }

    // Take a notification off the screen but keep it in the list.
    function unpop(n, timedOut) {
        popups = popups.filter(x => x !== n);
        delete remaining[n.id];
        delete held[n.id];
        if (timedOut && n.transient) n.expire();
    }

    // Pause / resume a pop-up's timer (on hover).
    function hold(n, on) {
        if (!n) return;
        if (on) held[n.id] = true;
        else delete held[n.id];
    }

    // Called when the notification closes for any reason.
    function forget(n) {
        list = list.filter(x => x !== n);
        popups = popups.filter(x => x !== n);
        delete remaining[n.id];
        delete held[n.id];
        if (times[n.id] !== undefined) {
            delete times[n.id];
            saveTimes();
        }
    }

    function saveTimes() {
        persist.times = JSON.stringify(times);
    }

    function dismiss(n) {
        if (n) n.dismiss();
    }

    function clear() {
        for (const n of list.slice()) n.dismiss();
        list = [];
        popups = [];
    }

    // Arrival time of a notification (ms since epoch).
    function timeOf(n) {
        return n ? (times[n.id] ?? now) : now;
    }

    // "now", "2m", "1h", "3d" (uses `now`, so bindings refresh every 30 s).
    function ago(n) {
        const s = Math.max(0, (now - timeOf(n)) / 1000);
        if (s < 60) return "now";
        if (s < 3600) return Math.floor(s / 60) + "m";
        if (s < 86400) return Math.floor(s / 3600) + "h";
        return Math.floor(s / 86400) + "d";
    }

    // The action whose identifier is "default", or null.
    function defaultAction(n) {
        if (!n) return null;
        const acts = n.actions;
        for (let i = 0; i < acts.length; i++) if (acts[i].identifier === "default") return acts[i];
        return null;
    }

    // Plain text from a body that may carry markup or entities.
    function plain(s) {
        return (s ?? "")
            .replace(/<br\s*\/?>/gi, "\n")
            .replace(/<[^>]*>/g, "")
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">")
            .replace(/&quot;/g, "\"")
            .replace(/&apos;|&#39;/g, "'")
            .replace(/&nbsp;/g, " ")
            .replace(/&amp;/g, "&")
            .trim();
    }
}
