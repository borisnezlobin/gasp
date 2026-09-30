"""Synthesizes the reel's soundtrack from a cue list.

    python synth.py <cues.json> <out.wav>

Everything is generated here: a dry percussive track at the cue list's
tempo (kick, closed hats, clicks and a sub pulse), key clicks at the times
keys were pressed in the captures, the inhale, and the whoosh and splash of
the breach. Every random source is seeded, so a render is repeatable.

The cue list (written by compose/timeline.js) looks like:

    {"duration": 42.0, "bpm": 124,
     "music": [[start, end, "full" | "light" | "pulse"], ...],
     "inhales": [[time, length], ...],
     "keys": [[time, strength], ...],
     "ticks": [time, ...],
     "whoosh": [[time, length], ...],
     "splash": [time, ...],
     "cuts": [time, ...],
     "hits": [time, ...]}
"""
import json
import sys
import wave

import numpy as np
from scipy import signal

RATE = 48000


def seconds(n: float) -> int:
    return int(round(n * RATE))


def rng(seed: int) -> np.random.Generator:
    return np.random.default_rng(seed)


def place(bus: np.ndarray, sound: np.ndarray, at: float, gain: float = 1.0, pan: float = 0.0) -> None:
    """Adds a mono `sound` into the stereo `bus` at `at` seconds, panned -1..1."""
    start = seconds(at)
    if start >= bus.shape[1] or start + len(sound) <= 0:
        return
    if start < 0:
        sound, start = sound[-start:], 0
    end = min(bus.shape[1], start + len(sound))
    left = np.cos((pan + 1) * np.pi / 4) * np.sqrt(2)
    right = np.sin((pan + 1) * np.pi / 4) * np.sqrt(2)
    part = sound[: end - start] * gain
    bus[0, start:end] += part * min(left, 1.0)
    bus[1, start:end] += part * min(right, 1.0)


def band(noise: np.ndarray, low: float, high: float, order: int = 2) -> np.ndarray:
    sos = signal.butter(order, [low, high], btype="band", fs=RATE, output="sos")
    return signal.sosfilt(sos, noise)


def high(noise: np.ndarray, cutoff: float, order: int = 2) -> np.ndarray:
    sos = signal.butter(order, cutoff, btype="high", fs=RATE, output="sos")
    return signal.sosfilt(sos, noise)


def low(noise: np.ndarray, cutoff: float, order: int = 2) -> np.ndarray:
    sos = signal.butter(order, cutoff, btype="low", fs=RATE, output="sos")
    return signal.sosfilt(sos, noise)


def resonator(noise: np.ndarray, centre: np.ndarray, width: np.ndarray) -> np.ndarray:
    """A two-pole resonator whose centre and bandwidth (Hz) move per sample."""
    out = np.zeros_like(noise)
    centre = np.broadcast_to(np.asarray(centre, float), noise.shape)
    width = np.broadcast_to(np.asarray(width, float), noise.shape)
    y1 = y2 = 0.0
    r = np.exp(-np.pi * width / RATE)
    theta = 2 * np.pi * centre / RATE
    a1 = 2 * r * np.cos(theta)
    a2 = -r * r
    gain = (1 - r * r) * 0.5
    for i, x in enumerate(noise):
        y = gain[i] * x + a1[i] * y1 + a2[i] * y2
        out[i] = y
        y2, y1 = y1, y
    return out


# ---- Drums -----------------------------------------------------------------

def kick(seed: int = 0) -> np.ndarray:
    n = seconds(0.42)
    t = np.arange(n) / RATE
    pitch = 44 + 110 * np.exp(-t / 0.03)
    phase = 2 * np.pi * np.cumsum(pitch) / RATE
    body = np.sin(phase) * np.exp(-t / 0.16)
    click = high(rng(seed).normal(0, 1, n), 2500) * np.exp(-t / 0.004) * 0.35
    out = body + click
    return np.tanh(out * 1.6) / np.tanh(1.6)


