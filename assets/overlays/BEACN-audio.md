# BEACN audio for OBS: what goes where

For streaming to Twitch and recording at the same time.

## The idea
BEACN builds separate mixes from your channels. OBS picks each mix up as an audio source and puts it on its own track. Each track then has one job.

| OBS track | BEACN mix | What it carries | Used for |
|---|---|---|---|
| **1** | Audience Mix | Everything viewers hear live, music included | The live stream. Also recorded locally: your "as streamed" version |
| **2** | VOD Track | The same, without music | Twitch VOD and clips. Also recorded locally: your clean mix for uploading anywhere else |
| **3** | VOD Track 2 | Voices only (your mic and Chat) | Local recording only. Lets you fix voice levels when editing |
| **4** | VOD Track 3 | Game and other sound, no voices | Local recording only. Lets you fix game and background levels when editing |

Tracks 3 and 4 never go to Twitch. They only exist in your local recording.

## 1. BEACN routing table (in the BEACN app)
Rows are what you route to. Columns are your channels.

| | Mic Relay | Chat | Music | Browser | Game | System | Aux 1 | Aux 2 |
|---|---|---|---|---|---|---|---|---|
| **Audience Mix** | on | on | on | on | on | on | on | on |
| **VOD Track** | on | on | **off** | on | on | on | on | on |
| **VOD Track 2** | on | on | off | off | off | off | off | off |
| **VOD Track 3** | off | off | off | on | on | on | on | on |
| Voice Chat Mic | on | off | off | off | off | off | off | off |

What changed from your screenshot: VOD Track 2 and VOD Track 3 were identical to VOD Track. Now they're stems. Audience Mix, VOD Track and Voice Chat Mic stay as you had them.

Rule from BEACN: a source that isn't in the Audience Mix can't reach the VOD tracks. If you mute something to your audience, it won't be in any VOD track either.

If you don't want the extra stems, leave VOD Track 2 and 3 as they were, untick them in the setup page, and record tracks 1 and 2 only.

## 2. OBS audio sources
BEACN's own guide for OBS says: turn off OBS's own audio devices, add the BEACN mixes as the only sources, give each its own track, and don't add filters. The setup page does this for you:
- Creates a scene called **Vainnor Audio** holding four sources: BEACN Audience Mix, BEACN VOD Track, BEACN VOD Track 2, BEACN VOD Track 3.
- Matches each to the right Windows device by name, sets the volume to 0 dB, unmutes it, and turns OBS monitoring off (you already hear everything through BEACN).
- Puts each on only its own track (1, 2, 3, 4).
- Adds "Vainnor Audio" into your scenes so the four sources are always in the mixer.
- Takes OBS's own Desktop Audio and Mic/Aux off every track so nothing is doubled. It doesn't delete them.

By hand, the same thing: Audio Input Capture sources, then Advanced Audio Properties (gear icon above the mixer): each source checked on only its own track, Monitor Off.

## 3. OBS output settings (Settings > Output, mode Advanced)
- **Streaming > Audio Track:** 1
- **Streaming > Twitch VOD Track:** on, Track 2 (your Twitch account has to be connected under Settings > Stream)
- **Recording > Audio Track:** tick 1, 2, 3 and 4
- **Recording Format:** Hybrid MP4 or MKV. FLV only holds one track.
- **Output > Audio tab:** set each track's bitrate (160 to 192 kbps is plenty)

The setup page can write the first three if you're already in Advanced mode. It won't switch you from Simple to Advanced, because that resets your encoder settings. After it runs, restart OBS and check the values in Settings > Output.

Recording while streaming: if the recording uses the same encoder as the stream, the file is stream quality. For better recordings pick a separate Video Encoder under Recording. That's a video setting, so it's not touched here.

## 4. Test before going live
1. Start a recording (no need to stream).
2. Play some music through BEACN, talk, play a game sound. 30 seconds.
3. Stop and open the file in an editor or VLC. You should see four audio tracks:
   - 1: everything, music audible
   - 2: everything, no music
   - 3: your voice (and chat) only
   - 4: game and other sound, no voice
4. After your first real stream, check the Twitch VOD has no music. That confirms the VOD Track is wired to Track 2.

## Things I couldn't confirm
- BEACN's guide for OBS only covers one VOD Track on OBS Track 2. VOD Track 2 and 3 are in your screenshot, so I've used them the same way. The exact Windows device names for them aren't documented where I looked. The page matches on the names "VOD Track", "VOD Track 2", "VOD Track 3". If OBS shows different names, change the text in the setup page.
- The setup page's output-track step was tested against a stand-in for OBS. OBS may only apply those values after a restart, which is why the page asks you to restart and check.
