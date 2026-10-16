#!/usr/bin/env python3
"""Synthesises the delver's breathing as mono 16-bit WAV loops: a soft inhale and exhale of
muted breath noise, at three strengths.

    python3 tools/synth_breath.py /tmp/breath
    oggenc -q 6 -o game/assets/audio/breath_steady.ogg /tmp/breath/breath_steady.wav
    oggenc -q 6 -o game/assets/audio/breath_harder.ogg /tmp/breath/breath_harder.wav
    oggenc -q 6 -o game/assets/audio/breath_ragged.ogg /tmp/breath/breath_ragged.wav

    python3 tools/synth_breath.py --measure /tmp/breath   # measure existing WAVs only

Standard library only. The output is the same on every run (a fixed seed).

Each breath is noise band-limited to roughly 300-1500 Hz (a 4-pole low-pass at the breath's top
frequency, a 2-pole high-pass at 300 Hz and a gentle extra low-pass), shaped by a smooth
raised-cosine envelope: a shorter inhale, a pause, a longer and softer exhale. The files loop:
every breath fades from and to silence, so the seam is silent. The strengths differ in pace and
character, not in brightness:

    steady   slow and even, a breath every 4.2 s, three to a loop
    harder   a breath every 2.4 s, deeper, the exhale a little louder
    ragged   a breath every 1.6 s on average, uneven in length and depth, with a catch partway through
             some inhales and a slight flutter in the exhale

Peaks are normalised to -16 dBFS. After writing, the script prints each file's spectral centroid
and the share of energy above 2 kHz (targets: centroid under 900 Hz, under 2% above 2 kHz).
"""

import cmath
import math
import os
import random
import struct
import sys
import wave

RATE = 44100
SEED = 2310
PEAK_DBFS = -16.0
HIGHPASS_HZ = 300.0


def biquad(samples, b, a):
    """Direct form I biquad with normalised coefficients b=(b0,b1,b2), a=(a1,a2)."""
    x1 = x2 = y1 = y2 = 0.0
    out = []
    for x in samples:
        y = b[0] * x + b[1] * x1 + b[2] * x2 - a[0] * y1 - a[1] * y2
        x2, x1, y2, y1 = x1, x, y1, y
        out.append(y)
    return out


def lowpass2(samples, cutoff, q=1 / math.sqrt(2)):
    w = 2.0 * math.pi * cutoff / RATE
    alpha = math.sin(w) / (2.0 * q)
    cw = math.cos(w)
    a0 = 1.0 + alpha
    b = ((1.0 - cw) / 2.0 / a0, (1.0 - cw) / a0, (1.0 - cw) / 2.0 / a0)
    return biquad(samples, b, (-2.0 * cw / a0, (1.0 - alpha) / a0))


def highpass2(samples, cutoff, q=1 / math.sqrt(2)):
    w = 2.0 * math.pi * cutoff / RATE
    alpha = math.sin(w) / (2.0 * q)
    cw = math.cos(w)
    a0 = 1.0 + alpha
    b = ((1.0 + cw) / 2.0 / a0, -(1.0 + cw) / a0, (1.0 + cw) / 2.0 / a0)
    return biquad(samples, b, (-2.0 * cw / a0, (1.0 - alpha) / a0))


def lowpass4(samples, cutoff):
    """A 4-pole Butterworth low-pass."""
    return lowpass2(lowpass2(samples, cutoff, 0.5412), cutoff, 1.3066)


def bump(t, length):
    """A raised-cosine hill from 0 at t=0 up to 1 and back to 0 at t=length."""
    if t <= 0.0 or t >= length:
        return 0.0
    return math.sin(math.pi * t / length) ** 2


def add_breath(env, start, inhale, pause, exhale, in_gain, out_gain, catch=None):
    """Adds one breath's envelope starting at `start` seconds. `catch` = (at fraction of the
    inhale, depth) dips the inhale briefly, as a ragged breath does."""
    for i in range(int(inhale * RATE)):
        v = in_gain * bump(i / RATE, inhale * 1.0) ** 0.8
        if catch:
            at, depth = catch
            d = (i / RATE) / inhale - at
            v *= 1.0 - depth * math.exp(-(d * d) / 0.004)
        idx = int(start * RATE) + i
        if idx < len(env):
            env[idx] += v
    ex_start = start + inhale + pause
    for i in range(int(exhale * RATE)):
        t = i / RATE
        # A quick swell, then a long soft tail.
        v = out_gain * bump(t, exhale * 1.0) ** 1.4 * (1.0 - 0.35 * t / exhale)
        idx = int(ex_start * RATE) + i
        if idx < len(env):
            env[idx] += v


