#!/usr/bin/env python3
"""Synthesises the delver's pain grunts as mono 16-bit WAVs: three short grunts and one hard
one, in a low voice and a high one.

    python3 tools/synth_grunt.py /tmp/grunt
    for f in /tmp/grunt/*.wav; do oggenc -q 6 -o game/assets/audio/$(basename $f .wav).ogg $f; done

Standard library only. The output is the same on every run (a fixed seed).

Each grunt is a voiced sound made the way a throat makes one: a train of glottal pulses (a
Rosenberg pulse, with a little jitter and shimmer so it never sounds like a buzzer), its pitch
jumping up at the blow and falling away, sent through three formant band-passes for a vowel,
with breath noise mixed in. A hard attack (the breath forced out) and a fast decay make it a
grunt rather than a word:

    grunt_<voice>_1   a short "uh"
    grunt_<voice>_2   a sharper "ah", more breath
    grunt_<voice>_3   an "ugh" that closes and creaks at the end
    grunt_<voice>_hard  a longer, louder cry for a blow that takes a big share of the delver

The low voice's pitch starts near 125 Hz, the high voice's near 220 Hz with its formants a
sixth higher (the shorter vocal tract). The game picks the voice by the delver's sex and nudges
the playback speed by folk (`game/src/combat/vitals.rs`, `voice_for`).

Peaks are normalised to -3 dBFS.
"""

import math
import os
import random
import struct
import sys
import wave

RATE = 44100
SEED = 4471


def svf_bandpass(samples, centre, q):
    """A state-variable band-pass whose centre may follow the sample index."""
    low = band = 0.0
    out = []
    damp = 1.0 / q
    for i, x in enumerate(samples):
        c = centre(i) if callable(centre) else centre
        f = 2.0 * math.sin(math.pi * min(c, RATE / 6.0) / RATE)
        high = x - low - damp * band
        band += f * high
        low += f * band
        out.append(band)
    return out


def normalise(samples, dbfs):
    peak = max(abs(x) for x in samples) or 1.0
    gain = 10 ** (dbfs / 20.0) / peak
    return [x * gain for x in samples]


def glottal(rng, n, pitch, jitter, creak_from=None):
    """Glottal flow derivative pulses at `pitch(t)` Hz, t from 0 to 1 over `n` samples."""
    out = []
    phase = 0.0
    period_jitter = 0.0
    shimmer = 1.0
    for i in range(n):
        t = i / n
        f0 = pitch(t) * (1.0 + period_jitter)
        if creak_from is not None and t > creak_from:
            # Vocal fry: the pitch drops and the pulses come unevenly.
            f0 *= 0.55
        phase += f0 / RATE
        if phase >= 1.0:
            phase -= 1.0
            period_jitter = rng.gauss(0.0, jitter * (3.0 if creak_from is not None and t > creak_from else 1.0))
            shimmer = 1.0 + rng.gauss(0.0, 0.08)
        # Rosenberg pulse: opens over 60% of the period, closes over 25%, shut for the rest.
        if phase < 0.6:
            flow_slope = math.sin(math.pi * phase / 0.6) * math.pi / 0.6 * 0.5
        elif phase < 0.85:
            flow_slope = -math.sin(math.pi * (phase - 0.6) / 0.5) * math.pi / 0.25
        else:
            flow_slope = 0.0
        out.append(flow_slope * shimmer)
    return out


def grunt(rng, seconds, pitch, formants, breath, decay, creak_from=None, jitter=0.012):
    n = int(seconds * RATE)
    source = glottal(rng, n, pitch, jitter, creak_from)
    noise = [rng.uniform(-1.0, 1.0) for _ in range(n)]
    voiced = [0.0] * n
    for k, (centre, q, gain) in enumerate(formants):
        band = svf_bandpass([s + 0.15 * w for s, w in zip(source, noise)], centre, q)
        for i in range(n):
            voiced[i] += gain * band[i]
    aspiration = svf_bandpass(noise, lambda i: formants[1][0](i) if callable(formants[1][0]) else formants[1][0], 2.0)
    out = []
    for i in range(n):
        t = i / n
        attack = min(1.0, i / (0.012 * RATE))
        env = attack * math.exp(-max(0.0, t - 0.08) * decay)
        # The breath forced out at the blow, fading faster than the voice.
        puff = breath * math.exp(-t * 9.0)
        out.append(env * (voiced[i] + puff * aspiration[i] * 6.0))
    # A short fade so the tail never clicks.
    tail = int(0.02 * RATE)
    for i in range(tail):
        out[n - 1 - i] *= i / tail
    return normalise(out, -3.0)


def vowel(f1, f2, f3, scale, close_to=None):
    """Three formants (centre, q, gain), scaled for the voice; `close_to` glides F1 and F2 toward
    a closed vowel over the sound."""
    def glide(start, end, n_hint):
        if close_to is None:
            return start * scale
        return lambda i: scale * (start + (end - start) * min(1.0, i / n_hint))
    n_hint = int(0.3 * RATE)
    return [
        (glide(f1, close_to[0] if close_to else f1, n_hint), 6.0, 1.0),
        (glide(f2, close_to[1] if close_to else f2, n_hint), 9.0, 0.55),
        (f3 * scale, 12.0, 0.25),
    ]


def voice(rng, low):
    base = 125.0 if low else 220.0
    scale = 1.0 if low else 1.17
    jump = lambda top, end: (lambda t: base * (end + (top - end) * math.exp(-t * 7.0) + 0.25 * math.exp(-((t - 0.04) / 0.03) ** 2)))
    return {
        "1": grunt(rng, 0.24, jump(1.25, 0.82), vowel(640, 1190, 2390, scale), 0.6, 5.0),
        "2": grunt(rng, 0.28, jump(1.4, 0.9), vowel(760, 1250, 2500, scale), 1.0, 4.5),
        "3": grunt(rng, 0.36, jump(1.2, 0.75), vowel(600, 1100, 2400, scale, close_to=(380, 800)), 0.5, 4.5, creak_from=0.55),
        "hard": grunt(rng, 0.5, jump(1.6, 0.85), vowel(780, 1300, 2550, scale), 1.2, 3.2, creak_from=0.7, jitter=0.02),
    }


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(max(-1.0, min(1.0, x)) * 32767))) for x in samples))


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_grunt.py OUT_DIR")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)
    for name, low in [("low", True), ("high", False)]:
        for take, samples in voice(rng, low).items():
            write(os.path.join(out, f"grunt_{name}_{take}.wav"), samples)


if __name__ == "__main__":
    main()
