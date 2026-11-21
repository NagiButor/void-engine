#!/usr/bin/env python3
"""Synthesize realistic MW2019-inspired FPS pistol, reload, casing, and surface bullet impact WAVs."""

from __future__ import annotations

import wave
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "assets" / "audio" / "sfx"
SR = 48000


def write_wav(path: Path, samples: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    peak = float(np.max(np.abs(samples))) if samples.size else 0.0
    if peak > 1e-6:
        samples = samples * min(0.94 / peak, 1.0)
    pcm = np.clip(samples * 32767.0, -32768.0, 32767.0).astype("<i2")
    with wave.open(str(path), "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(SR)
        wf.writeframes(pcm.tobytes())
    print(f"  wrote {path.relative_to(ROOT)} ({len(samples) / SR:.2f}s)")


def biquad_lowpass(x: np.ndarray, cutoff: float, q: float = 0.707) -> np.ndarray:
    w0 = 2.0 * np.pi * np.clip(cutoff, 20.0, SR * 0.45) / SR
    alpha = np.sin(w0) / (2.0 * q)
    cw = np.cos(w0)
    b0 = (1.0 - cw) * 0.5
    b1 = 1.0 - cw
    b2 = (1.0 - cw) * 0.5
    a0 = 1.0 + alpha
    a1 = -2.0 * cw
    a2 = 1.0 - alpha
    b0, b1, b2, a1, a2 = b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0
    y = np.zeros_like(x, dtype=np.float64)
    x1 = x2 = y1 = y2 = 0.0
    for i in range(len(x)):
        xi = float(x[i])
        yi = b0 * xi + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1 = x1, xi
        y2, y1 = y1, yi
        y[i] = yi
    return y.astype(np.float32)


def biquad_highpass(x: np.ndarray, cutoff: float, q: float = 0.707) -> np.ndarray:
    w0 = 2.0 * np.pi * np.clip(cutoff, 20.0, SR * 0.45) / SR
    alpha = np.sin(w0) / (2.0 * q)
    cw = np.cos(w0)
    b0 = (1.0 + cw) * 0.5
    b1 = -(1.0 + cw)
    b2 = (1.0 + cw) * 0.5
    a0 = 1.0 + alpha
    a1 = -2.0 * cw
    a2 = 1.0 - alpha
    b0, b1, b2, a1, a2 = b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0
    y = np.zeros_like(x, dtype=np.float64)
    x1 = x2 = y1 = y2 = 0.0
    for i in range(len(x)):
        xi = float(x[i])
        yi = b0 * xi + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1 = x1, xi
        y2, y1 = y1, yi
        y[i] = yi
    return y.astype(np.float32)


def one_pole_lp(x: np.ndarray, cutoff: float) -> np.ndarray:
    alpha = float(np.exp(-2.0 * np.pi * cutoff / SR))
    y = np.empty_like(x, dtype=np.float32)
    acc = 0.0
    a1 = 1.0 - alpha
    for i in range(len(x)):
        acc = acc * alpha + float(x[i]) * a1
        y[i] = acc
    return y


def add_at(dst: np.ndarray, src: np.ndarray, t_sec: float, gain: float = 1.0) -> None:
    idx = int(t_sec * SR)
    if idx >= len(dst):
        return
    n = min(len(src), len(dst) - idx)
    if n > 0:
        dst[idx : idx + n] += src[:n] * gain


def metallic_click(rng: np.random.Generator, freqs: list[float], dur: float = 0.06, decay: float = 95.0) -> np.ndarray:
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    sig = rng.standard_normal(n).astype(np.float32) * np.exp(-t * decay * 1.4) * 0.35
    for i, f in enumerate(freqs):
        sig += np.sin(2.0 * np.pi * f * t) * np.exp(-t * decay * (1.0 + 0.25 * i)) * (0.45 / (i + 1))
    return np.tanh(sig * 1.8).astype(np.float32)


def gunshot_core(rng: np.random.Generator, dur: float, pitch_mul: float) -> np.ndarray:
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    # 1) Initial supersonic muzzle shockwave transient (0-5ms)
    crack = rng.standard_normal(n).astype(np.float32) * np.exp(-t * 240.0)
    crack = biquad_highpass(crack, 900.0) * 1.4
    # 2) Propellant bark / mid punch (800-1700Hz sweep)
    f_bark = (1450.0 * pitch_mul) * np.exp(-t * 95.0) + 170.0 * pitch_mul
    phase_b = np.cumsum(2.0 * np.pi * f_bark / SR)
    bark = np.sin(phase_b).astype(np.float32) * np.exp(-t * 62.0) * 0.85
    # 3) Chest thump sub-punch (180Hz -> 52Hz)
    f_sub = (210.0 * pitch_mul) * np.exp(-t * 48.0) + 48.0
    phase_s = np.cumsum(2.0 * np.pi * f_sub / SR)
    sub = np.sin(phase_s).astype(np.float32) * np.exp(-t * 28.0) * 0.95
    # 4) Broadband muzzle blast noise
    blast = rng.standard_normal(n).astype(np.float32) * np.exp(-t * 42.0)
    blast = one_pole_lp(blast, 5800.0) * 0.90
    # 5) Mechanical slide blowback & return clack (subtle tactical signature)
    core = crack + bark + sub + blast
    slide_back = metallic_click(rng, [3200.0 * pitch_mul, 5400.0], 0.04, 140.0)
    add_at(core, slide_back, 0.016, 0.24)
    slide_fwd = metallic_click(rng, [2600.0 * pitch_mul, 4700.0], 0.05, 115.0)
    add_at(core, slide_fwd, 0.058, 0.16)
    return np.tanh(core * 1.55).astype(np.float32)


def make_indoor_shot(seed: int, pitch_mul: float) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 0.65
    core = gunshot_core(rng, dur, pitch_mul)
    out = core.copy()
    # Tight early wall/ceiling reflections (small concrete room)
    taps = [
        (0.0045, 0.48, 4200.0),
        (0.0082, 0.40, 3600.0),
        (0.0135, 0.34, 3100.0),
        (0.0195, 0.27, 2600.0),
        (0.0270, 0.22, 2200.0),
        (0.0360, 0.17, 1900.0),
    ]
    for delay, gain, lp in taps:
        add_at(out, one_pole_lp(core, lp), delay, gain)

    # Dense small-room box tail (compressed low-mid boom + concrete flutter)
    n = len(out)
    t = np.arange(n, dtype=np.float32) / SR
    room_noise = rng.standard_normal(n).astype(np.float32)
    # Add short metallic/concrete slap density
    room_tail = one_pole_lp(room_noise, 2400.0) * np.exp(-t * 10.5) * (1.0 - np.exp(-t * 160.0))
    # Subtle 125 Hz room mode resonance
    room_mode = np.sin(2.0 * np.pi * 128.0 * t) * np.exp(-t * 14.0) * 0.22
    out += room_tail * 0.48 + room_mode
    out = np.tanh(out * 1.25).astype(np.float32)
    # Smooth fade-out at tail end
    fade = np.minimum(1.0, (dur - t) * 18.0)
    return (out * fade).astype(np.float32)


def make_outdoor_shot(seed: int, pitch_mul: float) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 1.45
    core = gunshot_core(rng, dur, pitch_mul)
    # Open-air shot has slightly crisper snap & tighter low-mid (no room wall boost)
    snap = biquad_highpass(core, 220.0) * 0.92 + one_pole_lp(core, 220.0) * 0.62
    out = snap.copy()

    # Wide outdoor reflection slaps + air-absorbed rolling tail (MW2019 exterior style)
    slaps = [
        (0.042, 0.34, 4800.0),
        (0.088, 0.25, 3400.0),
        (0.145, 0.19, 2500.0),
        (0.220, 0.14, 1800.0),
        (0.315, 0.10, 1300.0),
        (0.440, 0.06, 950.0),
    ]
    for delay, gain, lp in slaps:
        add_at(out, one_pole_lp(snap, lp), delay, gain)

    n = len(out)
    t = np.arange(n, dtype=np.float32) / SR
    far_tail = rng.standard_normal(n).astype(np.float32)
    far_tail = one_pole_lp(far_tail, 1350.0) * np.exp(-t * 3.4) * (1.0 - np.exp(-t * 28.0))
    out += far_tail * 0.34
    out = np.tanh(out * 1.15).astype(np.float32)
    fade = np.minimum(1.0, (dur - t) * 8.0)
    return (out * fade).astype(np.float32)


def make_outdoor_tail(seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 1.55
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    noise = rng.standard_normal(n).astype(np.float32)
    low_roll = one_pole_lp(noise, 900.0) * np.exp(-t * 2.8) * (1.0 - np.exp(-t * 14.0))
    mid_echo = one_pole_lp(noise, 2400.0) * np.exp(-t * 5.2) * (1.0 - np.exp(-t * 45.0))
    sig = low_roll * 0.65 + mid_echo * 0.35
    fade = np.minimum(1.0, (dur - t) * 6.0)
    return (sig * fade).astype(np.float32)


def make_dry_fire() -> np.ndarray:
    rng = np.random.default_rng(901)
    c1 = metallic_click(rng, [2400.0, 4900.0, 7600.0], 0.055, 130.0)
    return c1


def make_reload_easy() -> np.ndarray:
    rng = np.random.default_rng(902)
    dur = 1.32
    out = np.zeros(int(dur * SR), dtype=np.float32)
    # Mag release click + slide-out friction
    add_at(out, metallic_click(rng, [2900.0, 5100.0], 0.05, 120.0), 0.14, 0.7)
    n_slide = int(0.14 * SR)
    t = np.arange(n_slide, dtype=np.float32) / SR
    mag_out = one_pole_lp(rng.standard_normal(n_slide).astype(np.float32), 2200.0) * np.sin(np.pi * t / 0.14)
    add_at(out, mag_out, 0.18, 0.25)
    # Mag insert + lock tap
    add_at(out, mag_out, 0.64, 0.28)
    add_at(out, metallic_click(rng, [1400.0, 2900.0, 4600.0], 0.08, 75.0), 0.78, 0.95)
    add_at(out, metallic_click(rng, [900.0, 2100.0], 0.07, 90.0), 0.96, 0.65)
    return np.tanh(out * 1.2).astype(np.float32)


def make_reload_full() -> np.ndarray:
    rng = np.random.default_rng(903)
    dur = 1.66
    out = np.zeros(int(dur * SR), dtype=np.float32)
    # Mag release + drop
    add_at(out, metallic_click(rng, [2800.0, 5200.0], 0.05, 125.0), 0.13, 0.75)
    # Empty mag insert
    add_at(out, metallic_click(rng, [1300.0, 2700.0, 4400.0], 0.08, 78.0), 0.76, 0.95)
    # Crisp slide release / rack clack!
    add_at(out, metallic_click(rng, [2200.0, 3900.0, 6100.0], 0.07, 95.0), 1.15, 0.85)
    add_at(out, metallic_click(rng, [1800.0, 3400.0, 5500.0], 0.09, 70.0), 1.24, 1.05)
    return np.tanh(out * 1.25).astype(np.float32)


def make_shell_clink() -> np.ndarray:
    rng = np.random.default_rng(904)
    dur = 0.24
    out = np.zeros(int(dur * SR), dtype=np.float32)
    b1 = metallic_click(rng, [6150.0, 9650.0, 12800.0], 0.08, 95.0)
    b2 = metallic_click(rng, [6220.0, 9750.0], 0.06, 120.0)
    b3 = metallic_click(rng, [6180.0, 9600.0], 0.05, 145.0)
    add_at(out, b1, 0.0, 0.9)
    add_at(out, b2, 0.055, 0.52)
    add_at(out, b3, 0.098, 0.28)
    return out


def make_bullet_concrete(seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 0.36
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    crack = biquad_highpass(rng.standard_normal(n).astype(np.float32), 700.0) * np.exp(-t * 95.0)
    thud = np.sin(2.0 * np.pi * 190.0 * np.exp(-t * 25.0) * t) * np.exp(-t * 48.0) * 0.55
    grit = biquad_highpass(rng.standard_normal(n).astype(np.float32), 1800.0) * np.exp(-t * 16.0) * 0.22
    # Tiny debris pebbles
    for d in (0.045, 0.095, 0.15):
        add_at(grit, metallic_click(rng, [2400.0, 4100.0], 0.025, 160.0), d, 0.25)
    return np.tanh((crack + thud + grit) * 1.4).astype(np.float32)


def make_bullet_metal(seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 0.48
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    snap = rng.standard_normal(n).astype(np.float32) * np.exp(-t * 130.0) * 0.7
    ring = (
        np.sin(2.0 * np.pi * 1640.0 * t) * np.exp(-t * 11.0) * 0.45
        + np.sin(2.0 * np.pi * 2780.0 * t) * np.exp(-t * 15.0) * 0.35
        + np.sin(2.0 * np.pi * 4320.0 * t) * np.exp(-t * 22.0) * 0.25
    )
    return np.tanh((snap + ring) * 1.35).astype(np.float32)


def make_bullet_ricochet(seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 0.38
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    # FM/chirp zing
    f = 4600.0 * np.exp(-t * 7.5) + 950.0
    ph = np.cumsum(2.0 * np.pi * f / SR)
    zing = np.sin(ph).astype(np.float32) * np.exp(-t * 12.0) * (1.0 - np.exp(-t * 250.0)) * 0.65
    ping = metallic_click(rng, [3400.0, 5900.0], 0.05, 100.0) * 0.6
    out = zing.copy()
    add_at(out, ping, 0.0, 0.8)
    return np.tanh(out * 1.3).astype(np.float32)


def make_bullet_wood(seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    dur = 0.28
    n = int(dur * SR)
    t = np.arange(n, dtype=np.float32) / SR
    splinter = biquad_lowpass(rng.standard_normal(n).astype(np.float32), 3400.0) * np.exp(-t * 75.0)
    knock = np.sin(2.0 * np.pi * 340.0 * np.exp(-t * 18.0) * t) * np.exp(-t * 38.0) * 0.75
    return np.tanh((splinter + knock) * 1.4).astype(np.float32)


def main() -> None:
    print("Generating MW2019-style FPS audio assets...")
    for i, pitch in enumerate((1.00, 1.03, 0.97), start=1):
        write_wav(OUT_DIR / f"pistol_shot_indoor_{i:02d}.wav", make_indoor_shot(100 + i, pitch))
        write_wav(OUT_DIR / f"pistol_shot_outdoor_{i:02d}.wav", make_outdoor_shot(200 + i, pitch))
    write_wav(OUT_DIR / "pistol_tail_outdoor.wav", make_outdoor_tail(301))
    write_wav(OUT_DIR / "pistol_dry_fire.wav", make_dry_fire())
    write_wav(OUT_DIR / "pistol_reload_easy.wav", make_reload_easy())
    write_wav(OUT_DIR / "pistol_reload_full.wav", make_reload_full())
    write_wav(OUT_DIR / "shell_clink.wav", make_shell_clink())
    for i in (1, 2):
        write_wav(OUT_DIR / f"bullet_concrete_{i:02d}.wav", make_bullet_concrete(400 + i))
        write_wav(OUT_DIR / f"bullet_metal_{i:02d}.wav", make_bullet_metal(500 + i))
        write_wav(OUT_DIR / f"bullet_wood_{i:02d}.wav", make_bullet_wood(600 + i))
        write_wav(OUT_DIR / f"bullet_ricochet_{i:02d}.wav", make_bullet_ricochet(700 + i))


if __name__ == "__main__":
    main()
