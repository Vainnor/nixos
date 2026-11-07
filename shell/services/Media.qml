pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Services.Mpris

// The MPRIS player worth showing: the one playing now, else the one that
// played last (while it still exists), else the first one.
Singleton {
    id: root

    readonly property var players: Mpris.players.values
    property var lastPlaying: null

    readonly property MprisPlayer player: {
        const playing = players.find(p => p.isPlaying);
        if (playing) return playing;
        if (lastPlaying && players.includes(lastPlaying)) return lastPlaying;
        return players[0] ?? null;
    }

    onPlayerChanged: if (player?.isPlaying) lastPlaying = player

    // 0..1 progress, or 0 when unknown. Call player.positionChanged() on a
    // timer while showing it (MPRIS position isn't pushed).
    readonly property real progress: (player && player.length > 0) ? Math.max(0, Math.min(1, player.position / player.length)) : 0

    function togglePlaying() { if (player?.canTogglePlaying) player.togglePlaying(); }
    function next() { if (player?.canGoNext) player.next(); }
    function previous() { if (player?.canGoPrevious) player.previous(); }
    function seekTo(fraction) {
        if (player?.canSeek && player.length > 0) player.position = Math.max(0, Math.min(1, fraction)) * player.length;
    }
}
