#!/usr/bin/env python3
"""Synthesises footstep sounds as mono 16-bit WAVs: soft, dull footfalls of a leather or cloth
sole on stone in a quiet cave.

    python3 tools/synth_footsteps.py /tmp/footsteps
    oggenc -q 6 -o game/assets/audio/step_light_1.ogg /tmp/footsteps/step_light_1.wav
    oggenc -q 6 -o game/assets/audio/step_light_2.ogg /tmp/footsteps/step_light_2.wav
    oggenc -q 6 -o game/assets/audio/step_light_3.ogg /tmp/footsteps/step_light_3.wav
    oggenc -q 6 -o game/assets/audio/step_heavy_1.ogg /tmp/footsteps/step_heavy_1.wav
    oggenc -q 6 -o game/assets/audio/step_heavy_2.ogg /tmp/footsteps/step_heavy_2.wav
    oggenc -q 6 -o game/assets/audio/step_heavy_3.ogg /tmp/footsteps/step_heavy_3.wav

    python3 tools/synth_footsteps.py --measure /tmp/footsteps   # measure existing WAVs only

Standard library only. The output is the same on every run (a fixed seed).

Each step is a low thump (a sine body of 80-180 Hz with a short downward pitch glide, a rounded
4-5 ms fade-in so there is no click, and an exponential decay of 60-120 ms) over a very faint
scuff (noise through a steep low-pass). Everything then goes through a 4-pole Butterworth
low-pass near 700 Hz, so nothing bright or hissy remains. Light and heavy steps differ in weight:
heavy is lower and slightly longer, not louder in the highs. Peaks are normalised to -12 dBFS.

After writing, the script prints each file's spectral centroid and the share of energy above
2 kHz (targets: centroid under 400 Hz, under 2% above 2 kHz).
"""

import cmath
import math
import os
import random
import struct
import sys
import wave

RATE = 44100
SEED = 1207
PEAK_DBFS = -12.0
LOWPASS_HZ = 700.0


def biquad_lowpass(samples, cutoff, q=1 / math.sqrt(2)):
    """A 2-pole (RBJ) low-pass biquad."""
    w = 2.0 * math.pi * cutoff / RATE
    alpha = math.sin(w) / (2.0 * q)
    cw = math.cos(w)
    b0 = b2 = (1.0 - cw) / 2.0
    b1 = 1.0 - cw
    a0, a1, a2 = 1.0 + alpha, -2.0 * cw, 1.0 - alpha
    x1 = x2 = y1 = y2 = 0.0
    out = []
    for x in samples:
        y = (b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0
        x2, x1, y2, y1 = x1, x, y1, y
        out.append(y)
    return out


def lowpass4(samples, cutoff):
    """A 4-pole Butterworth low-pass: two biquads with the Butterworth Q values."""
    samples = biquad_lowpass(samples, cutoff, 0.5412)
    return biquad_lowpass(samples, cutoff, 1.3066)


def thump(out, freq, tau, attack, gain):
    """Adds a sine body at `freq` Hz decaying with time constant `tau` s, with a raised-cosine
    fade-in of `attack` s and a small downward pitch glide."""
    attack_n = int(attack * RATE)
    theta = 0.0
    for i in range(min(len(out), int(tau * 8 * RATE))):
        t = i / RATE
        f = freq * (1.0 + 0.25 * math.exp(-t / 0.02))
        theta += 2.0 * math.pi * f / RATE
        fade = 0.5 - 0.5 * math.cos(math.pi * i / attack_n) if i < attack_n else 1.0
        out[i] += gain * fade * math.exp(-t / tau) * math.sin(theta)


def scuff(out, rng, length, cutoff, gain):
    """Adds a faint, heavily low-passed noise scuff under the thump."""
    n = int(length * RATE)
    noise = lowpass4([rng.uniform(-1.0, 1.0) for _ in range(n)], cutoff)
    fade_n = int(0.012 * RATE)
    for i in range(n):
        fade = 0.5 - 0.5 * math.cos(math.pi * i / fade_n) if i < fade_n else 1.0
        out[i] += gain * fade * (1.0 - i / n) ** 2 * noise[i] * 6.0


def normalise(samples, dbfs=PEAK_DBFS):
    peak = max(abs(x) for x in samples) or 1.0
    scale = 10 ** (dbfs / 20.0) / peak
    return [x * scale for x in samples]


def step(rng, freq, tau, length, scuff_gain, variation):
    """One footfall: a thump (with a faint second body a fifth-ish above), a scuff, then the
    steep low-pass over everything."""
    out = [0.0] * int(length * RATE)
    thump(out, freq * (1.0 + 0.08 * variation), tau * (1.0 + 0.15 * variation), 0.004 + 0.0008 * (variation + 1), 1.0)
    thump(out, freq * 1.9, tau * 0.6, 0.005, 0.25)
    scuff(out, rng, length * 0.6, 500.0 + 100.0 * variation, scuff_gain)
    return normalise(lowpass4(out, LOWPASS_HZ + 60.0 * variation))


def step_light(rng, variation):
    """Light: a body near 130-160 Hz, decaying over about 70 ms."""
    return step(rng, 145.0, 0.012, 0.22, 0.05, variation)


def step_heavy(rng, variation):
    """Heavy: lower (about 90-110 Hz) and slightly longer, decaying over about 110 ms."""
    return step(rng, 98.0, 0.018, 0.30, 0.06, variation)


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(x * 32767))) for x in samples))


def read(path):
    with wave.open(path, "rb") as wav:
        raw = wav.readframes(wav.getnframes())
    return [v / 32768.0 for (v,) in struct.iter_unpack("<h", raw)]


def fft(x):
    """Radix-2 FFT of a power-of-two-length list."""
    n = len(x)
    if n == 1:
        return list(x)
    even, odd = fft(x[0::2]), fft(x[1::2])
    t = [cmath.exp(-2j * math.pi * k / n) * odd[k] for k in range(n // 2)]
    return [even[k] + t[k] for k in range(n // 2)] + [even[k] - t[k] for k in range(n // 2)]


def measure(samples):
    """(spectral centroid in Hz, share of energy above 2 kHz) from the power spectrum."""
    n = 1
    while n < len(samples):
        n *= 2
    spec = fft(list(samples) + [0.0] * (n - len(samples)))[: n // 2]
    power = [abs(c) ** 2 for c in spec]
    total = sum(power) or 1.0
    freqs = [k * RATE / n for k in range(n // 2)]
    centroid = sum(f * p for f, p in zip(freqs, power)) / total
    high = sum(p for f, p in zip(freqs, power) if f > 2000.0) / total
    return centroid, high


def report(directory):
    for kind in ("light", "heavy"):
        for i in range(1, 4):
            name = f"step_{kind}_{i}.wav"
            samples = read(os.path.join(directory, name))
            centroid, high = measure(samples)
            ok = "ok" if centroid < 400.0 and high < 0.02 else "OVER TARGET"
            print(f"{name}: centroid {centroid:.0f} Hz, above 2 kHz {high * 100:.4f}% ({ok})")


def main():
    args = sys.argv[1:]
    if len(args) == 2 and args[0] == "--measure":
        report(args[1])
        return
    if len(args) != 1:
        sys.exit("usage: synth_footsteps.py [--measure] OUT_DIR")
    out = args[0]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)

    for i in range(3):
        variation = i - 1.0  # -1, 0, 1
        write(os.path.join(out, f"step_light_{i + 1}.wav"), step_light(rng, variation))
        write(os.path.join(out, f"step_heavy_{i + 1}.wav"), step_heavy(rng, variation))
    report(out)


if __name__ == "__main__":
    main()
