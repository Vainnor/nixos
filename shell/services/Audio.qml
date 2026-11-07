pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Services.Pipewire

// PipeWire volume, mute, devices and app streams.
//
// The default sink is usually a virtual channel (System/Game/Chat/Music/
// Browser, node.name channel_<lower>). When $VSHELL_AUDIO_OUTPUT names the real
// hardware sink, `output` (and so volume/mute/raise/lower) drives that node so
// every channel follows; otherwise it falls back to the default sink.
//
// osdRequested(kind) fires on user-visible changes ("volume" or "mic") so the
// OSD can show; it stays quiet while PipeWire binds at startup.
Singleton {
    id: root

    signal osdRequested(string kind)

    readonly property string hardwareName: Quickshell.env("VSHELL_AUDIO_OUTPUT") ?? ""

    readonly property var nodes: Pipewire.nodes.values

    // Defaults
    readonly property PwNode sink: Pipewire.defaultAudioSink
    readonly property PwNode source: Pipewire.defaultAudioSource
    readonly property PwNode hardwareOutput: {
        if (hardwareName === "") return null;
        return nodes.find(n => n.name === hardwareName && !n.isStream && n.isSink) ?? null;
    }
    // The node the volume keys and main slider control.
    readonly property PwNode output: hardwareOutput ?? sink

    readonly property real volume: output?.audio?.volume ?? 0
    readonly property bool muted: output?.audio?.muted ?? false
    readonly property real micVolume: source?.audio?.volume ?? 0
    readonly property bool micMuted: source?.audio?.muted ?? false

    // Device lists (no streams).
    function isChannel(n) { return (n?.name ?? "").startsWith("channel_"); }
    readonly property var sinks: nodes.filter(n => n.type === PwNodeType.AudioSink && !n.isStream && !isChannel(n))
    readonly property var channels: nodes.filter(n => n.type === PwNodeType.AudioSink && isChannel(n))
    readonly property var sources: nodes.filter(n => n.type === PwNodeType.AudioSource && !n.isStream && !isChannel(n))

    // App playback streams (the channels' own loopback playback nodes excluded).
    readonly property var allOutStreams: nodes.filter(n => n.type === PwNodeType.AudioOutStream && n.audio)
    readonly property var streams: allOutStreams.filter(n => {
        const p = n.properties ?? {};
        if (isChannel(n)) return false;
        if (p["node.passive"] === true || p["node.passive"] === "true") return false;
        return (p["application.name"] ?? "") !== "";
    })

    // Bind everything we read volume/mute/properties from.
    PwObjectTracker {
        objects: [root.sink, root.source, root.hardwareOutput].filter(n => n).concat(root.allOutStreams)
    }

    // Human names.
    function nodeName(n) {
        if (!n) return "none";
        return (n.nickname || n.description || n.name || "").toLowerCase();
    }
    function appName(n) {
        return ((n?.properties ?? {})["application.name"] ?? nodeName(n)).toLowerCase();
    }
    // The sink a playback stream is linked to (a channel or a device), or null.
    function streamTarget(n) {
        if (!n) return null;
        const g = Pipewire.linkGroups.values.find(g => g.source === n);
        return g ? g.target : null;
    }

    // Actions
    function clamp(v) { return Math.max(0, Math.min(1, v)); }
    function setVolume(v) {
        if (!output?.audio) return;
        output.audio.volume = clamp(v);
        if (v > 0 && output.audio.muted) output.audio.muted = false;
    }
    function setMicVolume(v) {
        if (!source?.audio) return;
        source.audio.volume = clamp(v);
    }
    function raise() {
        if (!output?.audio) return;
        output.audio.muted = false;
        output.audio.volume = clamp(output.audio.volume + 0.05);
        osdRequested("volume");
    }
    function lower() {
        if (!output?.audio) return;
        output.audio.volume = clamp(output.audio.volume - 0.05);
        osdRequested("volume");
    }
    function toggleMute() {
        if (!output?.audio) return;
        output.audio.muted = !output.audio.muted;
        osdRequested("volume");
    }
    function toggleMicMute() {
        if (!source?.audio) return;
        source.audio.muted = !source.audio.muted;
        osdRequested("mic");
    }
    function setStreamVolume(n, v) { if (n?.audio) n.audio.volume = clamp(v); }
    function toggleStreamMute(n) { if (n?.audio) n.audio.muted = !n.audio.muted; }
    function setDefaultSink(n) { if (n) Pipewire.preferredDefaultAudioSink = n; }
    function setDefaultSource(n) { if (n) Pipewire.preferredDefaultAudioSource = n; }

    // External changes (pwvucontrol, hardware knobs) also trigger the OSD, but
    // only after startup settles and only for real value changes on the same node.
    property bool armed: false
    Timer {
        interval: 2500
        running: Pipewire.ready
        onTriggered: root.armed = true
    }

    property var lastOut: null
    property real lastVol: -1
    property bool lastMuted: false
    property var lastSrc: null
    property bool lastMicMuted: false
    property real lastMicVol: -1

    function checkOut() {
        const same = output === lastOut;
        const changed = same && (Math.abs(volume - lastVol) > 0.001 || muted !== lastMuted);
        lastOut = output; lastVol = volume; lastMuted = muted;
        if (armed && changed) osdRequested("volume");
    }
    function checkSrc() {
        const same = source === lastSrc;
        const changed = same && (micMuted !== lastMicMuted || Math.abs(micVolume - lastMicVol) > 0.001);
        lastSrc = source; lastMicMuted = micMuted; lastMicVol = micVolume;
        if (armed && changed) osdRequested("mic");
    }
    onVolumeChanged: checkOut()
    onMutedChanged: checkOut()
    onOutputChanged: checkOut()
    onMicMutedChanged: checkSrc()
    onMicVolumeChanged: checkSrc()
    onSourceChanged: checkSrc()
}
