import QtQuick
import Quickshell
import qs.common

// Month calendar under the bar clock: flat grid, weeks start on monday,
// today accent-filled, ‹ › to page months. Opens via Panels.toggle("calendar").
Overlay {
    id: root
    name: "calendar"

    readonly property var monthNames: ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"]
    readonly property var dayNames: ["mo", "tu", "we", "th", "fr", "sa", "su"]

    property int year: clock.date.getFullYear()
    property int month: clock.date.getMonth()

    function reset() {
        const d = new Date();
        year = d.getFullYear();
        month = d.getMonth();
    }
    function page(delta) {
        const d = new Date(year, month + delta, 1);
        year = d.getFullYear();
        month = d.getMonth();
    }

    // 42 cells (6 weeks) starting on the monday on/before the 1st.
    readonly property var cells: {
        const first = new Date(year, month, 1);
        const offset = (first.getDay() + 6) % 7;
        const out = [];
        for (let i = 0; i < 42; i++) {
            const d = new Date(year, month, 1 - offset + i);
            out.push({ day: d.getDate(), inMonth: d.getMonth() === month, y: d.getFullYear(), m: d.getMonth() });
        }
        return out;
    }

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    Connections {
        target: Panels
        function onOpenChanged() {
            if (Panels.open === root.name) root.reset();
        }
    }

    Panel {
        id: panel
        anchors { top: parent.top; topMargin: Theme.panelGap; horizontalCenter: parent.horizontalCenter }
        width: grid.implicitWidth + Theme.pad * 2 + Theme.borderWidth * 2
        title: root.monthNames[root.month] + " " + root.year
        actions: [
            Btn { text: "‹"; onClicked: root.page(-1) },
            Btn { text: "›"; onClicked: root.page(1) }
        ]

        Grid {
            id: grid
            columns: 7
            spacing: Theme.gap

            Repeater {
                model: root.dayNames
                delegate: Txt {
                    required property string modelData
                    width: 28
                    height: 18
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData
                    color: Theme.muted
                    font.pixelSize: Theme.fontSmall
                }
            }

            Repeater {
                model: root.cells
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool today: modelData.inMonth
                        && modelData.day === clock.date.getDate()
                        && modelData.m === clock.date.getMonth()
                        && modelData.y === clock.date.getFullYear()
                    width: 28
                    height: 22
                    color: today ? Theme.accent : (modelData.inMonth ? Theme.surface : "transparent")
                    Txt {
                        anchors.centerIn: parent
                        text: parent.modelData.day
                        font.bold: parent.today
                        color: parent.today ? Theme.onAccent : (parent.modelData.inMonth ? Theme.text : Theme.muted)
                    }
                }
            }
        }
    }
}
