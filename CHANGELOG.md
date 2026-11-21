# Changelog

## Unreleased

- Add **Visibility Bitmask Global Illumination (`assets/shaders/gi.wgsl`)**: screen-space horizon bitmask GI with 32-bit visibility sectors, 4-mip `direct_hdr` color bleeding (`fs_box_downsample` in `bloom.wgsl`), 256² top-down `sky_height_map` indoor sky occlusion, temporal reprojection with strict disocclusion rejection, and exact texel-aligned normal+depth joint bilateral denoising/upsampling (`assets/shaders/bilateral.wgsl`, `assets/shaders/composite.wgsl`).
- Add **Procedural Volumetric Clouds (`assets/shaders/clouds.wgsl`)** & **Volumetric Light Shafts (`assets/shaders/volumetric.wgsl`)**: quarter-resolution cloud raymarching driven by a generated 64³ Perlin-Worley/Worley 3D noise volume and 256² weather map (`crates/void-engine/src/celestial.rs`) with Henyey-Greenstein phase scattering and temporal reprojection, plus shadowed quarter-res Sun/Moon god-rays with bilateral upsampling.
- Add **Procedural Sun, Moon & Orbital Trajectories (`celestial.rs`, `deferred.wgsl`, `ui.rs`)**: 512² procedural Sun (granulation, sunspots, limb darkening) and Moon (maria, craters, dynamic lunar phase lighting) sky discs, dotted orbital trajectory arcs projected onto the sky dome, and an upgraded **НЕБО, СОЛНЦЕ И ЛУНА** UI submenu with live 2D trajectory diagram, cloud coverage, lunar phase, GI intensity, and volumetric shaft controls (`RenderSettings` / `settings.toml`).
- Split out `assets/shaders/point_shadow.wgsl` (`vs_point_shadow`) to keep dynamic-offset uniform buffer sizing compact (`64 B` slice) and pass strict Vulkan/naga validation across all 10 WGSL shaders.
- Add cross-platform gamepad support (`void_engine::input::GamepadInput`, powered by `gilrs 0.11`) with radial deadzone, cubic-blended aim curve, trigger hysteresis (`LT` ADS / `RT` fire or throw), full FPS & sandbox bindings (`LS` walk, `L3` sprint toggle, `RS` look, `A` jump, `X` reload, `Y` grab/release, `B` holster/back, `R3` inspect, `LB` slow-mo, `RB` spawn barrel, `D-Pad` lights/distance/delete, `Start` pause/resume, `View` HUD toggle), force-feedback rumble on gunshots/landings/grabs, adaptive HUD prompts, and a dedicated **ГЕЙМПАД** settings tab (`gamepad_sensitivity`, `gamepad_deadzone`, `gamepad_vibration` in `settings.toml`).
- Add first-person Springfield Armory XD-9 pistol (`assets/models/fps_pistol.glb`, converted from `Armpist.fbx` via `tools/convert_pistol_fbx.py`) with 1024×1024 PBR arm/pistol textures (`textures::ARMS`, `textures::PISTOL`), skeletal animation blending (`viewmodel::RiggedModel` — idle, walk, 3 fire variants, tactical & empty reload, empty slide lock-back), and an isolated depth-cleared foreground G-Buffer/deferred viewmodel pass (`render.rs`, `deferred.wgsl`).
- Add full FPS shooting & ballistics (`crates/void-demo/src/weapon.rs`): left-click firing (`LMB`) with dynamic spread, right-click Aim Down Sights (`RMB`), tactical (`15+1`) & empty reload (`R`), weapon holster/draw (`V`), weapon inspect (`T`), procedural mouse sway, strafe tilt, sprint cant, landing dip, spring-damper recoil climb, multi-frequency trauma screen shake (`Camera::roll`), muzzle flash sprite + transient `PointLight`, bullet tracers, tumbling brass shell casings with floor bounce & clink SFX, and physical bullet impulses on dynamic barrels (`Physics::apply_impulse_at_point`).
- Add surface-aware bullet holes & impact VFX (`textures::DECAL_CONCRETE`, `DECAL_METAL`, `DECAL_WOOD` with crater normal maps & alpha cutout in `gbuffer.wgsl`, attached in hit body local space up to 128 decals): sparks + ricochet flash on metal, concrete dust puffs + chips on masonry, wood splinters + dust on crates/benches.
- Add MW2019-style adaptive gunshot acoustics (`tools/generate_fps_sfx.py`, `Audio::play_gunshot`): real-time 9-ray ceiling/wall acoustic probing (`ПОМЕЩЕНИЕ` / `УКРЫТИЕ` / `УЛИЦА · ЭХО`) that crossfades tight pressurized indoor room gunshots with open-air outdoor cracks + distant rolling tail + delayed stereo-panned wall slapback echoes, plus reload, dry-fire, shell clink, and surface-specific bullet impact WAVs.
- Move scene reset keybinding from `R` to `F5` / `Backspace` so `R` performs FPS reload; add ammo/acoustics pill and hitmarker to `ui.rs`.
- Replace legacy `footstep_1.wav` / `footstep_2.wav` with all 18 surfaces × 5 variations (90 clips converted to 48 kHz mono PCM_16 in `assets/audio/sfx/footsteps/`) from **Footsteps Mini Sound Pack**, add `FootstepSurface` & `Audio::play_footstep` in `void-engine`, add `locomotion::FootstepGait` to synchronize footstep/landing triggers with distance-driven camera head-bob and slow-motion, balance per-surface loudness (`gain_trim`), and assign `PavementTiles` as the default test hall floor surface (`MetalSteps` on metallic blocks/barrels).
- Replace single-pass spiral bloom in `post.wgsl` with a 5-mip pyramid HDR bloom pass (`assets/shaders/bloom.wgsl` — Karis-averaged soft-knee prefilter, 13-tap box downsample, and 9-tap tent additive upsample), eliminating ghost cube artifacts around bright point-light emitters.
- Add canonical `units` system (`crates/void-engine/src/units.rs`) defining right-handed Y-up coordinates, `1:1` SI metric scale (`1 unit = 1 m`), unit conversions, `RealWorldBounds` mesh scaling, industry-standard `Hor+` (16:9) camera/POV/sensitivity constants (`units::camera`), and real-world `human`, `world`, and `props` ratios, documented in `docs/scale_and_coordinates.md`, `README.md`, and `PROJECT.md`.
- Standardize camera & POV settings (`90°` `Hor+` 16:9 HFOV in `70–120°`, `1.65 m` floor-relative standing POV eye height in `1.50–1.74 m`, `±85°` pitch limit, `0.110°/count` mouse scale, and `invert_y` toggle) with automatic legacy `settings.toml` eye-height migration.
- Rescale player capsule (`1.80 m` stature, `0.30 m` radius, `1.65 m` eye level), locomotion/reach parameters, `30×30 m` test hall & architectural zones (`0.25 m` walls, `2.80 m` alcove, `2.10 m` door monolith, `0.20–1.80 m` ergonomic height gauge), and normalize `old_tin_barrel` to ISO 200 L drum dimensions (`Ø 0.585 × 0.880 m`, `20.0 kg`).
- Add the `old_tin_barrel` GLB prop (`assets/models/old_tin_barrel.glb`) with PBR albedo/roughness/normal maps (`textures::BARREL` layer) and Rapier cylinder physics + metal impact audio.
- Add interactive prop spawning (`F`), grabbing/releasing (`E`), throwing (`LMB`), and deletion (`X` / `Delete`), plus dynamic point-light placement (`G` / `L`), color preset selection (`1`–`6` / `C`), and removal (`X` / `Delete`) up to 16 active point lights.
- Remove first-person weapon/hand rig (`rig.rs`), `void-manipulator.glb`, viewmodel render pass, and legacy scene props, keeping the Rapier 3D physics core and architectural hall intact.
- Redesign the UI (`crates/void-demo/src/ui.rs`) with an Obsidian-Glass & Solar-Amber aesthetic: animated geometric V.O.I.D. emblem, custom animated buttons/toggles/sliders/segmented controls, tabbed settings modal, live frame-time sparkline HUD, physics kinematics bar, dynamic reticle, and keycap hints.

