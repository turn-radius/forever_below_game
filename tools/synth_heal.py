#!/usr/bin/env python3
"""Synthesises the healing draught's sound as a mono 16-bit WAV: a gentle rising chime.

    python3 tools/synth_heal.py /tmp/heal
    oggenc -q 6 -o game/assets/audio/heal.ogg /tmp/heal/heal.wav

Standard library only. The output is the same on every run (nothing random).

Technique, as tools/synth_door_shut.py: a struck bell or chime is a few decaying sine modes
at slightly inharmonic ratios, the method of Andy Farnell's "Designing Sound" (MIT Press
2010, the chapter on bells). Three soft notes climb an A major arpeggio (E5, A5, C#6), each
a fundamental with a faint upper mode at 2.76 times its pitch and a quick, quiet attack so
the chime stays gentle. The notes, ratios, decays and gains are our own.
"""

import math
import os
import struct
import sys
import wave

RATE = 44100
NOTES = [(0.00, 659.25), (0.11, 880.00), (0.22, 1108.73)]
LENGTH = 1.1


def chime(out, start, freq):
    s0 = int(start * RATE)
    for i in range(len(out) - s0):
        t = i / RATE
        attack = min(1.0, t / 0.006)
        body = math.exp(-t * 4.5) * math.sin(2.0 * math.pi * freq * t)
        shimmer = 0.18 * math.exp(-t * 9.0) * math.sin(2.0 * math.pi * freq * 2.76 * t)
        out[s0 + i] += attack * (body + shimmer)


def heal():
    out = [0.0] * int(LENGTH * RATE)
    for start, freq in NOTES:
        chime(out, start, freq)
    # A short fade at the very end so it ends in silence.
    tail = int(0.05 * RATE)
    for i in range(tail):
        out[-1 - i] *= i / tail
    peak = max(abs(x) for x in out)
    gain = 10 ** (-6.0 / 20.0) / peak
    return [x * gain for x in out]


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(x * 32767))) for x in samples))


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_heal.py OUT_DIR")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    write(os.path.join(out, "heal.wav"), heal())


if __name__ == "__main__":
    main()
