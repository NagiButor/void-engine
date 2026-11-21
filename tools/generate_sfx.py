#!/usr/bin/env python3
"""Generate the PHYSICS LAB sound effects with MOSS-SoundEffect v2.0.

Two backends are supported:

  space  - call the official Hugging Face Space (needs HF_TOKEN for ZeroGPU quota)
  local  - run the Apache-2.0 weights locally with the OpenMOSS pipeline

Examples:
  python tools/generate_sfx.py --backend space --hf-token $env:HF_TOKEN
  python tools/generate_sfx.py --backend local --moss-dir C:/MOSS-TTS --device cpu

Generated files land in assets/audio/sfx/ and are shipped with the demo.
Prompts and seeds are fixed so the set can be reproduced.
"""

from __future__ import annotations

import argparse
import contextlib
import math
import os
import shutil
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "assets" / "audio" / "sfx"

# name, prompt, seconds, seed
CLIPS = [
    ("impact_wood",
     "A heavy wooden crate dropped onto a hard concrete floor, close perspective, "
     "single short thud with a hollow wooden knock, dry, no music, no voice, no long reverb",
     1.0, 11),
    ("impact_metal",
     "A steel barrel clanging against a concrete floor, close perspective, single short "
     "metallic clang with a brief ringing rattle, dry, no music, no voice",
     1.0, 12),
    ("impact_concrete",
     "A concrete brick dropped and cracking on a hard concrete floor, close perspective, "
     "short sharp impact with a small debris crunch, dry, no music, no voice",
     1.0, 13),
    ("impact_plastic",
     "A plastic traffic cone bouncing once on a concrete floor, close perspective, short "
     "hollow plastic thump, dry, no music, no voice",
     1.0, 14),
    ("grab",
     "A sci-fi gravity gun grabbing a heavy wooden crate, short electric zap and suction "
     "whoosh, close, dry, no music, no voice",
     1.0, 21),
    ("throw",
     "A sci-fi gravity gun launching a heavy crate, powerful short pneumatic whoosh, "
     "close, dry, no music, no voice",
     1.0, 22),
    ("spawn",
     "A sci-fi matter replicator materializing a wooden crate, short digital shimmer and "
     "soft energy pulse, close, dry, no music, no voice",
     1.0, 23),
    ("slowmo_on",
     "Time slowing down in a warehouse, deep cinematic pitch-bending drone descending, "
     "short, no music, no voice",
     1.5, 31),
    ("slowmo_off",
     "Time speeding back to normal, rising cinematic pitch sweep, short, no music, no voice",
     1.5, 32),
    ("ui_click",
     "A crisp sci-fi interface button click, short, clean, close, no music, no voice",
     0.6, 41),
]


def write_wav(path: Path, audio, sample_rate: int) -> None:
    import numpy as np
    import soundfile as sf

    if hasattr(audio, "detach"):
        audio = audio.detach().cpu().float().numpy()
    audio = np.asarray(audio)
    while audio.ndim > 1:
        audio = audio[0] if audio.shape[0] == 1 else audio.mean(axis=0)
    peak = float(np.max(np.abs(audio))) if audio.size else 0.0
    if peak > 1e-5:
        audio = audio * (0.92 / peak)
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(path, audio, sample_rate, subtype="PCM_16")
    print(f"  wrote {path.relative_to(ROOT)} ({audio.size / sample_rate:.2f}s)", flush=True)


def run_local(args) -> None:
    os.environ.setdefault("TORCHDYNAMO_DISABLE", "1")
    import torch

    sys.path.insert(0, str(Path(args.moss_dir).resolve()))
    from moss_soundeffect_v2 import MossSoundEffectPipeline

    device = args.device
    if device == "auto":
        device = "cuda" if torch.cuda.is_available() else "cpu"
    dtype = torch.bfloat16 if device == "cuda" else torch.float32
    if device == "cpu":
        # Skylake-era CPUs emulate bf16 slowly; fp32 without autocast is much
        # faster and uses the physical cores.
        torch.set_num_threads(max(1, (os.cpu_count() or 4) // 2))
        torch.autocast = lambda *a, **k: contextlib.nullcontext()
    print(f"loading MOSS-SoundEffect v2.0 on {device} ({dtype})", flush=True)
    kwargs = {"cache_dir": args.cache_dir} if args.cache_dir else {}
    pipe = MossSoundEffectPipeline.from_pretrained(
        args.model, torch_dtype=dtype, device=device, **kwargs
    )

    for name, prompt, seconds, seed in selected(args):
        # The DiT denoises a fixed window. A short window is much cheaper on CPU
        # and is enough for one-shot effects.
        pipe.max_inference_seconds = max(1, int(math.ceil(seconds)))
        print(f"generating {name} ({seconds}s, seed {seed})", flush=True)
        start = time.time()
        audio = pipe(
            prompt=prompt,
            seconds=seconds,
            num_inference_steps=args.steps,
            cfg_scale=args.cfg,
            sigma_shift=args.sigma,
            seed=seed,
        )
        print(f"  {time.time() - start:.1f}s", flush=True)
        write_wav(OUT_DIR / f"{name}.wav", audio, pipe.sample_rate)


def run_space(args) -> None:
    from gradio_client import Client

    token = args.hf_token or os.environ.get("HF_TOKEN") or os.environ.get("HUGGING_FACE_HUB_TOKEN")
    if not token:
        raise SystemExit("space backend needs --hf-token or HF_TOKEN (ZeroGPU quota)")
    client = Client(args.space, hf_token=token)
    for name, prompt, seconds, seed in selected(args):
        print(f"generating {name} ({seconds}s, seed {seed})", flush=True)
        result = client.predict(
            prompt=prompt,
            seconds=seconds,
            steps=args.steps,
            cfg_scale=args.cfg,
            sigma_shift=args.sigma,
            seed=seed,
            api_name="/run_inference",
        )
        source = result[0] if isinstance(result, (tuple, list)) else result
        OUT_DIR.mkdir(parents=True, exist_ok=True)
        target = OUT_DIR / f"{name}.wav"
        shutil.copyfile(source, target)
        print(f"  wrote {target.relative_to(ROOT)}", flush=True)


def selected(args):
    if not args.only:
        return CLIPS
    wanted = {name.strip() for name in args.only.split(",")}
    return [clip for clip in CLIPS if clip[0] in wanted]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["local", "space"], default="local")
    parser.add_argument("--model", default="OpenMOSS-Team/MOSS-SoundEffect-v2.0")
    parser.add_argument("--space", default="OpenMOSS-Team/MOSS-SoundEffect-v2.0")
    parser.add_argument("--moss-dir", default=os.environ.get("MOSS_TTS_DIR", "MOSS-TTS"))
    parser.add_argument("--device", default="auto", choices=["auto", "cpu", "cuda"])
    parser.add_argument("--cache-dir", default=os.environ.get("HF_HOME"),
                        help="Hugging Face cache (defaults to HF_HOME)")
    parser.add_argument("--steps", type=int, default=10)
    parser.add_argument("--cfg", type=float, default=4.0)
    parser.add_argument("--sigma", type=float, default=5.0)
    parser.add_argument("--hf-token", default=None)
    parser.add_argument("--only", default=None, help="comma-separated clip names")
    args = parser.parse_args()

    if args.backend == "local":
        run_local(args)
    else:
        run_space(args)


if __name__ == "__main__":
    main()