def breath_loop(rng, spec):
    """One loop: `spec` is a list of (inhale, pause, exhale, in_gain, out_gain, catch, gap)
    breaths, played in a row, each followed by a silent `gap` seconds."""
    total = sum(s[0] + s[1] + s[2] + s[6] for s in spec)
    n = int(total * RATE)
    env = [0.0] * n
    t = 0.0
    for inhale, pause, exhale, in_gain, out_gain, catch, gap in spec:
        add_breath(env, t, inhale, pause, exhale, in_gain, out_gain, catch)
        t += inhale + pause + exhale + gap
    noise = [rng.uniform(-1.0, 1.0) for _ in range(n)]
    return env, noise


def voice(env, noise, top_hz, flutter_hz=0.0, flutter_depth=0.0):
    """Band-limits the noise, shapes it by the envelope. The envelope also opens the filter a
    little at the peak of each breath, so quiet edges are the dullest part."""
    low = highpass2(lowpass4(noise, top_hz), HIGHPASS_HZ)
    # A gentle extra low-pass so nothing near the top edge stays hissy.
    soft = lowpass2(low, top_hz * 0.8)
    out = []
    for i, (e, s) in enumerate(zip(env, soft)):
        f = 1.0
        if flutter_depth:
            f -= flutter_depth * (0.5 + 0.5 * math.sin(2.0 * math.pi * flutter_hz * i / RATE))
        out.append(e * f * s)
    return out


def normalise(samples, dbfs=PEAK_DBFS):
    peak = max(abs(x) for x in samples) or 1.0
    scale = 10 ** (dbfs / 20.0) / peak
    return [x * scale for x in samples]


def steady(rng):
    """Slow and even: three breaths of 4.2 s."""
    spec = [(1.5, 0.12, 2.1, 0.7, 0.9, None, 0.48), (1.6, 0.14, 2.1, 0.75, 0.95, None, 0.36), (1.5, 0.1, 2.2, 0.7, 0.9, None, 0.38)]
    env, noise = breath_loop(rng, spec)
    return normalise(voice(env, noise, 1000.0))


def harder(rng):
    """Faster and deeper: five breaths of about 2.4 s."""
    spec = [
        (0.85, 0.05, 1.2, 0.85, 1.0, None, 0.3),
        (0.9, 0.05, 1.15, 0.9, 1.0, None, 0.3),
        (0.8, 0.04, 1.25, 0.85, 1.0, None, 0.33),
        (0.9, 0.05, 1.15, 0.9, 1.0, None, 0.28),
        (0.85, 0.05, 1.2, 0.85, 1.0, None, 0.3),
    ]
    env, noise = breath_loop(rng, spec)
    return normalise(voice(env, noise, 1250.0))


def ragged(rng):
    """Fast and uneven: six breaths of different length and depth, with catches and flutter."""
    spec = [
        (0.55, 0.03, 0.8, 0.75, 1.0, (0.55, 0.45), 0.12),
        (0.7, 0.04, 0.65, 0.95, 0.85, None, 0.2),
        (0.5, 0.02, 0.9, 0.7, 1.0, None, 0.1),
        (0.65, 0.05, 0.75, 0.9, 0.95, (0.4, 0.5), 0.22),
        (0.45, 0.03, 0.7, 0.7, 0.9, None, 0.12),
        (0.7, 0.04, 0.95, 0.95, 1.0, (0.6, 0.4), 0.26),
    ]
    env, noise = breath_loop(rng, spec)
    return normalise(voice(env, noise, 1450.0, flutter_hz=9.0, flutter_depth=0.22))


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


NAMES = ("steady", "harder", "ragged")


def report(directory):
    for kind in NAMES:
        name = f"breath_{kind}.wav"
        samples = read(os.path.join(directory, name))
        centroid, high = measure(samples)
        peak = 20 * math.log10(max(abs(x) for x in samples))
        ok = "ok" if centroid < 900.0 and high < 0.02 else "OVER TARGET"
        print(f"{name}: {len(samples) / RATE:.1f} s, peak {peak:.1f} dBFS, centroid {centroid:.0f} Hz, above 2 kHz {high * 100:.4f}% ({ok})")


def main():
    args = sys.argv[1:]
    if len(args) == 2 and args[0] == "--measure":
        report(args[1])
        return
    if len(args) != 1:
        sys.exit("usage: synth_breath.py [--measure] OUT_DIR")
    out = args[0]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)
    for kind, make in zip(NAMES, (steady, harder, ragged)):
        write(os.path.join(out, f"breath_{kind}.wav"), make(rng))
    report(out)


if __name__ == "__main__":
    main()
