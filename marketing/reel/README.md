# The launch reel

Two cuts of one idea, speed you can feel, under the line "Faster than you can gasp.":

- **Hero**: 16:9, 1920×1080, about 42 seconds, 60 fps, H.264 with AAC.
- **Vertical**: 9:16, 1080×1920, 15 seconds, the same codecs. Type sits in the top third, the app in the centre band, and the bottom fifth stays clear for the platforms' own buttons.

Every shot of the app is a recording of the real app, driven by scripts on a virtual display. The rendered MP4s aren't committed. This folder holds everything that makes them.

## What's in it

| Path | What it does |
|---|---|
| `capture/make_vault.py` | Builds the scratch vault: `fixtures/corpus`, a generated archive of 5,000 notes from `gasp-corpus`, the showcase notes in `capture/notes/`, a `.gasp/` with a red accent, and a git clone of a local bare repository for sync |
| `capture/install_charter.py` | Installs Charter (XCharter from CTAN, renamed) so the Linux footage uses the app's default font instead of a fallback |
| `capture/grab.c` | The screen recorder (see "Capture" below) |
| `capture/take.py`, `capture/scenarios.py` | Record one take: start the recorder, launch `gasp`, drive it with xdotool, stop |
| `capture/mcp_call.py`, `capture/agent_calls.json` | Play an agent: run `gasp mcp <vault>` and send real `patch_note` calls |
| `capture/measure.py` | Measure the numbers the reel shows |
| `capture/unpack.py`, `capture/record_all.sh` | Record takes and unpack them into frames for the compositor |
| `compose/prep.py` | Gather takes, timings and numbers into `data.json`, and find the caret in every frame so the camera can follow it |
| `compose/stage.html`, `compose/reel.js` | The composition: every shot, camera move, type card, counter, the waveform and the whale, as a function of time |
| `compose/render.js` | Render frames with headless Chromium (Playwright) and pipe them to ffmpeg |
| `audio/synth.py` | Synthesize the soundtrack from the cue list the composition writes |
| `render.sh` | Render and mux both cuts |
| `compose/sheet.py`, `compose/events.py` | Contact sheets and take listings, for checking |

## Re-rendering

You need a Linux machine with Xvfb, openbox, xdotool, ffmpeg, ImageMagick, a C compiler with the X11, Xext, Xfixes, Xdamage and LZ4 libraries, Python 3 with numpy, Pillow, scipy and fontTools, and Node with Playwright's Chromium. Lavapipe (Mesa's software Vulkan) is enough for the app.

```sh
W=/tmp/reel                                   # any scratch folder
export CARGO_TARGET_DIR=/tmp/target-app
cargo build --release -p gasp-desktop -p gasp-corpus
export GASP=/tmp/target-app/release/gasp

# The recorder and the font.
cc -O2 -o $W/grab marketing/reel/capture/grab.c -lX11 -lXext \
  /usr/lib/x86_64-linux-gnu/libXfixes.so.3 /usr/lib/x86_64-linux-gnu/libXdamage.so.1 \
  /usr/lib/x86_64-linux-gnu/liblz4.so.1
export GRAB=$W/grab
mkdir -p $W/charter && for s in Roman Bold Italic BoldItalic; do
  curl -sSfLo $W/charter/XCharter-$s.otf https://mirrors.ctan.org/fonts/xcharter/opentype/XCharter-$s.otf; done
python3 marketing/reel/capture/install_charter.py $W/charter /usr/local/share/fonts/charter && fc-cache -f

# The vault, the numbers, and the takes.
python3 marketing/reel/capture/make_vault.py . $W/vault /tmp/target-app/release/gasp-corpus 5000
V="$W/vault/Field Notes"
python3 marketing/reel/capture/measure.py "$V" $W/measure.json fixtures/corpus 10
marketing/reel/capture/record_all.sh "$V" $W cold_start:cold_start_10 typing:typing_2 math:math_2 \
  table:table_2 search:search_4 accent:accent_1 keymap:keymap_1 snippet:snippet_1 sync:sync_2 \
  agent:agent_2 tagline:tagline_2

# Picture, sound, and the two MP4s.
marketing/reel/render.sh $W $W/out
```

The take names are the ones `render.sh` and `reel.js` expect. Record a scenario several times under different names, look at the unpacked frames (`$W/frames/<take>/`) and the preview `$W/frames/<take>.mp4`, and rename the best. Before the first take, launch the app once with the take's home (`$W/takes/home-seed`) so Mesa's shader cache is warm, as it would be on any machine that has run the app before: `take.py` copies that home for every take.

A render takes about 10 minutes for the hero cut on 4 cores. `render.js ... stills <dir> <t> ...` renders single frames for checking.

