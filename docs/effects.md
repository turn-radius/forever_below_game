# Effects

What a spell or an item shows when it goes off is data: one TOML file per effect under
`data/effects/`, read by `crates/fb_effects` (plain Rust) and played by `game/src/effects.rs`.
Edit a file while the game runs and the next play of that effect changes (a file added while it
runs shows after a restart). `data/effects/test_burst.toml` is the worked example; play it with
`FB_PLAY_EFFECT=test_burst`.

## An effect

```toml
name = "test_burst"
length = 1.6        # seconds it plays; a lasting effect loops its timeline this long until stopped
lasts = false       # false: plays once. true: holds until stopped
bloom_in = 0.15     # seconds its light blooms in over (past full, then settling)
sputter_out = 0.5   # seconds it sputters out over at the end

[light]             # optional
at = "above_target" # an anchor
peak = 1.4          # as a torch's peak
reach = 140.0       # units
colour = [1.0, 0.85, 0.5]
follows = { flash = { at_s = 0.25 } }   # "envelope", { flash = { at_s } } or "travel"

[[emitters]]
name = "sparks"
card = "spark"      # wisp, mote, spark, beam or ring
tail = 0.06
size = [{ at = 0.0, size = 0.7 }, { at = 1.0, size = 0.2 }]       # any number of points
colour = [{ at = 0.0, rgb = [1, 0.98, 0.85] }, { at = 1.0, rgb = [0.9, 0.4, 0.15] }]
[emitters.spawn]
[emitters.timing]
[emitters.motion]
```

A key left out keeps its default. Size and colour are given at shares of a particle's life, run
straight between the points and are held flat before the first and after the last. Colours are
sRGB, 0 to 1. Lengths are world units (1 unit = 10 cm).

### Anchors

`caster` (the eye), `beside_caster` (a hand), `target` (its middle), `targets_feet`,
`above_target`, `left_of_target`, `right_of_target` (as the caster sees them), `sky_above_target`
and `between` (caster to target). An emitter or light with any target anchor plays once on each
target the effect is given; one with only caster anchors plays once. `nudge = [right, up, ahead]`
moves a spawn off its anchor, in the caster's facing.

### Spawn

`shape`: `point`, `ring` (flat circle of radius `spread`), `disc`, `box` (`spread` across,
`height` tall), `column` (radius `spread`, `height`) or `path` (along the arc from `at` to `to`,
`arc` high at its middle, `spread` loose). `to` is the far end of a path and the goal of a
gathering motion.

### Timing

`kind`: `burst` (`count` at `start`), `stream` (`count` alive at once from `start` to `end`, so
`count / life` a second) or `lasting` (a stream already full at `start`). `count` is per target.
In a lasting effect (`lasts = true`) bursts go off again each `length`.

### Motion

`kind`: `still`, `rise`, `fall`, `burst` (outward at about `speed`), `drift`, `curl` (rises
winding a spiral `scatter` wide), `orbit` (a tilted ring of `orbit_radius` about the spawn
point, `orbits` turns in a life), `gather` (spawn to `to`) and `path` (spawn to `to`, bowed by
`arc`). `speed` is units a second, `scatter` strays each particle's velocity, `gravity` pulls
down, `wind` pushes.

### Cards and tail

`wisp` is a wavering heart, `mote` a soft dot, `spark` a streak stretched along its travel
(`tail` seconds of it, brightest at the head), `beam` a tall shaft on its foot (`tail` is its
height in units, `size` its width) and `ring` a flat ring on the floor (`size` its diameter).
A particle fades in over the first 6% of its life and out over the last 30%.

### Light

The light is a real point light in the level and another on the view model's stage, so what is
held is lit too. `envelope` blooms in, holds and sputters out with the effect, the same curve as
Divine Light (`fb_effects::envelope`); `flash` spikes at `at_s` and dies away; `travel` carries
the light from the caster to its anchor over the effect.

## Playing one

Send `crate::effects::PlayEffect` (`PlayEffect::new("smite", seed).on(entity, height)`,
`.at(point)`, `.from(Caster::...)`). The simulation is deterministic: the same effect, seed and
steps give the same particles.

## Adding a card

Add the variant to `fb_effects::Card`, a `WispKind` and a function in `wisp.wesl` for it
(switched on `wisp.kind`), and place it in `place_card` in `game/src/effects.rs`.
