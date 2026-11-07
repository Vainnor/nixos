pragma Singleton
import QtQuick
import Quickshell

// wlsunset night light. It runs from login, so assume on; toggling sends
// SIGUSR1 (wlsunset's on/off toggle) and flips our idea of the state.
Singleton {
    id: root
    property bool enabled: true

    function toggle() {
        Quickshell.execDetached(["pkill", "-USR1", "-x", "wlsunset"]);
        root.enabled = !root.enabled;
    }
}
