import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.common

// Super+W: every wallpaper as a thumbnail with its palette. Picking one swaps
// the wallpaper and recolours the shell straight away, then opens a small
// terminal running `vx wallpaper <name>` (asks for your password, rebuilds),
// after which GTK apps, the terminal, Mango and the lock screen follow.
Overlay {
    id: root
    name: "wallpaper"

    property var walls: []
    property string current: Quickshell.env("VSHELL_WALLPAPER_NAME") ?? ""
    property int sel: 0

    FileView {
        path: Quickshell.env("VSHELL_WALLPAPERS") ?? ""
        onLoaded: {
            try {
                root.walls = JSON.parse(text());
            } catch (e) {
                console.warn("wallpapers: " + e);
            }
        }
    }

    onVisibleChanged: {
        if (!visible) return;
        const i = walls.findIndex(w => w.name === current);
        sel = i >= 0 ? i : 0;
        grid.positionViewAtIndex(sel, GridView.Contain);
        grid.forceActiveFocus();
    }

    function apply(w) {
        // 1. the shell's colours, now
        Theme.raw = Object.assign({}, Theme.raw, w.colors);
        // 2. the wallpaper, now: wide image on wide screens, 4K one elsewhere
        const args = [];
        for (const s of Quickshell.screens)
            args.push("-o", s.name, "-m", "fill", "-i", s.width / s.height >= 2 ? w.wide : w.uhd);
        Quickshell.execDetached(["sh", "-c", "pkill -x swaybg; exec swaybg \"$@\"", "swaybg"].concat(args));
        // 3. everything else: rebuild with the new theme
        Quickshell.execDetached(["ghostty", "--title=vx-rebuild", "-e", "sh", "-c",
            "vx wallpaper \"$1\"; echo; printf 'enter to close '; read -r _", "sh", w.name]);
        current = w.name;
        Panels.close();
    }

    Panel {
        id: panel
        anchors.centerIn: parent
        width: Math.min(parent.width - 80, 4 * 220 + 3 * Theme.gap + Theme.pad * 2 + 4)
        title: "wallpaper"
        subtitle: root.current

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(3, Math.ceil(root.walls.length / 4)) * cellHeight
            clip: true
            cellWidth: 220 + Theme.gap
            cellHeight: 92 + 30 + Theme.gap
            model: root.walls
            currentIndex: root.sel
            focus: true
            boundsBehavior: Flickable.StopAtBounds

            Keys.onPressed: e => {
                const cols = 4, n = root.walls.length;
                let i = root.sel;
                if (e.key === Qt.Key_Left || e.key === Qt.Key_H) i -= 1;
                else if (e.key === Qt.Key_Right || e.key === Qt.Key_L) i += 1;
                else if (e.key === Qt.Key_Up || e.key === Qt.Key_K) i -= cols;
                else if (e.key === Qt.Key_Down || e.key === Qt.Key_J) i += cols;
                else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { if (n) root.apply(root.walls[root.sel]); e.accepted = true; return; }
                else if (e.key === Qt.Key_Escape) { Panels.close(); e.accepted = true; return; }
                else return;
                root.sel = Math.max(0, Math.min(n - 1, i));
                grid.positionViewAtIndex(root.sel, GridView.Contain);
                e.accepted = true;
            }

            delegate: Item {
                id: cell
                required property var modelData
                required property int index
                readonly property bool isCurrent: modelData.name === root.current
                readonly property bool isSel: index === root.sel
                width: 220
                height: 92 + 30

                Rectangle {
                    id: frame
                    width: parent.width
                    height: 92
                    color: Theme.surface
                    border.width: Theme.borderWidth
                    border.color: cell.isSel ? Theme.accent : (cell.isCurrent ? Theme.text : Theme.border)

                    Image {
                        anchors.fill: parent
                        anchors.margins: Theme.borderWidth
                        source: "file://" + cell.modelData.wide
                        sourceSize.width: 440
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        cache: true
                    }
                }

                Row {
                    anchors { left: parent.left; top: frame.bottom; topMargin: 6 }
                    spacing: 6
                    Rectangle {
                        width: Math.max(60, nameText.implicitWidth + Theme.padSmall * 2)
                        height: 18
                        color: cell.isSel ? Theme.accent : "transparent"
                        Txt {
                            id: nameText
                            anchors.centerIn: parent
                            text: cell.modelData.name + (cell.isCurrent ? " *" : "")
                            color: cell.isSel ? Theme.onAccent : Theme.text
                            font.bold: cell.isSel
                        }
                    }
                    Repeater {
                        model: ["base00", "base01", "base0D", "base08", "base0B", "base0A", "base0C"]
                        Rectangle {
                            required property string modelData
                            width: 12; height: 12
                            anchors.verticalCenter: parent.verticalCenter
                            color: "#" + cell.modelData.colors[modelData]
                            border.width: 1
                            border.color: Theme.border
                        }
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onEntered: root.sel = cell.index
                    onClicked: root.apply(cell.modelData)
                }
            }
        }

        Txt {
            text: "arrows / hjkl move · enter applies · the rest of the desktop follows after the rebuild"
            color: Theme.muted
            font.pixelSize: Theme.fontSmall
        }
    }
}
