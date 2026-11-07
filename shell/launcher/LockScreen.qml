import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Services.Pam
import qs.common
import qs.services

// Session lock (ext-session-lock-v1) while Session.locked. Every screen gets a
// surface with the wallpaper ($VSHELL_WALLPAPER, else flat Theme.bg) and a
// clock box; the primary screen also shows the password field. Keys typed on
// any screen feed the same password buffer, so it doesn't matter which lock
// surface the compositor focuses.
//
// PAM: uses /etc/pam.d/vshell (NixOS: `security.pam.services.vshell = {};`),
// falling back to /etc/pam.d/login when that file is missing.
Scope {
    id: root

    property string buffer: ""          // password being typed
    property string pending: ""         // password handed to PAM
    property string error: ""
    readonly property bool busy: pam.active
    readonly property string wallpaper: Quickshell.env("VSHELL_WALLPAPER") ?? ""

    function submit() {
        if (busy || buffer === "") return;
        pending = buffer;
        buffer = "";
        error = "";
        pam.config = "vshell";
        if (!pam.start()) {
            pam.config = "login";
            if (!pam.start()) {
                pending = "";
                error = "pam: no usable config";
            }
        }
    }

    function key(event) {
        const ctrl = event.modifiers & Qt.ControlModifier;
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            submit();
        } else if (event.key === Qt.Key_Backspace) {
            buffer = ctrl ? "" : buffer.slice(0, -1);
        } else if (event.key === Qt.Key_Escape || (ctrl && event.key === Qt.Key_U)) {
            buffer = "";
        } else if (!ctrl && event.text !== "" && event.text.charCodeAt(0) >= 32) {
            buffer += event.text;
            error = "";
        }
        event.accepted = true;
    }

    PamContext {
        id: pam
        onPamMessage: {
            if (responseRequired) {
                respond(root.pending);
                root.pending = "";
            } else if (messageIsError && message !== "") {
                root.error = message.toLowerCase();
            }
        }
        onCompleted: result => {
            root.pending = "";
            if (result === PamResult.Success) {
                root.error = "";
                root.buffer = "";
                Session.locked = false;
            } else {
                root.error = result === PamResult.Failed ? "wrong password" : "pam error";
            }
        }
        onError: err => {
            root.pending = "";
            root.error = "pam error";
        }
    }

    Connections {
        target: Session
        function onLockedChanged() {
            if (Session.locked) {
                Panels.close();
                root.buffer = "";
                root.error = "";
            }
        }
    }

    WlSessionLock {
        id: lock
        locked: Session.locked
        onSecureStateChanged: Session.secure = secure
        onLockStateChanged: if (!locked) Session.secure = false

        surface: Component {
            WlSessionLockSurface {
                id: surf
                color: Theme.bg
                readonly property bool primary: screen !== null && screen.name === (Panels.primary()?.name ?? "")

                SystemClock {
                    id: clock
                    precision: SystemClock.Minutes
                }

                Image {
                    anchors.fill: parent
                    visible: root.wallpaper !== "" && status === Image.Ready
                    source: root.wallpaper !== "" ? "file://" + root.wallpaper : ""
                    fillMode: Image.PreserveAspectCrop
                    sourceSize { width: surf.width; height: surf.height }
                    asynchronous: true
                }

                // Takes the keys on whichever surface the compositor focuses.
                Item {
                    id: keys
                    anchors.fill: parent
                    focus: true
                    Keys.onPressed: event => root.key(event)
                    Component.onCompleted: forceActiveFocus()
                    MouseArea { anchors.fill: parent; onClicked: keys.forceActiveFocus() }
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 420
                    height: col.implicitHeight + 48
                    // The one allowed translucency: the lock box over the wallpaper.
                    color: Qt.rgba(Theme.bg.r, Theme.bg.g, Theme.bg.b, 0.85)
                    border.color: Theme.border
                    border.width: Theme.borderWidth

                    Column {
                        id: col
                        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: 24 }
                        spacing: Theme.pad

                        Txt {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: Qt.formatDateTime(clock.date, "HH:mm")
                            font.pixelSize: 64
                            font.bold: true
                        }
                        Txt {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: Qt.formatDateTime(clock.date, "dddd dd MMMM").toLowerCase()
                            color: Theme.accent
                        }

                        Item { width: 1; height: Theme.pad; visible: surf.primary }

                        Rectangle {
                            visible: surf.primary
                            width: parent.width
                            height: 34
                            color: Theme.bg
                            border.width: Theme.borderWidth
                            border.color: root.busy ? Theme.yellow
                                : (root.error !== "" ? Theme.red
                                : (keys.activeFocus || root.buffer !== "" ? Theme.accent : Theme.border))

                            Row {
                                anchors { left: parent.left; leftMargin: Theme.pad; verticalCenter: parent.verticalCenter }
                                spacing: 0
                                Txt {
                                    text: root.busy ? "checking…"
                                        : (root.buffer === "" ? "password" : "*".repeat(Math.min(root.buffer.length, 40)))
                                    color: root.buffer === "" || root.busy ? Theme.muted : Theme.text
                                }
                                Rectangle {
                                    visible: !root.busy && root.buffer !== ""
                                    width: 7
                                    height: Theme.fontSize + 4
                                    color: Theme.accent
                                }
                            }
                        }

                        Txt {
                            visible: surf.primary && root.error !== ""
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: root.error
                            color: Theme.red
                        }
                    }
                }
            }
        }
    }
}
