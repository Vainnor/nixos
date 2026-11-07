pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Colours, font and sizes for the whole shell. Nix writes the palette to
// $VSHELL_THEME (JSON: base00..base0F, border, accent2, primaryModel); without
// it the shell falls back to Vainnor slate.
Singleton {
    id: root

    property var raw: ({})

    FileView {
        path: Quickshell.env("VSHELL_THEME") ?? ""
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.raw = JSON.parse(text());
            } catch (e) {
                console.warn("theme: " + e);
            }
        }
    }

    function hex(key, fallback) {
        return raw[key] ? "#" + raw[key] : fallback;
    }

    // base16 roles
    readonly property color bg: hex("base00", "#10141B")        // windows, panels
    readonly property color surface: hex("base01", "#161B24")   // modules, buttons, cards
    readonly property color raised: hex("base02", "#1D232E")    // hover, slider track
    readonly property color muted: hex("base03", "#6C7A8F")     // labels, timestamps
    readonly property color dim: hex("base04", "#8D98AB")       // secondary text
    readonly property color text: hex("base05", "#D3D9E2")
    readonly property color red: hex("base08", "#E5675F")
    readonly property color orange: hex("base09", "#E8803F")
    readonly property color yellow: hex("base0A", "#E6A52E")
    readonly property color green: hex("base0B", "#94C47D")
    readonly property color cyan: hex("base0C", "#4FC3D0")
    readonly property color accent: hex("base0D", "#B5A8F5")    // active, focus, fills
    readonly property color blue: hex("base0E", "#6F9FD8")
    readonly property color border: hex("border", "#35465C")
    readonly property color accent2: hex("accent2", "#D4CDF9")
    readonly property color onAccent: bg                        // text on accent fills

    // The monitor that gets pop-ups, OSD and panels when focus is unknown.
    readonly property string primaryModel: raw.primaryModel ?? ""

    readonly property string font: raw.font ?? "JetBrainsMono Nerd Font"
    readonly property int fontSize: 12
    readonly property int fontSmall: 10

    // Shape: square and flat. No radius, shadows, blur or gradients anywhere.
    readonly property int borderWidth: 2
    readonly property int barHeight: 30
    readonly property int moduleHeight: 22
    readonly property int gap: 4        // between modules / buttons
    readonly property int pad: 10       // inside panels
    readonly property int padSmall: 8   // inside buttons / modules (horizontal)
    readonly property int panelGap: 8   // panel distance from the bar / screen edge
    readonly property int anim: 120     // ms; keep motion short
}