## 0.2.0 — 2026-11-10

### Rust runtime

- Replace the C++ runtime with a Cargo workspace: reusable `void-engine` library
  and separate `void-demo` application.
- Render through native DX12 and Vulkan using wgpu and WGSL shaders.
- Move rigid-body physics, character movement, picking, spring grab, throwing,
  collision filtering, reset, spawning and slow motion to Rapier.
- Port spatial audio, the bounded 32-voice HDR mixer, limiter and WAV playback.
- Add scalable Cyrillic UI, live graphics/camera/audio controls and persistent
  validated TOML settings.

### Visual quality and first-person rig

- Add linear HDR lighting, GGX shading, PCF shadows, contact shading, bloom,
  edge smoothing and ACES tone mapping.
- Generate seamless colour/roughness and normal atlases with mipmaps and
  anisotropic filtering.
- Rebuild crates, barrels, pallets, jerry cans, dominoes and cones with detailed
  geometry, bevels, hardware and material variation.
- Expand the laboratory with roof trusses, lamps, loading door, ventilation,
  shelving, cargo deck and safety rails.
- Attach hands and weapon to one animated rig with metre-scale proportions,
  articulated fingers and elbow IK. Use a 56 cm manipulator and an independent
  62° viewmodel projection and depth buffer.

### Repository and verification

- Remove the previous C++ sources/headers, HLSL shaders, CMake files, legacy
  documentation and superseded screenshots.
- Add Cargo-based Windows CI and the PowerShell build/package script.
- Refresh README, dependency notices and screenshots for the Rust runtime.
- Add physics, audio, shader and IK tests, a headless self-test, GPU PNG capture
  and adapter-labelled benchmarks.