def hat(seed: int, open_: bool = False) -> np.ndarray:
    n = seconds(0.2 if open_ else 0.07)
    t = np.arange(n) / RATE
    noise = rng(seed).normal(0, 1, n)
    metal = sum(np.sign(np.sin(2 * np.pi * f * t)) for f in (3140, 4270, 5510, 6830, 8020))
    tone = high(noise * 0.6 + metal * 0.25, 7000, 4)
    return tone * np.exp(-t / (0.06 if open_ else 0.014)) * 0.5


def tick(seed: int) -> np.ndarray:
    """A dry, woody click on the off-beats."""
    n = seconds(0.03)
    t = np.arange(n) / RATE
    noise = rng(seed).normal(0, 1, n)
    return band(noise, 1800, 4200, 2) * np.exp(-t / 0.003) * 1.4


def sub(freq: float, length: float) -> np.ndarray:
    n = seconds(length)
    t = np.arange(n) / RATE
    env = np.minimum(1, t / 0.01) * np.exp(-t / (length * 0.6))
    wave_ = np.sin(2 * np.pi * freq * t) + 0.18 * np.sin(4 * np.pi * freq * t)
    return wave_ * env


def rim(seed: int) -> np.ndarray:
    n = seconds(0.09)
    t = np.arange(n) / RATE
    noise = rng(seed).normal(0, 1, n)
    body = np.sin(2 * np.pi * 820 * t) * np.exp(-t / 0.012)
    snap = band(noise, 1500, 6000) * np.exp(-t / 0.02)
    return (body * 0.6 + snap * 0.9) * 0.8


# ---- Foley -------------------------------------------------------------------

def key_click(seed: int, strength: float = 1.0) -> np.ndarray:
    """A low-profile mechanical key: a short bright click and a soft thock."""
    r = rng(seed)
    n = seconds(0.07)
    t = np.arange(n) / RATE
    noise = r.normal(0, 1, n)
    centre = r.uniform(2600, 4200)
    click = band(noise, centre * 0.7, centre * 1.3) * np.exp(-t / 0.0025)
    thock_f = r.uniform(190, 260)
    thock = np.sin(2 * np.pi * thock_f * t) * np.exp(-t / 0.018) * 0.5
    bottom = np.zeros(n)
    delay = seconds(r.uniform(0.012, 0.02))
    tail = band(noise, 900, 2400)[: n - delay] * np.exp(-t[: n - delay] / 0.006) * 0.5
    bottom[delay:] = tail
    return (click * 1.1 + thock + bottom) * strength * 0.8


def inhale(length: float, seed: int = 7) -> np.ndarray:
    """A sharp breath in through an open mouth: turbulent noise shaped by a
    vocal tract whose formants rise as the throat narrows, swelling fast and
    stopping on a catch."""
    n = seconds(length)
    t = np.arange(n) / RATE
    u = t / length
    r = rng(seed)
    noise = r.normal(0, 1, n)
    # Air through the lips and teeth, the brightest part.
    hiss = band(noise, 2400, 10000, 2)
    # Formants of a whispered, inhaled "huh" moving towards "ih".
    formants = [
        (700 + 250 * u, 260 + 60 * u, 1.0),
        (1250 + 700 * u ** 1.3, 300 + 80 * u, 0.9),
        (2500 + 500 * u, 380, 0.55),
        (3500 + 400 * u, 500, 0.35),
    ]
    voice = np.zeros(n)
    for centre, width, gain in formants:
        voice += resonator(noise, np.asarray(centre, float), np.asarray(width, float)) * gain
    voice /= np.max(np.abs(voice)) + 1e-9
    hiss /= np.max(np.abs(hiss)) + 1e-9
    # Turbulence wobbles the level a little.
    flutter = 1 + 0.12 * low(r.normal(0, 1, n), 18) * 12
    # A quick swell, rising to the end, then a catch in the last 25 ms.
    attack = np.clip(u / 0.16, 0, 1) ** 1.6
    rise = 0.55 + 0.45 * u ** 0.7
    catch = np.clip((1 - u) / (0.025 / length), 0, 1) ** 0.5
    env = attack * rise * catch * flutter
    breath = (voice * 0.6 + hiss * (0.45 + 0.4 * u)) * env
    # The glottis closing: a tiny low knock right at the end.
    knock_n = seconds(0.03)
    kt = np.arange(knock_n) / RATE
    knock = np.sin(2 * np.pi * 140 * kt) * np.exp(-kt / 0.006) * 0.25
    out = np.concatenate([breath, knock])
    return out / (np.max(np.abs(out)) + 1e-9)


