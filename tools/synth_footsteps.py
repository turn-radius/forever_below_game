#!/usr/bin/env python3
"""Synthesises footstep sounds as mono 16-bit WAVs: varied stone footsteps with different weights.

    python3 tools/synth_footsteps.py /tmp/footsteps
    oggenc -q 6 -o game/assets/audio/step_light_1.ogg /tmp/footsteps/step_light_1.wav
    oggenc -q 6 -o game/assets/audio/step_light_2.ogg /tmp/footsteps/step_light_2.wav
    oggenc -q 6 -o game/assets/audio/step_light_3.ogg /tmp/footsteps/step_light_3.wav
    oggenc -q 6 -o game/assets/audio/step_heavy_1.ogg /tmp/footsteps/step_heavy_1.wav
    oggenc -q 6 -o game/assets/audio/step_heavy_2.ogg /tmp/footsteps/step_heavy_2.wav
    oggenc -q 6 -o game/assets/audio/step_heavy_3.ogg /tmp/footsteps/step_heavy_3.wav

Standard library only. The output is the same on every run (a fixed seed).

Technique: everyday sounds built from oscillators, noise and filters. Modal synthesis for the
step impacts (the technique of K. van den Doel, P. Kry and D. Pai, "FoleyAutomatic"), as in
tools/synth_door_shut.py.

The light footsteps are quick impacts of filtered noise with modes in the 500–2000 Hz range.
The heavy footsteps are deeper, with modes in the 200–800 Hz range and longer sustain, so a
Burdened or Encumbered delver's steps sound heavier.
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


def normalise(samples, dbfs=-1.0):
    peak = max(abs(x) for x in samples) or 1.0
    scale = 10 ** (dbfs / 20.0) / peak
    return [x * scale for x in samples]


def step_light(rng, variation):
    """A light footstep: quick impact of filtered noise with high modes."""
    n = int(0.3 * RATE)  # About 300 ms total
    out = [0.0] * n

    # Brief impact: white noise, low-passed and quick.
    impact_len = int(0.08 * RATE)
    impact = [rng.uniform(-1.0, 1.0) for _ in range(impact_len)]
    impact = one_pole_lowpass(impact, 4000.0 + variation * 500.0)
    for i in range(impact_len):
        env = math.sin(0.5 * math.pi * (1.0 - i / impact_len))
        out[i] += env * impact[i]

    # High modes for ring-out (500–2000 Hz).
    modes_table = [
        (500.0 + variation * 200.0, 0.1, 0.15),
        (900.0 + variation * 300.0, 0.08, 0.12),
        (1400.0 + variation * 400.0, 0.06, 0.1),
    ]
    modes(out, 0.0, modes_table)

    return normalise(out, -12.0)


def step_heavy(rng, variation):
    """A heavy footstep: deeper impact with lower modes and longer sustain."""
    n = int(0.4 * RATE)  # About 400 ms total
    out = [0.0] * n

    # Heavier impact: white noise, low-passed and longer.
    impact_len = int(0.12 * RATE)
    impact = [rng.uniform(-1.0, 1.0) for _ in range(impact_len)]
    impact = one_pole_lowpass(impact, 3000.0 + variation * 400.0)
    for i in range(impact_len):
        env = math.sin(0.5 * math.pi * (1.0 - i / impact_len))
        out[i] += env * impact[i]

    # Lower modes for depth (200–800 Hz), longer sustain.
    modes_table = [
        (200.0 + variation * 100.0, 0.2, 0.2),
        (450.0 + variation * 150.0, 0.18, 0.17),
        (700.0 + variation * 200.0, 0.15, 0.15),
    ]
    modes(out, 0.0, modes_table)

    return normalise(out, -12.0)


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(x * 32767))) for x in samples))


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_footsteps.py OUT_DIR")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)

    # Generate 3 light and 3 heavy variants with variations.
    for i in range(3):
        variation = (i - 1.0) / 2.0  # -0.5, 0, 0.5 for variation
        write(os.path.join(out, f"step_light_{i+1}.wav"), step_light(rng, variation))
        write(os.path.join(out, f"step_heavy_{i+1}.wav"), step_heavy(rng, variation))


if __name__ == "__main__":
    main()
