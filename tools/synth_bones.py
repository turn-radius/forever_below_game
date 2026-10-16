#!/usr/bin/env python3
"""Synthesises a skeleton's sound effects as mono 16-bit WAVs: the click of a bony foot on stone
(three variants), the rattle of bones standing about, the clack of a blow on bone, the clang of
a blow on a shield and the clatter of a skeleton falling in a heap.

    python3 tools/synth_bones.py /tmp/bones
    oggenc -q 6 -o game/assets/audio/bone_step_1.ogg /tmp/bones/bone_step_1.wav
    (and the same for bone_step_2, bone_step_3, bone_rattle, bone_hit, bone_block, bone_collapse)

Standard library only. The output is the same on every run (a fixed seed).

Technique, as tools/synth_combat.py: everyday sounds built from oscillators, noise and
filters, the method of Andy Farnell's "Designing Sound" (MIT Press 2010). Every sound here is
made of clicks: a dry bone knocking on stone or on bone is a short noise burst that excites a
few resonances (a hollow bone rings at a handful of inharmonic partials) which die away
quickly (Farnell's treatment of impacts as a burst exciting resonant modes). A heap of falling
bones is many clicks, each with its own partials, spaced closer and closer as they settle
(the bouncing of a dropped object, bounce intervals shrinking geometrically). The partials,
decays and timings are our own.
"""

import math
import os
import random
import struct
import sys
import wave

RATE = 44100
SEED = 290


def click(out, start, amp, partials, decay, rng, noise=0.5):
    """Adds one knock at `start` seconds: a noise transient and sines at `partials` Hz that die
    away with time constant `decay` seconds."""
    s0 = int(start * RATE)
    n = int(min(decay * 7.0, 0.6) * RATE)
    phases = [rng.uniform(0.0, 2.0 * math.pi) for _ in partials]
    for i in range(n):
        if s0 + i >= len(out):
            break
        t = i / RATE
        env = math.exp(-t / decay)
        tone = sum(math.sin(2.0 * math.pi * f * t + p) / (1.0 + 0.6 * k) for k, (f, p) in enumerate(zip(partials, phases)))
        burst = noise * rng.uniform(-1.0, 1.0) * math.exp(-t / 0.0012)
        out[s0 + i] += amp * (env * tone * 0.5 + burst)


def bone_partials(rng, base):
    """A hollow bone's ringing: a few inharmonic partials about `base` Hz."""
    return [base * rng.uniform(0.95, 1.05), base * rng.uniform(1.9, 2.3), base * rng.uniform(3.2, 3.9), base * rng.uniform(5.0, 5.8)]


def normalise(samples, dbfs=-2.0):
    peak = max(abs(x) for x in samples) or 1.0
    scale = 10 ** (dbfs / 20.0) / peak
    return [x * scale for x in samples]


def fade(samples, ms=4.0):
    n = int(ms / 1000.0 * RATE)
    for i in range(min(n, len(samples))):
        samples[-1 - i] *= i / n
    return samples


def step(rng, k):
    """A bony foot on stone: a click and a smaller one just after, as heel and toe land."""
    out = [0.0] * int(0.22 * RATE)
    base = 1100.0 + 350.0 * k
    click(out, 0.0, 1.0, bone_partials(rng, base), 0.012, rng, 0.7)
    click(out, 0.045 + 0.01 * k, 0.55, bone_partials(rng, base * 1.35), 0.009, rng, 0.5)
    return fade(normalise(out, -4.0))


def rattle(rng):
    """Bones shifting against one another: a loose run of small clicks, swelling and dying."""
    n = int(0.9 * RATE)
    out = [0.0] * n
    t = 0.02
    while t < 0.8:
        amp = math.sin(math.pi * t / 0.8) ** 1.5 * rng.uniform(0.3, 1.0)
        click(out, t, amp, bone_partials(rng, rng.uniform(900.0, 2400.0)), rng.uniform(0.006, 0.014), rng, 0.4)
        t += rng.uniform(0.02, 0.09)
    return fade(normalise(out, -6.0))


def hit(rng):
    """A blow on bone: a sharp clack with a duller knock under it."""
    out = [0.0] * int(0.35 * RATE)
    click(out, 0.0, 1.0, bone_partials(rng, 1500.0), 0.02, rng, 0.9)
    click(out, 0.0, 0.7, bone_partials(rng, 520.0), 0.035, rng, 0.2)
    click(out, 0.06, 0.35, bone_partials(rng, 2100.0), 0.01, rng, 0.3)
    return fade(normalise(out, -2.0))


def block(rng):
    """A blow on a shield: a clang, inharmonic partials ringing a while."""
    out = [0.0] * int(0.5 * RATE)
    click(out, 0.0, 1.0, [430.0, 1130.0, 1870.0, 2650.0, 3720.0], 0.11, rng, 1.0)
    click(out, 0.0, 0.5, [260.0, 640.0], 0.05, rng, 0.2)
    return fade(normalise(out, -2.0), 20.0)


def collapse(rng):
    """A skeleton falling apart: a first heavy clatter, then bones bouncing and settling,
    clicks coming closer together and fainter."""
    out = [0.0] * int(1.6 * RATE)
    click(out, 0.0, 1.0, [120.0, 310.0], 0.05, rng, 0.8)
    t, gap, amp = 0.02, 0.07, 1.0
    while t < 1.4:
        click(out, t, amp * rng.uniform(0.5, 1.0), bone_partials(rng, rng.uniform(700.0, 2600.0)), rng.uniform(0.007, 0.02), rng, 0.5)
        t += gap * rng.uniform(0.5, 1.4)
        gap *= 0.97
        amp *= 0.965
    return fade(normalise(out, -2.0), 30.0)


def write(path, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(b"".join(struct.pack("<h", int(round(max(-1.0, min(1.0, x)) * 32767))) for x in samples))


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: synth_bones.py OUT_DIR")
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    rng = random.Random(SEED)
    sounds = [(f"bone_step_{k + 1}", lambda r, k=k: step(r, k)) for k in range(3)]
    sounds += [("bone_rattle", rattle), ("bone_hit", hit), ("bone_block", block), ("bone_collapse", collapse)]
    for name, make in sounds:
        write(os.path.join(out, name + ".wav"), make(rng))


if __name__ == "__main__":
    main()
