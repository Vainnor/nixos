import QtQuick
import Quickshell
import Quickshell.Io
import qs.common

// The launcher: `❯ query▌`, tabs (apps clipboard emoji calc files) and a
// results list. Tab / Shift+Tab or Ctrl+1..5 switch tabs, Up/Down or
// Ctrl+J/K move, Enter runs, Escape closes. Panels.arg picks the first tab:
//   qs ipc call panels toggle launcher emoji
Overlay {
    id: root
    name: "launcher"

    readonly property var tabs: ["apps", "clipboard", "emoji", "calc", "files"]
    readonly property int rowHeight: 26
    readonly property int maxRows: 8
    readonly property int limit: 100
    readonly property string home: Quickshell.env("HOME") ?? ""

    property int tab: 0
    readonly property string mode: tabs[tab]
    property string query: ""
    // Rows: { text, detail, icon (theme icon), glyph (text icon), data }
    property var results: []
    property int current: 0
    property string status: ""          // muted line when there are no rows

    // ---- state per mode ----
    property var history: ({})          // desktop entry id -> launch count
    property var clipLines: []          // raw `cliphist list` lines
    property var emojis: []             // [[name, emoji], ...]
    property string calcResult: ""
    property string calcFor: ""
    property var files: []
    property string filesFor: ""

    // ---- opening ----
    function opened() {
        const i = tabs.indexOf((Panels.arg ?? "").toLowerCase());
        tab = i >= 0 ? i : 0;
        input.text = "";
        query = "";
        enterMode();
        input.forceActiveFocus();
    }

    Connections {
        target: Panels
        function onOpenChanged() {
            if (Panels.open === root.name) root.opened();
        }
        function onArgChanged() {
            if (Panels.open === root.name && Panels.arg !== "") {
                const i = root.tabs.indexOf(Panels.arg.toLowerCase());
                if (i >= 0) root.setTab(i);
            }
        }
    }

    function setTab(i) {
        tab = (i + tabs.length) % tabs.length;
        enterMode();
    }

    // Called when the mode changes: fetch what the mode needs, then refresh.
    function enterMode() {
        calcResult = "";
        calcFor = "";
        files = [];
        filesFor = "";
        if (mode === "clipboard") {
            clipLines = [];
            status = "loading…";
            cliphist.running = false;
            cliphist.running = true;
        }
        if (mode === "emoji" && emojis.length === 0) emojiFile.reload();
        refresh();
        if (mode === "calc" || mode === "files") debounce.restart();
    }

    onQueryChanged: {
        refresh();
        if (mode === "calc" || mode === "files") debounce.restart();
    }

    function setResults(rows, emptyStatus) {
        results = rows;
        current = 0;
        status = rows.length === 0 ? emptyStatus : "";
        list.positionViewAtBeginning();
    }

    function refresh() {
        switch (mode) {
        case "apps": return refreshApps();
        case "clipboard": return refreshClipboard();
        case "emoji": return refreshEmoji();
        case "calc": return refreshCalc();
        case "files": return refreshFiles();
        }
    }

    // ---- matching ----
    // Score how well `q` (lowercase) matches `s`; -1 = no match.
    function score(s, q) {
        if (!s) return -1;
        s = s.toLowerCase();
        if (s === q) return 100;
        if (s.startsWith(q)) return 80;
        const at = s.indexOf(q);
        if (at > 0) return /[\s\-_.]/.test(s[at - 1]) ? 60 : 40;
        // subsequence: every char of q in order
        let j = 0, gaps = 0, last = -1;
        for (let i = 0; i < s.length && j < q.length; i++) {
            if (s[i] === q[j]) {
                if (last >= 0 && i !== last + 1) gaps++;
                last = i;
                j++;
            }
        }
        return j === q.length ? Math.max(1, 20 - gaps * 3) : -1;
    }

    // ---- apps ----
    function refreshApps() {
        const q = query.trim().toLowerCase();
        const rows = [];
        for (const e of DesktopEntries.applications.values) {
            if (!e || e.noDisplay) continue;
            const uses = history[e.id] ?? 0;
            let s = 0;
            if (q !== "") {
                s = score(e.name, q);
                const g = score(e.genericName, q);
                if (g >= 0) s = Math.max(s, g * 0.6);
                for (const k of e.keywords ?? []) {
                    const ks = score(k, q);
                    if (ks >= 0) s = Math.max(s, ks * 0.5);
                }
                if (s < 0) continue;
            }
            rows.push({
                text: e.name.toLowerCase(),
                detail: (e.genericName || (e.categories?.[0] ?? "")).toLowerCase(),
                icon: e.icon,
                glyph: "",
                data: e,
                rank: s + Math.min(uses, 20) * 1.5
            });
        }
        rows.sort((a, b) => b.rank - a.rank || a.text.localeCompare(b.text));
        setResults(rows.slice(0, limit), "no apps match");
    }

    Connections {
        target: DesktopEntries.applications
        function onValuesChanged() {
            if (Panels.open === root.name && root.mode === "apps") root.refreshApps();
        }
    }

    FileView {
        id: historyFile
        path: Quickshell.dataPath("launcher-history.json")
        printErrors: false
        onLoaded: {
            try {
                root.history = JSON.parse(text()) ?? {};
            } catch (e) {
                root.history = {};
            }
        }
    }

    function launchApp(entry) {
        const h = Object.assign({}, history);
        h[entry.id] = (h[entry.id] ?? 0) + 1;
        history = h;
        historyFile.setText(JSON.stringify(h));
        entry.execute();
        Panels.close();
    }

    // ---- clipboard ----
    Process {
        id: cliphist
        command: ["cliphist", "list"]
        stdout: StdioCollector {
            onStreamFinished: {
                root.clipLines = text.split("\n").filter(l => l !== "");
                if (root.mode === "clipboard") root.refreshClipboard();
            }
        }
    }

    // `cliphist list` lines are "<id>\t<preview>"; binary previews look like
    // "[[ binary data 12 KiB png 100x100 ]]".
    function clipPreview(line) {
        const tab = line.indexOf("\t");
        const body = tab >= 0 ? line.slice(tab + 1) : line;
        const bin = body.match(/^\[\[ binary data (.*) \]\]$/);
        return bin ? { text: "[image] " + bin[1], image: true } : { text: body.replace(/\s+/g, " "), image: false };
    }

    function refreshClipboard() {
        const q = query.trim().toLowerCase();
        const rows = [];
        for (const line of clipLines) {
            const p = clipPreview(line);
            if (q !== "" && !p.text.toLowerCase().includes(q)) continue;
            rows.push({ text: p.text, detail: "", icon: "", glyph: p.image ? "" : "", data: line });
            if (rows.length >= limit) break;
        }
        setResults(rows, cliphist.running ? "loading…" : (clipLines.length ? "nothing matches" : "clipboard history is empty"));
    }

    // The whole line goes to `cliphist decode` on stdin, passed as $1 (never
    // interpolated into the script).
    function copyClip(line) {
        Quickshell.execDetached(["sh", "-c", "printf '%s\\n' \"$1\" | cliphist decode | wl-copy", "sh", line]);
        Panels.close();
    }

    // ---- emoji ----
    FileView {
        id: emojiFile
        path: Quickshell.shellPath("launcher/emoji.json")
        preload: false
        onLoaded: {
            try {
                const obj = JSON.parse(text());
                root.emojis = Object.keys(obj).map(k => [k, obj[k]]);
            } catch (e) {
                console.warn("launcher: emoji.json: " + e);
            }
            if (root.mode === "emoji") root.refreshEmoji();
        }
    }

    function refreshEmoji() {
        const words = query.trim().toLowerCase().split(/\s+/).filter(w => w !== "");
        const rows = [];
        for (const [name, ch] of emojis) {
            let rank = 0, ok = true;
            for (const w of words) {
                const at = name.indexOf(w);
                if (at < 0) { ok = false; break; }
                rank += at === 0 ? 3 : (/\s/.test(name[at - 1]) ? 2 : 1);
            }
            if (!ok) continue;
            rows.push({ text: name, detail: "", icon: "", glyph: ch, data: ch, rank: rank - name.length * 0.01 });
        }
        if (words.length) rows.sort((a, b) => b.rank - a.rank);
        setResults(rows.slice(0, limit), emojis.length ? "no emoji match" : "loading…");
    }

    function copyText(s) {
        Quickshell.execDetached(["wl-copy", "--", s]);
    }

    // ---- calc / files: run as you type, debounced ----
    Timer {
        id: debounce
        interval: 150
        onTriggered: {
            const q = root.query.trim();
            if (root.mode === "calc" && q !== "") {
                root.calcFor = root.query;
                qalc.exec(["qalc", "-t", "--", q]);
            } else if (root.mode === "files" && q !== "") {
                root.filesFor = root.query;
                fd.exec(["fd", "--type", "f", "--hidden", "--exclude", ".git", "--fixed-strings",
                         "--max-results", "200", "--absolute-path", "--", q, root.home]);
            }
        }
    }

    Process {
        id: qalc
        stdout: StdioCollector {
            onStreamFinished: {
                if (root.calcFor !== root.query) return;   // stale
                root.calcResult = text.trim();
                if (root.mode === "calc") root.refreshCalc();
            }
        }
    }

    function refreshCalc() {
        if (query.trim() === "") return setResults([], "type an expression");
        if (calcFor !== query) return setResults(results.length ? results : [], "…");
        if (calcResult === "") return setResults([], "no result");
        setResults([{ text: calcResult, detail: "enter copies", icon: "", glyph: "=", data: calcResult }], "");
    }

    Process {
        id: fd
        stdout: StdioCollector {
            onStreamFinished: {
                if (root.filesFor !== root.query) return;  // stale
                root.files = text.split("\n").filter(l => l !== "");
                if (root.mode === "files") root.refreshFiles();
            }
        }
    }

    function tilde(p) {
        return home !== "" && p.startsWith(home + "/") ? "~" + p.slice(home.length) : p;
    }

    function refreshFiles() {
        if (query.trim() === "") return setResults([], "type to search ~");
        if (filesFor !== query) return setResults(results, "searching…");
        const rows = files.map(p => {
            const slash = p.lastIndexOf("/");
            return { text: p.slice(slash + 1), detail: tilde(p.slice(0, slash)), icon: "", glyph: "", data: p };
        });
        setResults(rows, "no files match");
    }

    // ---- running the selected row ----
    function run() {
        const r = results[current];
        if (!r) return;
        switch (mode) {
        case "apps": return launchApp(r.data);
        case "clipboard": return copyClip(r.data);
        case "emoji":
            copyText(r.data);
            return Panels.close();
        case "calc":
            copyText(r.data);
            input.text = r.data;     // keep going from the result
            return;
        case "files":
            Quickshell.execDetached(["xdg-open", r.data]);
            return Panels.close();
        }
    }

    function move(d) {
        if (results.length === 0) return;
        current = Math.max(0, Math.min(results.length - 1, current + d));
        list.positionViewAtIndex(current, ListView.Contain);
    }

    // ---- view ----
    Rectangle {
        id: box
        width: 560
        height: col.implicitHeight + Theme.borderWidth * 2
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round(parent.height * 0.2)
        color: Theme.bg
        border.color: Theme.border
        border.width: Theme.borderWidth

        // Swallow clicks on the box so only clicks outside close the overlay.
        MouseArea { anchors.fill: parent }

        Column {
            id: col
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.borderWidth }

            // input row: ❯ query▌
            Item {
                width: parent.width
                height: 36
                Txt {
                    id: prompt
                    anchors { left: parent.left; leftMargin: Theme.pad; verticalCenter: parent.verticalCenter }
                    text: "❯"
                    color: Theme.accent
                    font.bold: true
                }
                TextInput {
                    id: input
                    anchors { left: prompt.right; leftMargin: Theme.padSmall; right: parent.right; rightMargin: Theme.pad; verticalCenter: parent.verticalCenter }
                    color: Theme.text
                    selectionColor: Theme.accent
                    selectedTextColor: Theme.onAccent
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize
                    clip: true
                    focus: true
                    cursorDelegate: Rectangle {
                        width: 7
                        color: Theme.accent
                        visible: input.activeFocus
                    }
                    onTextChanged: root.query = text

                    Txt {
                        anchors { left: parent.left; leftMargin: 11; verticalCenter: parent.verticalCenter }
                        visible: input.text === ""
                        text: root.mode === "calc" ? "expression" : "search " + root.mode
                        color: Theme.muted
                    }

                    Keys.onPressed: event => {
                        const ctrl = event.modifiers & Qt.ControlModifier;
                        if (event.key === Qt.Key_Backtab || (event.key === Qt.Key_Tab && (event.modifiers & Qt.ShiftModifier))) {
                            root.setTab(root.tab - 1);
                        } else if (event.key === Qt.Key_Tab) {
                            root.setTab(root.tab + 1);
                        } else if (ctrl && event.key >= Qt.Key_1 && event.key <= Qt.Key_5) {
                            root.setTab(event.key - Qt.Key_1);
                        } else if (event.key === Qt.Key_Down || (ctrl && event.key === Qt.Key_J)) {
                            root.move(1);
                        } else if (event.key === Qt.Key_Up || (ctrl && event.key === Qt.Key_K)) {
                            root.move(-1);
                        } else if (event.key === Qt.Key_PageDown) {
                            root.move(root.maxRows);
                        } else if (event.key === Qt.Key_PageUp) {
                            root.move(-root.maxRows);
                        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            root.run();
                        } else {
                            return;          // let TextInput (and Escape → Overlay) handle it
                        }
                        event.accepted = true;
                    }
                }
            }

            Rectangle { width: parent.width; height: Theme.borderWidth; color: Theme.border }

            // tabs
            Item {
                width: parent.width
                height: tabRow.implicitHeight + 6
                Row {
                    id: tabRow
                    anchors { left: parent.left; leftMargin: Theme.pad; bottom: parent.bottom }
                    spacing: Theme.gap
                    Repeater {
                        model: root.tabs
                        delegate: Rectangle {
                            required property string modelData
                            required property int index
                            readonly property bool on: index === root.tab
                            implicitWidth: tabLabel.implicitWidth + 14
                            implicitHeight: tabLabel.implicitHeight + 4
                            color: on ? Theme.accent : (tabMouse.containsMouse ? Theme.raised : Theme.surface)
                            Txt {
                                id: tabLabel
                                anchors.centerIn: parent
                                text: modelData
                                color: parent.on ? Theme.onAccent : Theme.dim
                                font.bold: parent.on
                            }
                            MouseArea {
                                id: tabMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    root.setTab(index);
                                    input.forceActiveFocus();
                                }
                            }
                        }
                    }
                }
            }

            // results
            Item {
                width: parent.width
                height: Math.max(1, Math.min(root.results.length, root.maxRows)) * root.rowHeight + 12

                Txt {
                    anchors { left: parent.left; leftMargin: Theme.pad; verticalCenter: parent.verticalCenter }
                    visible: root.results.length === 0
                    text: root.status
                    color: Theme.muted
                }

                ListView {
                    id: list
                    anchors { fill: parent; topMargin: 6; bottomMargin: 6 }
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: root.results
                    currentIndex: root.current
                    delegate: Rectangle {
                        id: row
                        required property var modelData
                        required property int index
                        readonly property bool on: index === root.current
                        width: ListView.view.width
                        height: root.rowHeight
                        color: on ? Theme.accent : (rowMouse.containsMouse ? Theme.surface : "transparent")

                        Item {
                            id: iconBox
                            anchors { left: parent.left; leftMargin: Theme.pad; verticalCenter: parent.verticalCenter }
                            width: 16
                            height: 16
                            Image {
                                anchors.fill: parent
                                visible: row.modelData.icon !== ""
                                source: !row.modelData.icon ? ""
                                    : row.modelData.icon.startsWith("/") ? "file://" + row.modelData.icon
                                    : Quickshell.iconPath(row.modelData.icon, "application-x-executable")
                                sourceSize { width: 16; height: 16 }
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                                smooth: true
                            }
                            Txt {
                                anchors.centerIn: parent
                                visible: row.modelData.icon === ""
                                text: row.modelData.glyph
                                color: row.on ? Theme.onAccent : Theme.muted
                                elide: Text.ElideNone
                            }
                        }
                        Txt {
                            id: detail
                            anchors { right: parent.right; rightMargin: Theme.pad; verticalCenter: parent.verticalCenter }
                            width: Math.min(implicitWidth, parent.width * 0.4)
                            text: row.modelData.detail
                            color: row.on ? Theme.raised : Theme.muted
                            elide: Text.ElideLeft
                        }
                        Txt {
                            anchors { left: iconBox.right; leftMargin: Theme.padSmall; right: detail.left; rightMargin: Theme.padSmall; verticalCenter: parent.verticalCenter }
                            text: row.modelData.text
                            color: row.on ? Theme.onAccent : Theme.text
                            font.bold: row.on
                        }
                        MouseArea {
                            id: rowMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.current = row.index;
                                root.run();
                                input.forceActiveFocus();
                            }
                        }
                    }
                }
            }
        }
    }
}
