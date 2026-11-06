# Vainnor overlays: three themes

Every theme has the same pages and behaves the same way. Only the look changes.

| Theme | Look |
|---|---|
| `themes/mono` | Black and white. Wide heavy type (Archivo), plain sans (Atkinson Hyperlegible). |
| `themes/horror` | Warm black, bone white, blood red. Gothic blackletter titles (Pirata One), serif text (Cormorant Garamond). Moon, graveyard horizon. |
| `themes/purple` | Deep violet and pastel purple. Wide bold type (Unbounded), clean sans (DM Sans). Moon, stars, graveyard horizon. |

All of them tell the same story: a ghost station that keeps coming through.

## What's in each theme folder
| File | Use | Transparent? |
|---|---|---|
| `starting-soon.html` | Starting Soon, with countdown | No |
| `gameplay.html` | Gameplay for an **ultrawide** game, letterboxed. Minimal: on-air plate, clock, webcam bezel (toggleable), station mark. No chat. | Yes (game strip) |
| `gameplay-nocam.html` | Same, no webcam | Yes (game strip) |
| `gameplay-16x9.html` | Same minimal overlay for a 16:9 game filling the frame | Yes |
| `chatting.html` | Just Chatting: big webcam bezel, chat as captions | Cam area only |
| `brb.html` | Be Right Back | No |
| `ending.html` | Ending (picture collapses to a dot) | No |
| `alerts.html` | Follow / sub / raid / cheer reactions. Add on top of any scene | Yes |
| `offline.html` | Offline screen | No |
| `panels.html` | Source for the panel images | |
| `export/panels/` | Seven panel headers, 320 px and 640 px wide | |
| `export/offline-1920x1080.png` | Twitch offline image | |

Shared by all themes, in the main folder:
- `setup-obs.html`: one-click OBS setup (below), including BEACN audio.
- `BEACN-audio.md`: which BEACN mix goes on which OBS track, and what to record where.
- `js/config.js`: **your settings.** One file for every theme.
- `panels.md`: the text for your Twitch panels.

