import argparse
import os

import numpy as np
import soundfile as sf

SR = 48000

# tau (decay seconds), length (seconds), lowpass, highpass, thump frequency
RECIPES = {
    "impact_wood":     (0.16, 0.50, 12000, 35, 88.0, 0.22),
    "impact_metal":    (0.28, 0.65, 14000, 45, 64.0, 0.16),
    "impact_concrete": (0.15, 0.48, 12000, 35, 72.0, 0.24),
    "impact_plastic":  (0.13, 0.42, 12000, 40, 115.0, 0.18),
    "grab":            (0.16, 0.45, 10000, 45, None, 0.0),
    "throw":           (0.18, 0.50, 11000, 45, None, 0.0),
    "spawn":           (0.28, 0.75, 11000, 45, None, 0.0),
    "slowmo_on":       (None, 1.50, 8000, 30, None, 0.0),
    "slowmo_off":      (None, 1.50, 8000, 30, None, 0.0),
    "ui_click":        (0.022, 0.13, 14000, 450, None, 0.0),
}


def lowpass(x, cutoff):
    if cutoff >= SR / 2:
        return x
    rc = 1.0 / (2.0 * np.pi * cutoff)
    dt = 1.0 / SR
    alpha = dt / (rc + dt)
    y = np.empty_like(x)
    acc = 0.0
    for i, sample in enumerate(x):
        acc += alpha * (sample - acc)
        y[i] = acc
    return y


def highpass(x, cutoff):
    rc = 1.0 / (2.0 * np.pi * cutoff)
    dt = 1.0 / SR
    alpha = rc / (rc + dt)
    y = np.empty_like(x)
    prev_x = 0.0
    prev_y = 0.0
    for i, sample in enumerate(x):
        prev_y = alpha * (prev_y + sample - prev_x)
        prev_x = sample
        y[i] = prev_y
    return y


def process(name, x):
    tau, length, lp, hp, thump_hz, thump_gain = RECIPES.get(
        name, (0.2, 0.6, 12000, 40, None, 0.0)
    )
    n = min(len(x), int(length * SR))
    x = x[:n].astype(np.float32)
    t = np.arange(n, dtype=np.float32) / SR

    if tau is not None:
        env = np.exp(-t / tau)
    else:
        env = np.ones_like(t)
    attack = np.clip(t / 0.002, 0.0, 1.0)
    release_start = max(0.0, (n / SR) - (0.25 if tau is None else 0.01))
    release = np.clip((n / SR - t) / max(1e-4, (n / SR) - release_start), 0.0, 1.0)
    x *= env * attack * release

    x = highpass(x, hp)
    x = lowpass(x, lp)

    if thump_hz is not None and thump_gain > 0.0:
        thump = np.sin(2.0 * np.pi * thump_hz * t) * np.exp(-t / 0.075) * thump_gain
        x = x + thump

    # tiny transient click for interface feedback
    if name == "ui_click":
        ping = np.sin(2.0 * np.pi * 1650.0 * t) * np.exp(-t / 0.018) * 0.35
        x = x + ping

    peak = float(np.max(np.abs(x))) if x.size else 0.0
    if peak > 1e-6:
        x = x * (0.9 / peak)
    return x


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--src", default=os.path.join("assets", "audio", "sfx"))
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    out_dir = args.out or args.src
    os.makedirs(out_dir, exist_ok=True)
    names = [n for n in RECIPES if os.path.exists(os.path.join(args.src, n + ".wav"))]
    for name in sorted(names):
        x, sr = sf.read(os.path.join(args.src, name + ".wav"), dtype="float32")
        if x.ndim > 1:
            x = x.mean(axis=1)
        if sr != SR:
            # simple linear resample
            count = int(len(x) * SR / sr)
            x = np.interp(np.linspace(0, len(x) - 1, count), np.arange(len(x)), x)
        y = process(name, x)
        sf.write(os.path.join(out_dir, name + ".wav"), y, SR, subtype="PCM_16")
        print(f"{name:18s} {len(y)/SR:.2f}s peak={np.max(np.abs(y)):.3f} rms={np.sqrt(np.mean(y*y)):.3f}")
    print("written to", out_dir)


if __name__ == "__main__":
    main()
