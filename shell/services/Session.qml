pragma Singleton
import QtQuick
import Quickshell

// Session actions: lock, suspend, reboot, power off, log out, reboot to Windows.
// `locked` drives the LockScreen (WlSessionLock); LockScreen sets it back to
// false after a successful PAM login and keeps `secure` in sync.
//
// Idle / logind: swayidle should call `qs ipc call lock lock` (the "lock" IPC
// handler in shell.qml) both on timeout and on `lock` / `before-sleep`, e.g.
//   swayidle -w timeout 600 'qs ipc call lock lock' \
//               lock 'qs ipc call lock lock' before-sleep 'qs ipc call lock lock'
// so `loginctl lock-session` reaches the shell without any D-Bus code here.
Singleton {
    id: root

    property bool locked: false
    // Set by LockScreen: true once the compositor confirms every screen is covered.
    property bool secure: false

    property bool suspendPending: false

    function lock() {
        locked = true;
    }

    // Lock first, then suspend once the lock is secure (or after a short
    // fallback delay, so a slow compositor can't block the suspend forever).
    function suspend() {
        lock();
        if (secure) {
            doSuspend();
        } else {
            suspendPending = true;
            suspendFallback.restart();
        }
    }

    function doSuspend() {
        suspendPending = false;
        suspendFallback.stop();
        Quickshell.execDetached(["systemctl", "suspend"]);
    }

    function reboot() {
        Quickshell.execDetached(["systemctl", "reboot"]);
    }

    function poweroff() {
        Quickshell.execDetached(["systemctl", "poweroff"]);
    }

    // Mango's `quit` dispatcher (src/config/binding_parse.c) ends the session.
    function logout() {
        Quickshell.execDetached(["mmsg", "dispatch", "quit"]);
    }

    // This exact path is allowed NOPASSWD in sudoers; -n so it never prompts.
    function rebootToWindows() {
        Quickshell.execDetached(["sudo", "-n", "/run/current-system/sw/bin/reboot-to-windows"]);
    }

    onSecureChanged: if (secure && suspendPending) doSuspend()

    Timer {
        id: suspendFallback
        interval: 1500
        onTriggered: if (root.suspendPending) root.doSuspend()
    }
}
