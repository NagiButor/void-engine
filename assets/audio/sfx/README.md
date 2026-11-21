# Sound effects

These clips are generated with **MOSS-SoundEffect v2.0** by the OpenMOSS team
(Apache-2.0), using the prompts, durations and seeds in
[`tools/generate_sfx.py`](../../../tools/generate_sfx.py).

They are 48 kHz mono WAV files, shaped for gameplay by
[`tools/postprocess_sfx.py`](../../../tools/postprocess_sfx.py) and normalized/
trimmed at load time by `vd::AudioKit`. If a file is missing, the engine falls
back to its small procedural synthesizer, so the demo still runs.

| File | Used for |
| --- | --- |
| `impact_wood.wav` | wooden crate / plank impacts |
| `impact_metal.wav` | barrels, cans and steel balls |
| `impact_concrete.wav` | bricks and concrete props |
| `impact_plastic.wav` | traffic cones |
| `grab.wav` | gravity-gun grab |
| `throw.wav` | throw / shove |
| `spawn.wav` | prop spawn and scene reset |
| `slowmo_on.wav`, `slowmo_off.wav` | slow-motion toggle |
| `ui_click.wav` | menu, pause and button clicks |
| `footsteps/<surface>_01..05.wav` | surface-aware player footsteps (18 surfaces × 5 variations from Footsteps Mini Sound Pack, converted to 48 kHz mono PCM_16 with per-surface `gain_trim` and `FootstepGait` synchronization; `pavement_tiles` used for the test hall floor, `metal_steps` for metallic blocks/barrels) |

Regenerate with either the official Hugging Face Space or a local checkout of
the [MOSS-TTS](https://github.com/OpenMOSS/MOSS-TTS) pipeline:

```powershell
python tools/generate_sfx.py --backend space --hf-token $env:HF_TOKEN
python tools/generate_sfx.py --backend local --moss-dir C:/MOSS-TTS --device cpu --steps 30
python tools/postprocess_sfx.py
```
