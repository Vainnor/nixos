import QtQuick
import Quickshell
import Quickshell.Services.Notifications
import qs.common
import qs.services

// One notification: surface card with a 3px stripe on the left (accent; red
// when critical, muted when low), app name + age, bold summary, dim body,
// an optional square image on the right, then action buttons and [ dismiss ].
// Clicking the card runs the "default" action if there is one, else dismisses.
Rectangle {
    id: root
    required property var notification
    readonly property alias hovered: hover.hovered

    readonly property bool critical: notification?.urgency === NotificationUrgency.Critical
    readonly property bool low: notification?.urgency === NotificationUrgency.Low
    readonly property var actionList: Array.prototype.filter.call(notification?.actions ?? [], a => a.identifier !== "default" && a.text !== "")

    // The image (often a profile picture), else the app icon.
    readonly property string imageSource: {
        const n = notification;
        if (!n) return "";
        const src = n.image !== "" ? n.image : n.appIcon;
        if (src === "") return "";
        if (src.startsWith("/")) return "file://" + src;
        if (src.includes("://")) return src;
        return Quickshell.iconPath(src, true);
    }

    implicitHeight: Math.max(content.implicitHeight, image.visible ? image.height : 0) + 14
    color: Theme.surface

    HoverHandler { id: hover }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            const n = root.notification;
            const a = Notifs.defaultAction(n);
            if (a) a.invoke();
            else Notifs.dismiss(n);
        }
    }

    Rectangle {
        id: stripe
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: 3
        color: root.critical ? Theme.red : (root.low ? Theme.muted : Theme.accent)
    }

    Image {
        id: image
        visible: root.imageSource !== "" && status !== Image.Error
        anchors { right: parent.right; top: parent.top; margins: 7; rightMargin: 9 }
        width: 40
        height: 40
        source: root.imageSource
        sourceSize { width: 80; height: 80 }
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        smooth: true
    }

    Column {
        id: content
        anchors {
            left: stripe.right; leftMargin: 9
            right: image.visible ? image.left : parent.right; rightMargin: 9
            top: parent.top; topMargin: 7
        }
        spacing: 2

        Item {
            width: parent.width
            height: app.implicitHeight
            Txt {
                id: app
                anchors { left: parent.left; right: age.left; rightMargin: Theme.gap }
                text: (root.notification?.appName || "notification").toLowerCase()
                color: Theme.muted
                font.pixelSize: Theme.fontSmall
            }
            // The age; turns into a dismiss "x" while the card is hovered.
            Txt {
                id: age
                anchors.right: parent.right
                text: root.hovered && !buttons.visible ? "x" : Notifs.ago(root.notification)
                color: closeMouse.containsMouse ? Theme.red : Theme.muted
                font.pixelSize: Theme.fontSmall
                font.bold: root.hovered && !buttons.visible
                MouseArea {
                    id: closeMouse
                    anchors { fill: parent; margins: -4 }
                    enabled: root.hovered && !buttons.visible
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Notifs.dismiss(root.notification)
                }
            }
        }

        Txt {
            width: parent.width
            visible: text !== ""
            text: Notifs.plain(root.notification?.summary)
            textFormat: Text.PlainText
            font.bold: true
            wrapMode: Text.Wrap
            maximumLineCount: 2
        }

        Txt {
            width: parent.width
            visible: text !== ""
            text: Notifs.plain(root.notification?.body)
            textFormat: Text.PlainText
            color: Theme.dim
            wrapMode: Text.Wrap
            maximumLineCount: 4
        }

        Item { width: 1; height: 4; visible: buttons.visible }

        Flow {
            id: buttons
            width: parent.width
            spacing: Theme.gap
            visible: root.actionList.length > 0

            Repeater {
                model: root.actionList
                delegate: Btn {
                    required property var modelData
                    text: "[ " + modelData.text.toLowerCase() + " ]"
                    onClicked: modelData.invoke()
                }
            }
            Btn {
                text: "[ dismiss ]"
                onClicked: Notifs.dismiss(root.notification)
            }
        }
    }
}