## Fastest way: setup-obs.html
1. In OBS: Tools > WebSocket Server Settings > Enable WebSocket server, and note the password.
2. Stop streaming and recording (OBS won't change the canvas while live).
3. Open `setup-obs.html` in Chrome or Edge. Enter the password.
4. Pick a **theme**.
5. Enter the full path of **this folder** (the one containing `themes` and `js`).
6. Optionally type the exact names of your game capture and webcam sources. If they exist, they're placed and layered for you.
7. Press **Connect**, then **Preview**, then **Create**.

It can also set up your BEACN audio (section 4 on the page): four audio sources on tracks 1 to 4, OBS's own audio taken off the tracks, and, if you're already in Advanced output mode, the stream, Twitch VOD and recording tracks. See `BEACN-audio.md` for the routing table to set in the BEACN app.

It only adds scenes and sources. Existing ones are skipped and nothing is deleted. Scenes are named with the theme (for example "Vainnor Horror Gameplay"), so you can run it again for another theme and keep all of them. It sets the canvas and output to 1920x1080 and leaves your frame rate alone.

## Or set up by hand (1920x1080 canvas, ultrawide game letterboxed)
0. Settings > Video: Base and Output resolution **1920x1080**. Your game can still run at 3440x1440.
1. For each scene: Sources > + > Browser, tick **Local file**, pick the HTML file from `themes/<theme>/`, Width **1920**, Height **1080**.
2. Tick **Shutdown source when not visible** and **Refresh browser when scene becomes active**. This restarts the countdown, power-on and ending each time. For smoother motion tick **Use custom frame rate** and set 60.
3. Leave the custom CSS box empty.

### Layer order
- **Gameplay**: game capture at the bottom, webcam above it, then `gameplay.html`, then `alerts.html`.
  - Game capture: right-click > Transform > **Fit to screen**. OBS places the 21:9 picture in the middle (about 1920 x 804) and leaves 138 px bars above and below. The overlay fills those bars.
  - Webcam: 480 x 270 at x 64, y 622, inside the bezel.
  - Camera off all stream: use `gameplay-nocam.html`.
  - 16:9 game: use `gameplay-16x9.html` (webcam 480 x 270 at x 64, y 650).
- **Just Chatting**: webcam below `chatting.html`, then `alerts.html`. Webcam: 1180 x 664 at x 90, y 170 (the overlay has a hole there).
- **Starting Soon, BRB, Ending, Offline**: use the page on its own. Add `alerts.html` on top if you want alerts there.

## Webcam on or off during a stream
The bezel can follow your webcam source in OBS. Toggle the eye next to the webcam (or use a hotkey or Stream Deck) and the bezel and LIVE FEED tab disappear or return with a short flicker.
1. OBS > Tools > WebSocket Server Settings > Enable WebSocket server. Set a password or turn authentication off.
2. In `js/config.js` set `cameraSource` to the **exact name** of your webcam source and `obsPassword` to the password.
3. Refresh the source. The webcam can sit directly in the scene or inside a nested scene.
4. If the bezel never hides, the name doesn't match.
Without this, `camera: true/false` in `config.js` sets it by hand, or use `gameplay-nocam.html`.

## Settings (`js/config.js`)
Open it in any text editor. OBS local files can't take `?options`, so settings live here.
- `channel`: your Twitch name. Chat captions (Just Chatting only) read your public chat with no login.
- `minutes`: countdown length on Starting Soon.
- `endHold`: seconds on the Ending card before the picture collapses.
- `nowPlaying`, `discord`: shown in the crawl when set. Empty hides them.
- `gameplayCrawl`: `true` adds the bottom crawl to the gameplay scenes. Off by default.
- `startTime`: optional ISO time you went live. If empty, the on-air clock remembers the first load and restarts after 8 hours of no use.
- `chatMax`, `chatTtl`, `hideUsers`: how many captions, how long they stay, which bots to hide.
- `camera`, `cameraSource`, `obsUrl`, `obsPassword`: the webcam toggle above.

## Alerts (StreamElements)
1. StreamElements dashboard > Account > Channels: copy your **Channel ID**.
2. Account > Show secrets: copy the **JWT Token**.
3. Paste both into `js/config.js` (`seChannelId`, `seJwt`).
4. **The JWT works like a password.** Don't share `config.js` or put it online once it's filled in.

Reactions: follow (ghost double-image and tear lines), sub (slow interference roll), raid (bars surge), cheer (static burst and flash). The signal meter on the live scenes dips on any event. Events queue, so several at once play one after another.

## Test without waiting for real events
Set `debug: true` in `config.js`, refresh the source, and a row of buttons appears (Follow, Sub, Raid, Cheer, Chat, Toggle cam, Reset clock). Set it back to `false` before streaming.

## Twitch uploads
- Panels: `themes/<theme>/export/panels/` (use the 640 px files for sharper panels). Text is in `panels.md`.
- Offline image: `themes/<theme>/export/offline-1920x1080.png`.

## What each piece means
- Status plate: the state of the signal (ACQUIRING, ON AIR, SIGNAL LOST, TRANSMISSION ENDS, OFF AIR).
- Signal meter: how strong the signal is.
- Test bars: the station is idle. They fade out once you're on air.
- Grain, scanlines, rolling bar, dark rounded edge: the old screen. Never drawn over your game.
- Ghosting and the faint veiled figure: the signal arriving twice.
- Moon and horizon (horror and purple): where the ghost stands.
- Bezel and LIVE FEED label: your cam is the feed.
- Captions: chat as closed captions.
- Crawl: real info only (now playing, links).
- Vainnor mark, bottom right: the station bug.

## Audio
See `BEACN-audio.md`. In short: Track 1 = Audience Mix (stream), Track 2 = VOD Track (Twitch VOD, no music), Track 3 = voices only, Track 4 = game and other sound. Record tracks 1 to 4 while you stream.

## Not tested live
This was built and rendered in a sandbox with no access to Twitch, StreamElements or OBS. I checked every page in every theme, the fake chat, every alert through the debug panel, the webcam toggle and the setup page against a stand-in for OBS. The real chat connection, real StreamElements events and the real OBS setup haven't run yet. Test one follow and the setup page once before going live, and check each scene in OBS afterwards.