def whoosh(length: float, seed: int = 11) -> np.ndarray:
    n = seconds(length)
    t = np.arange(n) / RATE
    u = t / length
    noise = rng(seed).normal(0, 1, n)
    centre = 250 * (14 ** (u ** 1.4))
    swept = resonator(noise, centre, centre * 0.9)
    env = np.sin(np.pi * np.clip(u, 0, 1)) ** 2 * (0.3 + 0.7 * u)
    out = swept * env
    return out / (np.max(np.abs(out)) + 1e-9)


def splash(seed: int = 13) -> np.ndarray:
    """Water parting and falling back: a noisy burst with a long wash, and
    droplets scattered through the tail."""
    length = 1.8
    n = seconds(length)
    t = np.arange(n) / RATE
    r = rng(seed)
    noise = r.normal(0, 1, n)
    burst = low(noise, 2200, 2) * np.exp(-t / 0.09)
    wash = band(noise, 400, 5000, 2) * np.exp(-t / 0.55) * np.minimum(1, t / 0.03)
    drops = np.zeros(n)
    for _ in range(70):
        at = r.uniform(0.05, 1.4) ** 1.4
        start = seconds(at)
        dn = seconds(0.03)
        if start + dn >= n:
            continue
        dt = np.arange(dn) / RATE
        f0 = r.uniform(900, 2600)
        chirp = np.sin(2 * np.pi * (f0 * dt + 0.5 * f0 * 4 * dt ** 2)) * np.exp(-dt / 0.006)
        drops[start:start + dn] += chirp * r.uniform(0.05, 0.25) * np.exp(-at / 0.8)
    thump = np.sin(2 * np.pi * 60 * t) * np.exp(-t / 0.12) * 0.6
    out = burst * 0.9 + wash * 0.7 + drops + thump
    return out / (np.max(np.abs(out)) + 1e-9)


def cut_accent(seed: int) -> np.ndarray:
    """A dry clap with a little body, on the picture's cuts."""
    n = seconds(0.18)
    t = np.arange(n) / RATE
    noise = rng(seed).normal(0, 1, n)
    bursts = np.zeros(n)
    for offset in (0.0, 0.009, 0.017):
        start = seconds(offset)
        bursts[start:] += np.exp(-t[: n - start] / 0.006)
    clap = band(noise, 900, 3500) * (bursts + 0.4 * np.exp(-t / 0.05))
    body = np.sin(2 * np.pi * 180 * t) * np.exp(-t / 0.03) * 0.5
    return (clap * 0.7 + body) * 0.8


def boom(seed: int = 17) -> np.ndarray:
    """A low hit that marks the logo landing."""
    n = seconds(1.6)
    t = np.arange(n) / RATE
    pitch = 38 + 40 * np.exp(-t / 0.08)
    body = np.sin(2 * np.pi * np.cumsum(pitch) / RATE) * np.exp(-t / 0.5)
    air = low(rng(seed).normal(0, 1, n), 800) * np.exp(-t / 0.05) * 0.3
    return np.tanh((body + air) * 1.3)


# ---- The track ----------------------------------------------------------------

