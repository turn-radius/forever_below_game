# Tools

Both scripts read the prototypes in `reference/` and need Node 18+ and three.js r128:

```
npm pack three@0.128.0 && tar xzf three-0.128.0.tgz   # gives package/build/three.min.js
```

**Example fixtures**: 64 seeds per Armory family, plus mulberry32 reference streams. Reference only.

```
N=64 node tools/gen_fixtures.js package/build/three.min.js reference/spline_armory_engine.js fixtures
```

`reference/spline_armory_engine.js` is the engine as embedded in the plate. If `spline-armory.html` changes, re-extract the engine from the new page (everything between the Schema section and the Three setup section, plus the build functions) or ask Claude to do it, then regenerate.

**Game tables**: the plate's data (races, slots, bases, goods, affixes, outfits, Armory mappings, fitting constants) as JSON. Functions are kept as their JS source.

```
node tools/extract_data.js reference/forever-below-plate.html data
```

**Soundtrack**: the sources in `reference/audio/` are Opus in MP4, which Bevy can't decode. They're converted to Ogg Vorbis at quality 6 in `game/assets/audio/`. Homebrew's ffmpeg has no libvorbis encoder, so ffmpeg decodes and `oggenc` (`brew install vorbis-tools`) encodes:

```
ffmpeg -i "reference/audio/The Wizards Tower.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/the-wizards-tower.ogg -
ffmpeg -i "reference/audio/Shadowed Pixel.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/shadowed-pixel.ogg -
ffmpeg -i "reference/audio/Forever Below.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/forever-below.ogg -
ffmpeg -i "reference/audio/Cavernous Arpeggios.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/cavernous-arpeggios.ogg -
ffmpeg -i "reference/audio/Unreachable Home.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/unreachable-home.ogg -
ffmpeg -i "reference/audio/Cavern Descent.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/cavern-descent.ogg -
ffmpeg -i "reference/audio/I'm not alone.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/im-not-alone.ogg -
ffmpeg -i "reference/audio/Trolls.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/trolls.ogg -
ffmpeg -i "reference/audio/Measured Pulse.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/measured-pulse.ogg -
ffmpeg -i "reference/audio/Let  me think.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/let-me-think.ogg -
ffmpeg -i "reference/audio/Meant to be here.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/meant-to-be-here.ogg -
ffmpeg -i "reference/audio/What am I doing?.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/what-am-i-doing.ogg -
ffmpeg -i "reference/audio/Am I seeing the light?.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/am-i-seeing-the-light.ogg -
ffmpeg -i "reference/audio/Bitter Pulse.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/bitter-pulse.ogg -
ffmpeg -i "reference/audio/Descenso al Infierno.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/descenso-al-infierno.ogg -
ffmpeg -i "reference/audio/Falling Apart.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/falling-apart.ogg -
ffmpeg -i "reference/audio/I should have stayed.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/i-should-have-stayed.ogg -
ffmpeg -i "reference/audio/I went too far.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/i-went-too-far.ogg -
ffmpeg -i "reference/audio/I’m right where I started.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/im-right-where-i-started.ogg -
ffmpeg -i "reference/audio/Missing Friends.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/missing-friends.ogg -
ffmpeg -i "reference/audio/Sinking Bass.m4a" -vn -f wav - | oggenc --ignorelength -q 6 -o game/assets/audio/sinking-bass.ogg -
```

**Sound effects**: there are no recorded sound effects. `tools/synth_door_shut.py` synthesises the entrance door's slam and latch by modal synthesis (decaying sine modes struck by filtered noise; the script's docstring cites the sources) with Python's standard library alone, the same on every run, and `oggenc` encodes it:

```
python3 tools/synth_door_shut.py /tmp/door-shut.wav
oggenc -q 6 -o game/assets/audio/door-shut.ogg /tmp/door-shut.wav
```

