#!/usr/bin/env python3
"""Synthesises ambient sounds as mono 16-bit WAVs: water drips and a deep stony rumble.

    python3 tools/synth_ambience.py /tmp/ambience
    oggenc -q 6 -o game/assets/audio/drip.ogg /tmp/ambience/drip.wav
    oggenc -q 6 -o game/assets/audio/rumble.ogg /tmp/ambience/rumble.wav

Standard library only. The output is the same on every run (a fixed seed).

Technique: everyday sounds built from oscillators, noise and filters, the method of Andy
Farnell's "Designing Sound" (MIT Press 2010). Modal synthesis for the rumble (K. van den
Doel, P. Kry and D. Pai, "FoleyAutomatic"), as in tools/synth_door_shut.py.

- The drip is a rising sine-sweep (800–2000 Hz) decaying over about 150 ms with proper phase
  accumulation, plus a faint splash of white noise, bright and short.
- The rumble is a deep rolling sound from modes in the 40–120 Hz range that sustain for about
  5 seconds and fade smoothly, the stone settling far below.
"""

import math
import os
import random
import struct
import sys
import wave

RATE = 44100
SEED = 1207


def one_pole_lowpass(samples, cutoff):
    """A one-pole low-pass at `cutoff` Hz (y += a * (x - y))."""
    a = 1.0 - math.exp(-2.0 * math.pi * cutoff / RATE)
    y, out = 0.0, []
    for x in samples:
        y += a * (x - y)
        out.append(y)
    return out


def modes(out, start, table):
    """Adds decaying sine modes to `out` from `start` seconds: each (freq Hz, decay s, gain).
    Every mode starts from rest (sine phase 0), as a struck body does."""
    s0 = int(start * RATE)
    for freq, decay, gain in table:
        n = min(len(out) - s0, int(decay * 8 * RATE))
        theta = 0.0
        for i in range(n):
            t = i / RATE
            theta += 2.0 * math.pi * freq / RATE
            out[s0 + i] += gain * math.exp(-t / decay) * math.sin(theta)


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


def drip(rng):
    """A drop hitting water: a rising sine sweep decaying fast, plus splash noise."""
    n = int(0.2 * RATE)  # About 200 ms total
    out = [0.0] * n

    # Rising sine sweep from 800 Hz to 2000 Hz with decay over ~150 ms (5 time constants).
    decay_time = 0.03
    decay_samples = int(5 * decay_time * RATE)
    theta = 0.0
    for i in range(decay_samples):
        t = i / RATE
        # Sweep frequency: starts at 800 Hz, ends at 2000 Hz
        freq = 800.0 + (2000.0 - 800.0) * (t / (5 * decay_time))
        # Accumulate phase as an integral of frequency.
        theta += 2.0 * math.pi * freq / RATE
        # Exponential decay envelope
        amplitude = math.exp(-t / decay_time)
        out[i] += amplitude * math.sin(theta)

    # Faint splash: white noise, high-passed and short.
    splash_len = int(0.05 * RATE)
    splash = [rng.uniform(-1.0, 1.0) * 0.2 for _ in range(splash_len)]
    splash = one_pole_lowpass(splash, 6000.0)
    for i in range(splash_len):
        env = math.sin(0.5 * math.pi * (1.0 - i / splash_len))
        out[i] += env * splash[i]

    return normalise(out, -6.0)


def rumble(rng):
    """A deep rumble: modes in the 40–120 Hz range, sustaining and fading over 5 seconds."""
    n = int(5.5 * RATE)  # About 5.5 seconds
    out = [0.0] * n

    # Modes that cover the rumble range, with longer decay times.
    modes_table = [
        (45.0, 4.5, 0.2),
        (67.0, 4.2, 0.18),
        (92.0, 3.9, 0.15),
        (118.0, 3.5, 0.12),
    ]
    modes(out, 0.0, modes_table)

    # Smooth fade-out over the last 1.5 seconds.
    fade_start = int(4.0 * RATE)
    fade_len = int(1.5 * RATE)
    for i in range(fade_len):
        fade = math.sin(0.5 * math.pi * (1.0 - i / fade_len))
        out[fade_start + i] *= fade

    return normalise(out, -6.0)


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(x * 32767))) for x in samples))


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_ambience.py OUT_DIR")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)
    for name, make in [("drip", drip), ("rumble", rumble)]:
        write(os.path.join(out, name + ".wav"), make(rng))


if __name__ == "__main__":
    main()
