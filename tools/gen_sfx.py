#!/usr/bin/env python3
"""Generate the game's sound effects as small OGG files (no external Python deps).

Each effect is synthesised from simple oscillators + shaped noise (retro / 8-bit
flavour), written to a temporary WAV, then encoded to OGG with `ffmpeg` (Bevy plays
OGG out of the box via its default `vorbis` feature). Output: `assets/sounds/*.ogg`,
which `build.rs` bakes into the binary.

Run from anywhere:  python3 tools/gen_sfx.py
Deterministic (seeded), so re-running reproduces identical files.
"""

import math
import os
import random
import struct
import subprocess
import wave

SR = 44_100
OUT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "assets", "sounds"))
random.seed(1234)


def n(dur):
    return int(SR * dur)


def env(i, count, k):
    """Exponential decay envelope (k = how fast it dies away)."""
    return math.exp(-k * i / count)


def tone(freq, dur, vol=0.5, wave="sine", decay=5.0, f1=None):
    """A pitched oscillator; sweeps freq -> f1 over the duration when f1 is given."""
    count = n(dur)
    out = [0.0] * count
    phase = 0.0
    for i in range(count):
        f = freq if f1 is None else freq + (f1 - freq) * (i / count)
        phase += 2.0 * math.pi * f / SR
        if wave == "square":
            s = 1.0 if math.sin(phase) >= 0.0 else -1.0
        elif wave == "saw":
            s = 2.0 * ((phase / (2.0 * math.pi)) % 1.0) - 1.0
        else:
            s = math.sin(phase)
        out[i] = s * vol * env(i, count, decay)
    return out


def noise(dur, vol=0.5, decay=8.0, lp=0.0):
    """White noise, optionally softened by a one-pole low-pass (lp in 0..1)."""
    count = n(dur)
    out = [0.0] * count
    prev = 0.0
    for i in range(count):
        x = random.uniform(-1.0, 1.0)
        if lp > 0.0:
            prev += lp * (x - prev)
            x = prev
        out[i] = x * vol * env(i, count, decay)
    return out


def layer(*segments):
    """Mix several segments (summed, aligned at the start)."""
    length = max(len(s) for s in segments)
    out = [0.0] * length
    for seg in segments:
        for i, v in enumerate(seg):
            out[i] += v
    return out


def at(buf, seg, start):
    """Add `seg` into `buf` starting at `start` seconds (extending if needed)."""
    s = n(start)
    if s + len(seg) > len(buf):
        buf = buf + [0.0] * (s + len(seg) - len(buf))
    for i, v in enumerate(seg):
        buf[s + i] += v
    return buf


def finalize(buf, fade=0.004):
    """Short start/end fades to avoid clicks."""
    f = max(1, n(fade))
    for i in range(min(f, len(buf))):
        buf[i] *= i / f
        buf[-1 - i] *= i / f
    return buf


