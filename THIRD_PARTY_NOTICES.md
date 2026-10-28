# Third-Party Notices

V.O.I.D. Engine uses the following third-party components. They are fetched by CMake at configure time unless noted otherwise. All are permissive licenses; no copyleft dependencies are included.

| Component | Version / pin | License | Use | Link |
| --- | --- | --- | --- | --- |
| **Box3D** | `v0.1.0` | MIT | Native 3D rigid-body physics | https://github.com/erincatto/box3d |
| **SDL3** | `release-3.4.18` | zlib | Window, input, audio device, platform | https://github.com/libsdl-org/SDL |
| **GLM** | `1.0.3` | MIT | Math | https://github.com/g-truc/glm |
| **volk** | Vulkan SDK 1.4.363 tag | MIT | Vulkan function loading | https://github.com/zeux/volk |
| **Vulkan-Headers** | `vulkan-sdk-1.4.363.0` | Apache-2.0 | Vulkan declarations | https://github.com/KhronosGroup/Vulkan-Headers |
| **stb_truetype / stb_image** | commit `003501f6` | MIT | Font atlas rasterization and GLB texture decoding | https://github.com/nothings/stb |
| **nlohmann/json** | `v3.12.0` | MIT | glTF/GLB JSON parsing | https://github.com/nlohmann/json |
| **Roboto** | hinted regular TTF | Apache-2.0 | UI font | https://github.com/googlefonts/roboto-2 |
| **DirectXShaderCompiler** | `v1.9.2609` release | Apache-2.0 with LLVM exception | Build-time DXIL/SPIR-V shader compilation | https://github.com/microsoft/DirectXShaderCompiler |
| **MOSS-SoundEffect v2.0** | generated clips in `assets/audio/sfx` | Apache-2.0 (model); generated output | AI-generated sound effects | https://huggingface.co/OpenMOSS-Team/MOSS-SoundEffect-v2.0 |
| **DirectX 12 / DXGI** | Windows SDK | Microsoft SDK license | System graphics API | Windows SDK |
| **Vulkan loader** | GPU driver | Khronos / driver license | Runtime loader | system |

## Notes

- Box3D is the work of Erin Catto. Its MIT license ships from the upstream repository and applies to the physics engine.
- SDL3's zlib license permits commercial use with attribution; the SDL copyright notice is retained in the upstream source fetched by CMake.
- The Roboto font license is bundled at `assets/fonts/LICENSE.txt`.
- DXC is used only as a build-time tool and is not shipped as part of the demo executable. If you redistribute the build tree containing DXC, retain its license and notices.
- The clips in `assets/audio/sfx` were generated with MOSS-SoundEffect v2.0 by the OpenMOSS team (Apache-2.0). Reproducible prompts and seeds are in `tools/generate_sfx.py`; no model weights are redistributed in this repository.
- The `V.O.I.D.` manipulator concept art and the `assets/models/void-manipulator.glb` model were supplied by the project author (Mi5hlen) and are covered by the engine license, not by a third-party asset license. The embedded texture was downscaled to 2048x2048 for the repository build.
- Vulkan and DirectX are trademarks of their respective owners. This project is not affiliated with Valve, Microsoft, Khronos, or Erin Catto.
- If you add a dependency, update this file and keep all licenses compatible with commercial licensing. Avoid GPL/AGPL and other copyleft licenses for the engine runtime.