`tools/synth_combat.py` synthesises the fight's sounds the same way: a rat's squeak (a sine chirp with vibrato), a snake's hiss (band-passed noise) and a weapon's whoosh (noise through a sweeping band-pass), a rat's attack squeal (a held, climbing sine), a snake's rising hiss (band-passed noise with a climbing band) and the dull thud of a blow landing on the delver (a falling low sine with a puff of low noise), its docstring citing the sources:

```
python3 tools/synth_combat.py /tmp/combat
oggenc -q 6 -o game/assets/audio/squeak.ogg /tmp/combat/squeak.wav
oggenc -q 6 -o game/assets/audio/hiss.ogg /tmp/combat/hiss.wav
oggenc -q 6 -o game/assets/audio/whoosh.ogg /tmp/combat/whoosh.wav
oggenc -q 6 -o game/assets/audio/rat-squeal.ogg /tmp/combat/rat_squeal.wav
oggenc -q 6 -o game/assets/audio/snake-rise.ogg /tmp/combat/snake_rise.wav
oggenc -q 6 -o game/assets/audio/thud.ogg /tmp/combat/thud.wav
```

**Ambient sounds**: `tools/synth_ambience.py` synthesises ambient sounds as mono 16-bit WAVs: water drips and a deep stony rumble, by oscillators, noise and filters (the technique of Andy Farnell's "Designing Sound"), the same on every run, and `oggenc` encodes them:

```
python3 tools/synth_ambience.py /tmp/ambience
oggenc -q 6 -o game/assets/audio/drip.ogg /tmp/ambience/drip.wav
oggenc -q 6 -o game/assets/audio/rumble.ogg /tmp/ambience/rumble.wav
```

The drip is a rising sine sweep decaying over about 150 ms with proper phase accumulation, plus faint splash noise. The rumble is a deep rolling sound from decaying sine modes in the 40–120 Hz range.

**Footsteps**: `tools/synth_footsteps.py` synthesises soft, dull footfalls, like a leather or cloth sole on stone in a quiet cave: a low sine thump (80–180 Hz, rounded 4–5 ms fade-in, 60–120 ms decay) over a very faint scuff, all through a 4-pole low-pass near 700 Hz, with no broadband noise. There are 3 light variants and 3 heavy variants (lower and slightly longer, not brighter), normalised to -12 dBFS, the same on every run. `oggenc` encodes them:

```
python3 tools/synth_footsteps.py /tmp/footsteps
oggenc -q 6 -o game/assets/audio/step_light_1.ogg /tmp/footsteps/step_light_1.wav
oggenc -q 6 -o game/assets/audio/step_light_2.ogg /tmp/footsteps/step_light_2.wav
oggenc -q 6 -o game/assets/audio/step_light_3.ogg /tmp/footsteps/step_light_3.wav
oggenc -q 6 -o game/assets/audio/step_heavy_1.ogg /tmp/footsteps/step_heavy_1.wav
oggenc -q 6 -o game/assets/audio/step_heavy_2.ogg /tmp/footsteps/step_heavy_2.wav
oggenc -q 6 -o game/assets/audio/step_heavy_3.ogg /tmp/footsteps/step_heavy_3.wav
python3 tools/synth_footsteps.py --measure /tmp/footsteps
```

The script prints each file's spectral centroid and share of energy above 2 kHz (targets: centroid under 400 Hz, under 2% above 2 kHz). The earlier noisy steps measured centroids of 2600–4700 Hz with 30–52% of energy above 2 kHz. Now:

| File | Centroid | Above 2 kHz |
| --- | --- | --- |
| step_light_1 | 163 Hz | 0.00% |
| step_light_2 | 172 Hz | 0.00% |
| step_light_3 | 183 Hz | 0.00% |
| step_heavy_1 | 109 Hz | 0.00% |
| step_heavy_2 | 115 Hz | 0.00% |
| step_heavy_3 | 122 Hz | 0.00% |

In game, the delver's steps play at 0.35 of the effects volume (creatures at 0.7 of that), each with a random gain of ±15% and playback speed of ±6%. The same sample is never played twice in a row.

**Pain grunts**: `tools/synth_grunt.py` synthesises the grunt the delver gives when a blow lands, the way a throat makes one: Rosenberg glottal pulses with a little jitter and shimmer, the pitch jumping at the blow and falling away, through three formant band-passes for a vowel, with breath noise forced out at the start. There are three everyday takes ("uh", a breathier "ah", and an "ugh" that closes and creaks) and a longer hard cry, each in a low voice (from about 125 Hz) and a high one (about 220 Hz, formants a sixth higher), normalised to -3 dBFS, the same on every run. The game picks the voice by the delver's sex and nudges the speed by folk (`game/src/combat/voice.rs`).

```
python3 tools/synth_grunt.py /tmp/grunt
for f in /tmp/grunt/*.wav; do oggenc -q 6 -o game/assets/audio/$(basename $f .wav).ogg $f; done
```

**Breathing**: `tools/synth_breath.py` synthesises the delver's breath as three seamless loops, each a run of soft inhales and exhales: noise band-limited to about 300–1500 Hz (a high-pass at 300 Hz, a 4-pole low-pass and a gentle extra low-pass, so no hiss) under smooth raised-cosine envelopes, normalised to -16 dBFS, the same on every run. Steady is a slow, even breath every 4.2 s (12.6 s loop), harder is a deeper breath every 2.4 s (12.0 s) and ragged is fast and uneven with catches and a slight flutter (9.5 s). The strengths differ in pace and character, not brightness.

```
python3 tools/synth_breath.py /tmp/breath
oggenc -q 6 -o game/assets/audio/breath_steady.ogg /tmp/breath/breath_steady.wav
oggenc -q 6 -o game/assets/audio/breath_harder.ogg /tmp/breath/breath_harder.wav
oggenc -q 6 -o game/assets/audio/breath_ragged.ogg /tmp/breath/breath_ragged.wav
python3 tools/synth_breath.py --measure /tmp/breath
```

The script prints each file's peak, spectral centroid and share of energy above 2 kHz (targets: centroid under 900 Hz, under 2% above 2 kHz):

| File | Centroid | Above 2 kHz |
| --- | --- | --- |
| breath_steady | 578 Hz | 0.004% |
| breath_harder | 691 Hz | 0.04% |
| breath_ragged | 776 Hz | 0.17% |

In game (`game/src/breath.rs`) the loops are non-spatial and play at 0.25 of the effects volume, scaled up 15% per exhaustion level. Stamina below 70% breathes steadily, below 45% harder, and winded or below 20% raggedly, settling as stamina refills. Exhaustion from level 1 keeps a slow loop going under everything (steady at levels 1–2, harder at 3–4, ragged at 5–6) until the level returns to 0. Loops crossfade over 1.5 s and pause while a screen is open.

`tools/synth_heal.py` synthesises the healing draught's gentle rising chime (three soft bell notes, decaying sine modes at slightly inharmonic ratios), its docstring citing the source:

```
python3 tools/synth_heal.py /tmp/heal
oggenc -q 6 -o game/assets/audio/heal.ogg /tmp/heal/heal.wav
```

Bevy loads assets from `game/assets/` under `cargo run` (it uses `CARGO_MANIFEST_DIR`). To run the binary directly, set `BEVY_ASSET_ROOT=game`.

**Running the tests**: `tools/test.sh` builds every test target once, then runs each test binary in its own process, all at the same time, each running its tests on threads. Anything after it goes to every binary: `tools/test.sh rarity` runs tests whose names contain "rarity", and `tools/test.sh --include-ignored` adds the slow sweeps. It prints one line per binary with its count and time, then the failures in full, then the five slowest binaries, and says when the tests took longer than the budget (`FB_TEST_BUDGET`, 120 s by default). Timing checks inside tests read the thread's CPU time, not the wall clock, so a busy machine doesn't fail them.

It exists because on the work machine every process launch waits in one queue for its endpoint security check (about 0.27 s, even for `/usr/bin/true`). `cargo nextest` starts a process per test, so 1,727 tests spent about 8 minutes waiting to start; one process per binary takes a few seconds of that.