def write_ogg(name, buf):
    os.makedirs(OUT, exist_ok=True)
    wav = os.path.join(OUT, name + ".wav")
    ogg = os.path.join(OUT, name + ".ogg")
    with wave.open(wav, "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        frames = bytearray()
        for v in buf:
            v = max(-1.0, min(1.0, v))
            frames += struct.pack("<h", int(v * 32767))
        w.writeframes(frames)
    # `-bitexact` makes the encode reproducible: without it ffmpeg stamps every Ogg file with
    # a random stream serial number (in the page header), so re-running would rewrite all the
    # .ogg files with byte differences even though the audio is identical — noisy git diffs.
    subprocess.run(
        [
            "ffmpeg", "-y", "-loglevel", "error",
            "-fflags", "+bitexact", "-flags", "+bitexact",
            "-i", wav,
            "-c:a", "libvorbis", "-qscale:a", "5",
            "-fflags", "+bitexact",
            ogg,
        ],
        check=True,
    )
    os.remove(wav)
    print(f"  {name}.ogg  ({os.path.getsize(ogg)} bytes)")


# --- the effects ---------------------------------------------------------------

SOUNDS = {
    # A soft, quiet footfall: a dull low-passed noise tap + a faint low thump.
    "footstep": lambda: finalize(layer(
        noise(0.07, vol=0.22, decay=18.0, lp=0.25),
        tone(112.0, 0.06, vol=0.16, wave="sine", decay=20.0),
    )),
    # Classic rising blip.
    "jump": lambda: finalize(tone(220.0, 0.16, vol=0.38, wave="square", decay=4.5, f1=560.0)),
    # Airier, higher second jump.
    "double_jump": lambda: finalize(layer(
        tone(420.0, 0.15, vol=0.34, wave="sine", decay=4.0, f1=960.0),
        tone(840.0, 0.10, vol=0.12, wave="sine", decay=6.0, f1=1500.0),
    )),
    # A quick scrape (noise) plus a mid blip kicking off the wall.
    "wall_jump": lambda: finalize(layer(
        noise(0.05, vol=0.20, decay=22.0, lp=0.4),
        tone(300.0, 0.12, vol=0.30, wave="square", decay=5.0, f1=520.0),
    )),
    # A low thump as the feet hit the ground.
    "land": lambda: finalize(layer(
        tone(120.0, 0.12, vol=0.34, wave="sine", decay=7.0, f1=70.0),
        noise(0.06, vol=0.16, decay=20.0, lp=0.2),
    )),
    # A bright airy "swish".
    "slash": lambda: finalize(layer(
        noise(0.13, vol=0.42, decay=10.0),
        tone(900.0, 0.08, vol=0.10, wave="sine", decay=9.0, f1=1600.0),
    )),
    # The 3-hit finisher: a bigger whoosh + a metallic "shing" + low body.
    "slash_heavy": lambda: finalize(layer(
        noise(0.16, vol=0.46, decay=8.0),
        tone(1200.0, 0.22, vol=0.22, wave="sine", decay=5.0, f1=900.0),
        tone(1800.0, 0.22, vol=0.14, wave="sine", decay=5.0, f1=1300.0),
        tone(140.0, 0.12, vol=0.25, wave="square", decay=7.0, f1=90.0),
    )),
    # A punchy hit "thwack".
    "enemy_hit": lambda: finalize(layer(
        noise(0.08, vol=0.40, decay=16.0, lp=0.5),
        tone(160.0, 0.08, vol=0.32, wave="square", decay=12.0, f1=80.0),
    )),
    # Taking damage: a descending square tone with a little grit.
    "hurt": lambda: finalize(layer(
        tone(440.0, 0.28, vol=0.5, wave="square", decay=3.0, f1=150.0),
        noise(0.12, vol=0.22, decay=10.0),
    )),
    # Energy pickup: a pleasant two-note rising ding.
    "pickup": lambda: finalize(at(
        tone(660.0, 0.09, vol=0.30, wave="sine", decay=6.0),
        tone(990.0, 0.12, vol=0.30, wave="sine", decay=6.0),
        0.07,
    )),
    # Dash: a short airy whoosh — low-passed noise + a quick falling tone.
    "dash": lambda: finalize(layer(
        noise(0.14, vol=0.34, decay=12.0, lp=0.5),
        tone(700.0, 0.12, vol=0.18, wave="sine", decay=8.0, f1=300.0),
    )),
    # Save/rest jingle: a bright ascending C-major arpeggio (C-E-G-C) resolving high.
    "save": lambda: finalize(at(at(at(
        tone(523.25, 0.26, vol=0.26, wave="sine", decay=6.0),
        tone(659.25, 0.26, vol=0.26, wave="sine", decay=6.0), 0.09),
        tone(783.99, 0.26, vol=0.26, wave="sine", decay=6.0), 0.18),
        tone(1046.5, 0.42, vol=0.30, wave="sine", decay=3.6), 0.27)),
}


def main():
    print(f"generating {len(SOUNDS)} sounds -> {OUT}")
    for name, make in SOUNDS.items():
        write_ogg(name, make())


if __name__ == "__main__":
    main()
