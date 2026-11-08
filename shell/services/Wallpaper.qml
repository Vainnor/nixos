pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// The wallpaper and its colours, live. Picking one (Super+W) changes the
// background (wallpaper/Background.qml fades it in), the shell, Mango's
// borders and Ghostty at once: no rebuild, no password.
//
// The choice lives in $VSHELL_STATE: `wallpaper` (the name), `mango.conf`
// (sourced by Mango's config) and `ghostty` (a config-file of Ghostty's). It
// is also written to meta.nix (`vx wallpaper set --no-switch`), so the next
// switch builds GTK, the greeter and the rest with it; that switch removes
// the state files again, as the built config then matches.
Singleton {
    id: root

    readonly property string stateDir: Quickshell.env("VSHELL_STATE") ?? ""
    readonly property string builtName: Quickshell.env("VSHELL_WALLPAPER_NAME") ?? ""
    property var walls: []
    property string name: builtName
    readonly property var current: walls.find(w => w.name === name) ?? null

    property bool wallsReady: false
    property bool stateReady: stateDir === ""
    property string savedName: ""
    // The palette over the built theme (common/Theme.qml reads it); empty = as built.
    property var liveColors: ({})

    // The image for a screen: the wide one on ultrawides, the 4K one elsewhere.
    function image(screen) {
        if (!current) return Quickshell.env("VSHELL_WALLPAPER") ?? "";
        return screen && screen.height > 0 && screen.width / screen.height >= 2 ? current.wide : current.uhd;
    }

    FileView {
        path: Quickshell.env("VSHELL_WALLPAPERS") ?? ""
        onLoaded: {
            try {
                root.walls = JSON.parse(text());
            } catch (e) {
                console.warn("wallpapers: " + e);
            }
            root.wallsReady = true;
            root.restore();
        }
        onLoadFailed: { root.wallsReady = true; root.restore(); }
    }

    FileView {
        id: saved
        path: root.stateDir !== "" ? root.stateDir + "/wallpaper" : ""
        printErrors: false
        onLoaded: { root.savedName = text().trim(); root.stateReady = true; root.restore(); }
        onLoadFailed: { root.stateReady = true; root.restore(); }
    }

    // At start: a wallpaper picked since the last switch wins over the built one.
    function restore() {
        if (!wallsReady || !stateReady) return;
        const n = savedName;
        if (n !== "" && walls.some(w => w.name === n)) name = n;
        if (name !== builtName && current) liveColors = current.colors;
    }

    function set(n) {
        const w = walls.find(x => x.name === n);
        if (!w || n === name) return;
        name = n;
        liveColors = w.colors;
        if (stateDir === "") return;
        writer.command = ["sh", "-c", writer.script, "sh", stateDir, n, mangoConf(w.colors), ghosttyConf(w.colors)];
        writer.running = true;
    }

    function argb(c, k) { return "0x" + (c[k] ?? "000000") + "ff"; }

    function mangoConf(c) {
        return [
            "root_color=" + argb(c, "base00"),
            "border_color=" + argb(c, c.border ? "border" : "base02"),
            "focus_color=" + argb(c, "base0D"),
            "urgent_color=" + argb(c, "base08"),
            "maximized_screen_color=" + argb(c, "base0B"),
            "scratchpad_color=" + argb(c, "base0E"),
            "global_color=" + argb(c, "base0C"),
            "overlay_color=" + argb(c, "base0A"),
        ].join("\n") + "\n";
    }

    // Same roles as Stylix's Ghostty theme.
    function ghosttyConf(c) {
        const ansi = ["base00", "base08", "base0B", "base0A", "base0D", "base0E", "base0C", "base05",
                      "base03", "base08", "base0B", "base0A", "base0D", "base0E", "base0C", "base07"];
        const l = [
            "background = " + c.base00,
            "foreground = " + c.base05,
            "cursor-color = " + c.base05,
            "selection-background = " + c.base02,
            "selection-foreground = " + c.base05,
        ];
        ansi.forEach((k, i) => l.push("palette = " + i + "=#" + c[k]));
        return l.join("\n") + "\n";
    }

    // Write the state, tell Mango and Ghostty to reload, record it in meta.nix.
    Process {
        id: writer
        readonly property string script: [
            'd="$1"; mkdir -p "$d"',
            'printf "%s" "$3" > "$d/mango.conf.tmp" && mv "$d/mango.conf.tmp" "$d/mango.conf"',
            'printf "%s" "$4" > "$d/ghostty.tmp" && mv "$d/ghostty.tmp" "$d/ghostty"',
            'printf "%s\\n" "$2" > "$d/wallpaper.tmp" && mv "$d/wallpaper.tmp" "$d/wallpaper"',
            'mmsg dispatch reload_config >/dev/null 2>&1',
            'pkill -USR2 -x .ghostty-wrappe; pkill -USR2 -x ghostty',
            'vx wallpaper set --no-switch "$2" >/dev/null 2>&1 || true',
        ].join("\n")
    }
}
