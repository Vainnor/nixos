import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.common
import qs.services

// The wallpaper on every screen, on the background layer (this replaces
// swaybg). A new one fades in over the old while it settles from a slight
// zoom, so picking a wallpaper is one smooth change; the old image is let go
// once the new one has covered it.
Variants {
    model: Quickshell.screens

    delegate: Component {
        PanelWindow {
            id: win
            required property var modelData
            screen: modelData

            WlrLayershell.layer: WlrLayer.Background
            WlrLayershell.namespace: "vshell-wallpaper"
            exclusionMode: ExclusionMode.Ignore
            anchors { top: true; bottom: true; left: true; right: true }
            color: Theme.bg

            readonly property string src: Wallpaper.image(modelData)
            property Image front: null      // the image showing now
            property Image incoming: null   // loading, revealed when ready

            onSrcChanged: show()
            Component.onCompleted: show()

            function show() {
                if (src === "") return;
                const url = "file://" + src;
                if (front && front.source.toString() === url) return;
                const next = front === a ? b : a;
                reveal.stop();
                next.opacity = 0;
                next.scale = 1.05;
                next.z = 1;
                if (front) front.z = 0;
                incoming = next;
                if (next.source.toString() === url && next.status === Image.Ready) start(next);
                else next.source = url;
            }

            function start(img) {
                incoming = null;
                reveal.img = img;
                reveal.restart();
            }

            ParallelAnimation {
                id: reveal
                property Image img: null
                NumberAnimation { target: reveal.img; property: "opacity"; from: 0; to: 1; duration: 700; easing.type: Easing.OutCubic }
                NumberAnimation { target: reveal.img; property: "scale"; from: 1.05; to: 1; duration: 1200; easing.type: Easing.OutQuart }
                onFinished: {
                    const old = win.front;
                    win.front = reveal.img;
                    if (old && old !== win.front) old.source = "";
                }
            }

            Item {
                anchors.fill: parent
                clip: true
                Image {
                    id: a
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectCrop
                    sourceSize { width: win.modelData.width; height: win.modelData.height }
                    asynchronous: true
                    cache: false
                    opacity: 0
                    onStatusChanged: if (status === Image.Ready && win.incoming === a) win.start(a)
                }
                Image {
                    id: b
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectCrop
                    sourceSize { width: win.modelData.width; height: win.modelData.height }
                    asynchronous: true
                    cache: false
                    opacity: 0
                    onStatusChanged: if (status === Image.Ready && win.incoming === b) win.start(b)
                }
            }
        }
    }
}
