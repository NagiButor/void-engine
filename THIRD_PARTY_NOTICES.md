# Third-Party Notices

V.O.I.D. Engine uses the following third-party components. The active Rust
runtime is resolved by Cargo; exact transitive versions are in `Cargo.lock`.

## Rust runtime (0.2)

| Component | Version | License | Use |
| --- | --- | --- | --- |
| wgpu / naga | 0.20 / 0.20 | MIT OR Apache-2.0 | Native graphics abstraction and WGSL translation |
| winit | 0.29 | Apache-2.0 | Window and input |
| Rapier / Parry / nalgebra | 0.22 / 0.17 / 0.33 | Apache-2.0 / Apache-2.0 / BSD-3-Clause | Native Rust rigid-body physics and mathematics |
| glam | 0.27 | MIT OR Apache-2.0 | Rendering mathematics |
| egui / egui-wgpu / egui-winit | 0.28 | MIT OR Apache-2.0 | Scalable UI and GPU integration |
| rodio / cpal / hound | 0.19 / 0.15 / 3.5 | MIT OR Apache-2.0 / Apache-2.0 / Apache-2.0 | Audio device and WAV decoding |
| gilrs | 0.11 | MIT OR Apache-2.0 | Cross-platform gamepad input and force feedback |
| image | 0.25 | MIT OR Apache-2.0 | Embedded PNG/JPEG decoding and PNG capture |
| serde / serde_json / toml | 1 / 1 / 0.8 | MIT OR Apache-2.0 | Settings and GLB JSON |
| bytemuck | 1 | MIT OR Apache-2.0 OR Zlib | GPU data layout |
| anyhow | 1 | MIT OR Apache-2.0 | Error propagation |
| pollster | 0.3 | MIT OR Apache-2.0 | Async device initialization |

Crate license texts ship in the respective registry source distributions.

## Runtime assets and system APIs

| Component | Version / pin | License | Use | Link |
| --- | --- | --- | --- | --- |
| **Roboto** | hinted regular TTF | Apache-2.0 | UI font | https://github.com/googlefonts/roboto-2 |
| **DevTextures** | Grid 4×4 PNG sets in `assets/textures/dev` | CC0 1.0 | Dev grid albedo, normal & roughness maps | `assets/textures/dev/LICENSE.txt` |
| **MOSS-SoundEffect v2.0** | generated clips in `assets/audio/sfx` | Apache-2.0 (model); generated output | AI-generated sound effects | https://huggingface.co/OpenMOSS-Team/MOSS-SoundEffect-v2.0 |
| **Footsteps Mini Sound Pack** | 90 clips in `assets/audio/sfx/footsteps` | Asset pack (user-supplied) | Surface-aware player footstep sounds (18 surfaces × 5 variations) | — |
| **DirectX 12 / DXGI** | Windows SDK | Microsoft SDK license | System graphics API | Windows SDK |
| **Vulkan loader** | GPU driver | Khronos / driver license | Runtime loader | system |

## Notes

- The Roboto font license is bundled at `assets/fonts/LICENSE.txt`.
- The clips in `assets/audio/sfx` were generated with MOSS-SoundEffect v2.0 by the OpenMOSS team (Apache-2.0). Reproducible prompts and seeds are in `tools/generate_sfx.py`; no model weights are redistributed in this repository.
- The `V.O.I.D.` manipulator concept art and the `assets/models/void-manipulator.glb` model were supplied by the project author (Mi5hlen) and are covered by the engine license, not by a third-party asset license. The embedded texture was downscaled to 2048x2048 for the repository build.
- Vulkan and DirectX are trademarks of their respective owners. This project is not affiliated with Valve, Microsoft, Khronos, or Erin Catto.
- If you add a dependency, update this file and keep all licenses compatible with commercial licensing. Avoid GPL/AGPL and other copyleft licenses for the engine runtime.
