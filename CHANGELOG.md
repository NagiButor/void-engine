# Changelog

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
