#!/usr/bin/env python3
"""Synthesises the entrance door's slam and latch as a mono 16-bit WAV.

    python3 tools/synth_door_shut.py /tmp/door-shut.wav
    oggenc -q 6 -o game/assets/audio/door-shut.ogg /tmp/door-shut.wav

Standard library only. The output is the same on every run (a fixed seed).

Technique: modal synthesis. A struck object rings as a sum of exponentially decaying sine
modes, each with its own frequency, damping and gain, excited by a short contact force
(Perry Cook, "Real Sound Synthesis for Interactive Applications", A K Peters 2002, ch. 4;
K. van den Doel, P. Kry and D. Pai, "FoleyAutomatic: Physically-based Sound Effects for
Interactive Simulation and Animation", SIGGRAPH 2001). Wood is heavily damped, so a door's
modes are low and die in a tenth of a second; a latch is small steel, so its modes are high and
ring for a few hundredths. The contact itself is a short burst of low-passed noise; building
everyday sounds from noise, filters and resonators this way is the method of Andy Farnell's
"Designing Sound" (MIT Press 2010). The frequencies, dampings and timings below are our own
picks for a heavy plank door in a stone frame, as is the passage's echo: a few delayed,
darkened copies of the dry sound, a much simpler thing than a reverberator.
"""

import math
import random
import struct
import sys
import wave

RATE = 44100
LENGTH = 0.9  # seconds
SEED = 1207


def one_pole_lowpass(samples, cutoff):
    """A one-pole low-pass at `cutoff` Hz (y += a * (x - y))."""
    a = 1.0 - math.exp(-2.0 * math.pi * cutoff / RATE)
    y, out = 0.0, []
    for x in samples:
        y += a * (x - y)
        out.append(y)
    return out


def noise_burst(rng, seconds, cutoff):
    """A contact force: white noise under a fast attack and decay, low-passed at `cutoff`."""
    n = int(seconds * RATE)
    raw = [rng.uniform(-1.0, 1.0) * math.sin(math.pi * i / n) ** 2 for i in range(n)]
    return one_pole_lowpass(raw, cutoff)


def modes(out, start, table, drop=0.0):
    """Adds decaying sine modes to `out` from `start` seconds: each (freq Hz, decay s, gain).
    Every mode starts from rest (sine phase 0), as a struck body does, so the onset adds no
    click of its own. `drop` bends every mode's pitch down by that fraction over its first
    decay, as a panel under a hard blow settles."""
    s0 = int(start * RATE)
    for freq, decay, gain in table:
        n = min(len(out) - s0, int(decay * 8 * RATE))
        theta = 0.0
        for i in range(n):
            t = i / RATE
            f = freq * (1.0 + drop * math.exp(-t / decay))
            theta += 2.0 * math.pi * f / RATE
            out[s0 + i] += gain * math.exp(-t / decay) * math.sin(theta)


def strike(out, start, force, gain):
    """Adds a contact force to `out` at `start` seconds."""
    s0 = int(start * RATE)
    for i, x in enumerate(force):
        if s0 + i < len(out):
            out[s0 + i] += gain * x


def echoes(dry, taps, cutoff):
    """Dry signal plus delayed, darkened copies: each tap (delay s, gain)."""
    dark = one_pole_lowpass(dry, cutoff)
    out = list(dry)
    for delay, gain in taps:
        d = int(delay * RATE)
        for i in range(d, len(out)):
            out[i] += gain * dark[i - d]
    return out


def synthesise():
    rng = random.Random(SEED)
    out = [0.0] * int(LENGTH * RATE)

    # The slam: the leaf's weight meeting the frame. A deep thump, then the planks' modes.
    thump = [(58.0, 0.16, 0.9), (91.0, 0.12, 0.6), (137.0, 0.09, 0.45)]
    planks = [
        (176.0, 0.075, 0.42), (243.0, 0.06, 0.36), (318.0, 0.05, 0.3), (402.0, 0.045, 0.24),
        (517.0, 0.035, 0.2), (655.0, 0.03, 0.15), (838.0, 0.022, 0.11), (1090.0, 0.016, 0.07),
    ]
    modes(out, 0.0, thump, drop=0.12)
    modes(out, 0.0, planks, drop=0.04)
    strike(out, 0.0, noise_burst(rng, 0.012, 2400.0), 1.4)
    # Boards knocking in the frame a moment later, softer.
    modes(out, 0.034, [(f * 1.07, d * 0.7, g * 0.35) for f, d, g in planks[:5]])
    strike(out, 0.034, noise_burst(rng, 0.006, 1800.0), 0.5)

    # The latch: the bar dropping into its keeper, then seating.
    latch = [(2140.0, 0.024, 0.16), (3410.0, 0.018, 0.12), (5230.0, 0.012, 0.09), (7870.0, 0.007, 0.06)]
    modes(out, 0.13, latch)
    strike(out, 0.13, noise_burst(rng, 0.0015, 9000.0), 0.35)
    modes(out, 0.185, [(f * 0.94, d * 0.8, g * 0.55) for f, d, g in latch])
    strike(out, 0.185, noise_burst(rng, 0.001, 8000.0), 0.2)
    # The keeper's iron answering in the wood, low and short.
    modes(out, 0.13, [(410.0, 0.03, 0.08), (690.0, 0.02, 0.05)])

    # The stone passage throws it back a few times.
    out = echoes(out, [(0.023, 0.32), (0.041, 0.22), (0.067, 0.15), (0.103, 0.09)], 1600.0)

    # A short fade so the tail ends in silence, then normalise to -1 dBFS.
    fade = int(0.15 * RATE)
    for i in range(fade):
        out[-1 - i] *= i / fade
    peak = max(abs(x) for x in out) or 1.0
    scale = 10 ** (-1.0 / 20.0) / peak
    return [x * scale for x in out]


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_door_shut.py OUT.wav")
    samples = synthesise()
    with wave.open(sys.argv[1], "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(x * 32767))) for x in samples))


if __name__ == "__main__":
    main()