def music(bus: np.ndarray, bpm: float, sections: list) -> None:
    beat = 60.0 / bpm
    step = beat / 4
    kick_s = kick()
    hats = [hat(100 + i) for i in range(8)]
    open_hat = hat(99, open_=True)
    ticks = [tick(200 + i) for i in range(8)]
    rims = [rim(300 + i) for i in range(4)]
    # A two-note sub figure over two bars, in A.
    notes = [55.0, 55.0, 55.0, 65.41, 55.0, 55.0, 49.0, 55.0]
    for start, end, mode in sections:
        first = int(np.ceil(start / step - 1e-6))
        last = int(np.floor(end / step - 1e-6))
        for index in range(first, last + 1):
            at = index * step
            if at >= end - 1e-6:
                break
            sixteenth = index % 4
            beat_index = index // 4
            bar_pos = beat_index % 4
            if mode in ("full", "light", "drive") and sixteenth == 0:
                place(bus, kick_s, at, 0.95)
            if mode == "drive" and sixteenth == 2 and bar_pos == 3:
                place(bus, kick_s, at, 0.6)
            if mode in ("full", "drive"):
                gain = 0.32 if sixteenth == 2 else 0.14
                place(bus, hats[index % 8], at, gain, pan=0.35)
                if sixteenth == 2 and bar_pos == 3 and beat_index % 8 == 7:
                    place(bus, open_hat, at, 0.25, pan=0.35)
            if mode == "light" and sixteenth == 2:
                place(bus, hats[index % 8], at, 0.22, pan=0.35)
            if mode in ("full", "drive") and sixteenth == 0 and bar_pos in (1, 3):
                place(bus, rims[beat_index % 4], at, 0.35, pan=-0.2)
            if mode in ("full", "drive", "pulse") and sixteenth in (1, 3) and index % 3 != 0:
                place(bus, ticks[index % 8], at, 0.16, pan=-0.45)
            if mode in ("full", "drive", "pulse") and sixteenth == 2:
                freq = notes[(beat_index // 1) % len(notes)]
                place(bus, sub(freq, step * 1.8), at, 0.5)


def render(cues: dict) -> np.ndarray:
    duration = cues["duration"]
    bus = np.zeros((2, seconds(duration) + RATE))
    music(bus, cues["bpm"], cues["music"])
    for index, (at, strength) in enumerate(cues.get("keys", [])):
        place(bus, key_click(1000 + index, strength), at, 0.55, pan=0.1)
    for index, at in enumerate(cues.get("ticks", [])):
        place(bus, tick(5000 + index), at, 0.35)
    for index, (at, length) in enumerate(cues.get("inhales", [])):
        place(bus, inhale(length, INHALE_SEED), at, 0.9)
    for index, (at, length) in enumerate(cues.get("whoosh", [])):
        place(bus, whoosh(length, 11 + index), at, 0.5)
    for index, at in enumerate(cues.get("splash", [])):
        place(bus, splash(13 + index), at, 0.75)
    for index, at in enumerate(cues.get("cuts", [])):
        place(bus, cut_accent(600 + index), at, 0.45)
    for index, at in enumerate(cues.get("hits", [])):
        place(bus, boom(17 + index), at, 0.8)
    bus = bus[:, : seconds(duration)]
    # Fade the last few milliseconds so the end is silent, not clipped.
    fade = seconds(0.02)
    bus[:, -fade:] *= np.linspace(1, 0, fade)
    peak = np.max(np.abs(bus)) + 1e-9
    return bus / peak * 0.7


def write(path: str, bus: np.ndarray) -> None:
    data = (np.clip(bus.T, -1, 1) * 32767).astype("<i2")
    with wave.open(path, "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(data.tobytes())


INHALE_LENGTH = 0.72
INHALE_SEED = 7


def peaks(path: str, bins: int = 260) -> None:
    """The inhale's envelope, for the waveform the picture draws."""
    sound = inhale(INHALE_LENGTH, INHALE_SEED)
    chunks = np.array_split(np.abs(sound), bins)
    values = np.array([chunk.max() for chunk in chunks])
    values = np.convolve(values, np.ones(3) / 3, mode="same")
    values /= values.max()
    with open(path, "w") as handle:
        json.dump({"length": INHALE_LENGTH, "peaks": [round(float(v), 4) for v in values]}, handle)


if __name__ == "__main__":
    if sys.argv[1] == "peaks":
        peaks(sys.argv[2])
        sys.exit()
    with open(sys.argv[1]) as handle:
        cue_list = json.load(handle)
    write(sys.argv[2], render(cue_list))
