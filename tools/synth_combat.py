#!/usr/bin/env python3
"""Synthesises the fight's sound effects as mono 16-bit WAVs: a rat's squeak, a snake's hiss
and a weapon's whoosh through the air.

    python3 tools/synth_combat.py /tmp/combat
    oggenc -q 6 -o game/assets/audio/squeak.ogg /tmp/combat/squeak.wav
    oggenc -q 6 -o game/assets/audio/hiss.ogg /tmp/combat/hiss.wav
    oggenc -q 6 -o game/assets/audio/whoosh.ogg /tmp/combat/whoosh.wav

Standard library only. The output is the same on every run (a fixed seed).

Technique, as tools/synth_door_shut.py: everyday sounds built from oscillators, noise and
filters, the method of Andy Farnell's "Designing Sound" (MIT Press 2010).

- The squeak is a chirp: a sine whose pitch leaps up and falls back, with a fast vibrato and a
  little noise for breath, under a short swell. Most of a rat's calls are ultrasonic, beyond
  hearing (S. M. Brudzynski, "Ethotransmission: communication of emotional states through
  ultrasonic vocalization in rats", Current Opinion in Neurobiology 23, 2013), so the audible
  squeak's pitches and timings here are our own.
- The hiss is white noise through a band-pass, swelling and fading: air forced past a narrow
  opening is turbulent, broadband noise, which the filter shapes (Farnell, ch. on turbulence).
- The whoosh is noise through a band-pass whose centre sweeps up and back down as the blade
  passes, louder at its fastest.

The band-pass is Hal Chamberlin's state-variable filter ("Musical Applications of
Microprocessors", 2nd ed., Hayden 1985, ch. 14). Frequencies, Q and envelopes are our own.
"""

import math
import os
import random
import struct
import sys
import wave

RATE = 44100
SEED = 1207


def svf_bandpass(samples, centre, q):
    """Chamberlin's state-variable filter, band-pass output. `centre` is a number of Hz or a
    function of the sample index, so the band can sweep."""
    low = band = 0.0
    out = []
    damp = 1.0 / q
    for i, x in enumerate(samples):
        fc = centre(i) if callable(centre) else centre
        f = 2.0 * math.sin(math.pi * min(fc, RATE / 6.0) / RATE)
        low += f * band
        high = x - low - damp * band
        band += f * high
        out.append(band)
    return out


def swell(i, n, attack, release):
    """An envelope over `n` samples: up over the first `attack` share, down over the last
    `release` share, sine-shaped so it starts and ends in silence."""
    t = i / n
    if t < attack:
        return math.sin(0.5 * math.pi * t / attack)
    if t > 1.0 - release:
        return math.sin(0.5 * math.pi * (1.0 - t) / release)
    return 1.0


def normalise(samples, dbfs=-1.0):
    peak = max(abs(x) for x in samples) or 1.0
    scale = 10 ** (dbfs / 20.0) / peak
    return [x * scale for x in samples]


def squeak(rng):
    """Two quick chirps, the second higher: a hurt rat."""
    out = [0.0] * int(0.32 * RATE)
    for start, length, base, leap in [(0.0, 0.11, 2300.0, 1900.0), (0.13, 0.16, 2600.0, 2500.0)]:
        s0, n = int(start * RATE), int(length * RATE)
        phase = 0.0
        for i in range(n):
            t = i / n
            # Up fast, then down: a pitch arc peaking a third of the way in.
            arc = math.sin(math.pi * min(1.0, t / 0.35) * 0.5) * (1.0 - 0.6 * max(0.0, t - 0.35) / 0.65)
            f = base + leap * arc + 90.0 * math.sin(2.0 * math.pi * 38.0 * i / RATE)
            phase += 2.0 * math.pi * f / RATE
            tone = math.sin(phase) + 0.25 * math.sin(2.0 * phase)
            out[s0 + i] += swell(i, n, 0.12, 0.5) * (tone + 0.12 * rng.uniform(-1.0, 1.0))
    return normalise(out, -3.0)


def hiss(rng):
    """A long hiss, a breath pushed out and tailing away."""
    n = int(0.7 * RATE)
    noise = [rng.uniform(-1.0, 1.0) for _ in range(n)]
    # Two bands: the sharp sibilance and a breathier lower one.
    sharp = svf_bandpass(noise, 6200.0, 1.6)
    breath = svf_bandpass(noise, 2600.0, 1.2)
    out = [swell(i, n, 0.08, 0.6) * (sharp[i] + 0.5 * breath[i]) for i in range(n)]
    return normalise(out, -3.0)


def whoosh(rng):
    """A blade through the air: the band sweeps up as it speeds past and falls away."""
    n = int(0.38 * RATE)
    noise = [rng.uniform(-1.0, 1.0) for _ in range(n)]

    def speed(i):
        t = i / n
        return math.exp(-((t - 0.42) / 0.16) ** 2)

    band = svf_bandpass(noise, lambda i: 300.0 + 1500.0 * speed(i), 3.0)
    out = [band[i] * speed(i) for i in range(n)]
    return normalise(out, -4.0)


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(x * 32767))) for x in samples))


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_combat.py OUT_DIR")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)
    for name, make in [("squeak", squeak), ("hiss", hiss), ("whoosh", whoosh)]:
        write(os.path.join(out, name + ".wav"), make(rng))


if __name__ == "__main__":
    main()
