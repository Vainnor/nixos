import QtQuick
import qs.common

// A bar dropdown: one Panel hung under the module that opened it
// (Panels.anchorX, set by TopBar.toggle), kept on screen. Clicking outside,
// Escape or clicking the module again closes it (Overlay).
//   Dropdown { name: "cpu"; title: "cpu"; StatRow { ... } }
Overlay {
    id: root
    property alias title: panel.title
    property alias subtitle: panel.subtitle
    property alias panelWidth: panel.width
    default property alias body: panel.body

    content: Panel {
        id: panel
        y: Theme.panelGap
        width: 320
        x: Math.round(Math.max(Theme.panelGap, Math.min(root.width - width - Theme.panelGap,
            (Panels.anchorX >= 0 ? Panels.anchorX : root.width) - width / 2)))
    }
}
