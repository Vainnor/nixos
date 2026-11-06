// Edit this file. Everything is optional.
// OBS local-file browser sources can't take ?options in the address, so settings live here.
// The same names also work as address options if you load a page by URL (?minutes=10&debug=1).
//
// KEEP THIS FILE PRIVATE once you add your StreamElements token. Don't share or upload it.

window.VAINNOR = {
  channel: "vainnor",          // Twitch channel name (chat captions read this, no login needed)

  // Starting Soon
  minutes: 5,                  // countdown length

  // Ending
  endHold: 20,                 // seconds on the thank-you card before the picture collapses

  // Live scenes
  nowPlaying: "",              // shown in the crawl, for example "Overwatch". Empty hides it.
  discord: "",                 // for example "discord.gg/yourcode". Empty hides it.
  startTime: "",               // optional ISO time you went live, e.g. "2026-11-03T19:00:00". Empty = clock remembers the first load.
  gameplayCrawl: false,        // true adds the bottom crawl to the gameplay scenes (off = minimal)

  // Webcam toggle on the gameplay scenes.
  // camera: true/false sets the starting state by hand.
  // To follow OBS instead, turn on OBS > Tools > WebSocket Server Settings > Enable, then fill these in.
  camera: true,
  cameraSource: "",            // the exact name of your webcam source in OBS, e.g. "Webcam". Empty = don't follow OBS.
  obsUrl: "ws://127.0.0.1:4455",
  obsPassword: "",             // the WebSocket server password from OBS (leave empty if you turned authentication off)

  chatMax: 6,                  // captions on screen at once
  chatTtl: 60,                 // seconds before a caption goes away
  hideUsers: ["nightbot", "streamelements", "streamlabs", "moobot"],

  // StreamElements events (follow, sub, raid, cheer). Leave empty to run without alerts.
  seChannelId: "",             // StreamElements dashboard > Account > Channels > Channel ID
  seJwt: "",                   // StreamElements dashboard > Account > Show secrets > JWT Token

  debug: false                 // true shows a test panel with buttons for each event. Turn off before streaming.
};