## Capture

The footage was recorded on Linux in a 4-core cloud VM with no GPU. The app draws through Lavapipe, Mesa's software Vulkan, on a 2560×1600 Xvfb screen at a scale factor of 2, so text is sharp when the camera pushes in.

Two things about that machine shaped the capture:

- **The recorder.** Nothing on the machine can encode 2560×1600 at 60 fps while the app renders in software, and grabbing every frame slows the app down through the X server. `grab.c` runs a fixed 60 Hz clock, grabs the screen over MIT-SHM only on the ticks after the X server reports damage, and keeps only frames that changed, LZ4-compressed, with the tick they were grabbed on. `unpack.py` turns that back into a constant 60 fps timeline. Nothing is interpolated.
- **Paced takes.** Software rendering draws about three frames a second at this size, so keys typed at human speed would land several to a frame. Every take except the launch is paced: each key or pointer step is sent alone, the take waits half a second for the app's frame, and a nominal clock advances by the step's human duration. The compositor plays the recording on that nominal clock, so each key's own frame appears when the key lands. The frames are all the app's; only the waits between them are shortened. The end card's footnote says so.

The launch is the exception: one real-time take, recorded unpaced, with the counter running from `exec` to the first recorded frame with the note on screen.

## The numbers

All measured here, on the machine above, and printed in the end card's footnote:

- **Launch, 420 ms**: the recorded take, from `exec` to the first recorded frame showing the note. The app's own trace (`EDITOR_TRACE_STARTUP`) put the first frame at 384 ms after `main` in that run. Ten unrecorded runs (`measure.py`) had a median of 438 ms and a minimum of 354 ms.
- **Keystroke, 2.1 ms**: `gasp --bench-layout fixtures/corpus --keystrokes 300`, the median input-to-paint time over 300 keys typed into the corpus joined into one 5,026-line note. It's the app's work from the key event to a painted frame; it doesn't include the display's own latency.
- **Search, 17 ms**: the app's `search-query` trace for the full query "echo" in the recorded take, across a vault of 5,208 notes.
- The threshold on screen is Jakob Nielsen's 0.1 second, the limit for a response to feel instant.

## Type, colour and sound

- **Display face: Bricolage Grotesque** (Mathieu Triay, SIL OFL, on Google Fonts). It's variable in optical size, width and weight. At large optical sizes it tightens and grows ink traps, so the short cards read as one dense shape, and its width axis lets each word arrive condensed and breathe out to full width as it surfaces. Its tabular figures set the counters, so digits don't jitter as they count. It sits well beside the app's Charter without imitating a serif. The agent's terminal pane uses JetBrains Mono, because it shows code.
- **Palette**: paper `#F3EFE6`, ink `#1B1A17`, and the caret red `#C02B4A` from the icon, used for the caret, the counters' bars, the waveform's playhead and the URL, never as a fill.
- **Motif**: the inhale's waveform draws across the top at the open and again at the replay, where eight features flash by inside one breath. The whale breaches out of the note's text lines: they part around it, letters spray, and it lands as the icon. The icon's text lines and red caret sit under every card as a waterline.
- **Sound**: `audio/synth.py` makes everything from seeded noise and oscillators: a dry 124 BPM track (kick, closed hats, clicks, a sub pulse), key clicks at the times keys were pressed, a clap on section cuts, the inhale (filtered noise through moving vocal-tract formants, cut off by a glottal catch), and the whoosh, splash and low hit of the breach. Picture cuts sit on the beat grid (60/124 s). The mix is normalized to −14 LUFS integrated with peaks under −1 dBTP.

## Credits

The whale in Gasp's icon is drawn from a 3D humpback model by Susana Gutarra Díaz, Thomas L. Stubbs, Benjamin C. Moon, Colin Palmer and Michael J. Benton (doi:10.5281/zenodo.5979631), under CC BY 4.0. See `apps/desktop/assets/icon/THIRD_PARTY_NOTICES.txt`. The end card credits it.

Bricolage Grotesque and JetBrains Mono are under the SIL Open Font License. XCharter is under Bitstream's Charter licence.

## Known compromises

- The footage is Linux only, rendered in software. The end card names macOS, Windows, Linux and iPhone as a line of type rather than showing screens that weren't recorded.
- Paced capture: see above.
- The soundtrack is synthesized. A licensed track at 124 BPM can replace it: the cut points are on that grid, and `render.sh` muxes whatever `audio-<format>.wav` holds.
- `gasp.app` is a placeholder address; nothing in the repository names one yet.
- The sync take pushes to a local bare repository over `file://`, not GitHub.
