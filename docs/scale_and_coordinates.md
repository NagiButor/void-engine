# V.O.I.D. Engine — Coordinate System, Units & Real-World Scale

> Canonical reference implemented in [`crates/void-engine/src/units.rs`](../crates/void-engine/src/units.rs).

V.O.I.D. Engine operates on a strict **1:1 SI metric scale** (`1.0` engine unit = `1.0` real-world meter) so that physics, camera perspective, lighting attenuation, spatial audio, imported GLB assets, and architectural geometry share a single physical frame of reference.

---

## 1. Coordinate System (`units::coord`)

| Property | Specification |
| --- | --- |
| **Handedness** | Right-handed 3D Cartesian (`X × Y = Z`, `glam` RH matrices) |
| **Up Axis** | `+Y` (Zenith) |
| **Right Axis** | `+X` (East) |
| **Forward Axis** | `-Z` (North at `yaw = 0`, `pitch = 0`) |
| **Backward Axis** | `+Z` (South) |
| **Gravity** | `-Y` (`9.80665 m/s²`) |
| **Ground Datum** | `Y = 0.00 m` (top surface of floor slab) |
| **Angular Units** | Radians; positive `yaw` rotates CCW around `+Y` (from `-Z` toward `-X`); `pitch` clamped to `±1.48 rad` (`±84.8°`) |

### Dev Texture World Grid

Procedural and architectural blocks use world-space tiled UVs (`TILE_SIZE_M = 1.0 m`):

- **1 UV tile (`0..1`)** = `1.00 m × 1.00 m`
- **1 cell of the `4×4` dev grid** = `0.25 m × 0.25 m` (`25 cm`)

Every floor tile, wall, and block in the test hall therefore serves as an exact metric ruler.

---

## 2. Scale Ratios & Conversions (`units`)

- `UNITS_PER_METER = 1.0`, `METERS_PER_UNIT = 1.0` (`1:1` real-world ratio).
- Metric & imperial constructors: `mm()`, `cm()`, `m()`, `km()`, `inches()`, `feet()`.
- Reverse & velocity conversions: `to_cm()`, `to_mm()`, `kmh_to_mps()`, `mps_to_kmh()`.
- Mesh inspection & normalization: `RealWorldBounds::from_mesh()` measures `(width, height, depth)` in meters and computes `scale_for_height(target_height_m)` and `human_height_ratio()` so external GLB models authored at arbitrary scales normalize cleanly to real-world dimensions.

---

## 3. Human & Player Proportions (`units::human`)

Player physics, camera placement, and interaction ranges are modeled around a standing `1.80 m` adult human (`1:1`):

| Parameter | Engine Value | Real-World Reference | Ratio to Human Height |
| --- | --- | --- | --- |
| Standing Height (`HEIGHT`) | `1.80 m` | Adult standing stature | `1.00×` |
| Capsule Radius (`RADIUS`) | `0.30 m` (`0.60 m` dia.) | Shoulder breadth + clearance (fits `1.00 m` doors) | `0.33×` (width) |
| Capsule Half-Segment | `0.60 m` | `2 × 0.60 + 2 × 0.30 = 1.80 m` | — |
| Capsule Center (`CENTER_HEIGHT`) | `0.90 m` | Mid-hip / center of mass | `0.50×` |
| Eye Level (`EYE_HEIGHT`) | `1.65 m` (`+0.75 m` offset) | Standing eye height (`~91.7%` of stature) | `0.92×` |
| Configurable Eye Range | `1.50 – 1.74 m` (`+0.60..+0.84 m`) | Strictly inside the `1.80 m` head crown | `0.83–0.97×` |
| Body Mass (`MASS_KG`) | `78.0 kg` | Average adult mass | — |
| Base Move Speed (`WALK_SPEED`) | `3.6 m/s` (`13.0 km/h`) | Brisk FPS jog | `2.00× / s` |
| Sprint Speed (`SPRINT_SPEED`) | `6.4 m/s` (`23.0 km/h`) | Human sprint | `3.56× / s` |
| Jump Velocity (`JUMP_VELOCITY`) | `4.6 m/s` (`~1.08 m` apex) | Clears `0.90 m` counters/crates | — |
| Base Step Stride (`STEP_STRIDE`) | `1.80 m` (scales `0.78 + 0.22·v/v_walk`) | `2.0 Hz` walk / `~3.0 Hz` sprint cadence (`locomotion::FootstepGait`) | `1.00×` |
| Anatomical Arm Reach (`ARM_REACH`) | `0.80 m` | Human arm length | `0.44×` |
| Prop Pick / Hold Range | `1.00 – 3.50 m` | Comfortable first-person manipulation | `0.56–1.94×` |
| Throw Velocity | `13.0 m/s` (`+1.5 m/s` lift) | `~46.8 km/h` throw | — |

---

## 4. World & Architectural Proportions (`units::world`)

Architectural references and the `30 × 30 m` (`900 m²`) test hall follow real-world building dimensions:

| Element | Dimensions (W × H × D) | Real-World Equivalent | Ratio to Human (`1.80 m`) |
| --- | --- | --- | --- |
| Test Hall Floor | `30.0 × 0.30 × 30.0 m` | Warehouse / courtyard (`900 m²`, `30 cm` slab) | `16.67×` span |
| Perimeter Walls | `4.50 m` tall, `0.25 m` thick | `1.5×` standard `3.00 m` story, `25 cm` masonry | `2.50×` height |
| Hero Stack (Center) | `1.20 m` cube + `0.60 m` crate | Chest barrier + crate (`1.80 m` combined) | `1.00×` height |
| Hero Counter Block | `0.90 × 0.90 × 0.90 m` | Standard counter / workbench height | `0.50×` height |
| Hero Bench Block | `0.45 × 0.45 × 0.45 m` | Standard seat / bench height | `0.25×` height |
| West Column | `0.80 × 4.20 × 0.80 m` | Structural concrete column | `2.33×` height |
| West Door Monolith | `0.70 × 2.10 × 0.70 m` | Standard `2.10 m` doorway height reference | `1.17×` height |
| West Canopy Arch | `2.40 m` clear, `0.25 m` slab | Industrial pergola / loading canopy | `1.33×` height |
| NW Workshop Alcove | `5.0 × 2.80 × 5.0 m` (`2.20 m` gate) | Standard `2.80 m` room ceiling + `0.90 m` table | `1.56×` ceiling |
| East Calibration Bench | `1.20 × 0.45 × 7.50 m` + `0.60 m` cubes | `0.45 m` plinth, cubes top at `1.05 m` | `0.58×` inspection |
| North Height Gauge | `0.20, 0.45, 0.90, 1.20, 1.80 m` | Stair riser, seat, counter, barrier, human | `0.11× – 1.00×` |
| Luminaire Indicators | `0.16 m` cube (`0.19` hover / `0.21` held) | Compact ceiling/work-light fixture | `0.09×` height |
| Camera Clip Planes | `0.06 m` near / `160.0 m` far | `6 cm` close-up clearance | — |

---

## 5. Physical Prop Standards (`units::props`)

| Prop | Asset / Shape | Dimensions | Mass | Ratio to Human |
| --- | --- | --- | --- | --- |
| **200 L Steel Drum** | `old_tin_barrel.glb` -> `Shape::Cylinder(0.2925, 0.880)` | `Ø 0.585 m × 0.880 m` (`585 × 880 mm`) | `20.0 kg` | `0.49×` height (waist-high) |

Imported props are normalized at load time via `units::mesh_scale_for_height` so rendered geometry and Rapier collision shapes match real-world dimensions to within a millimeter.

---

## 6. Camera, POV & Projection Standards (`units::camera`)

V.O.I.D. Engine uses the PC first-person **`Hor+` (16:9 reference Horizontal FOV)** standard, decoupling the user-facing field of view from monitor aspect ratio so ultrawide displays (`21:9`, `32:9`) expand peripheral vision horizontally instead of cropping vertically (`Vert-`).

### Projection & Field of View

| Parameter | Value / Range | Standard & Rationale |
| --- | --- | --- |
| **Scaling Rule** | `Hor+` (`REFERENCE_ASPECT = 16/9`) | Constant vertical FOV across aspect ratios (`vfov = 2·atan(tan(hfov₁₆:₉/2) / (16/9))`) |
| **Default Gameplay FOV** | `90.0°` HFOV (`58.72°` VFOV) | Industry baseline (`~18 mm` 35mm full-frame equivalent); natural 1:1 depth perception without fisheye |
| **Menu / Showcase FOV** | `75.0°` HFOV (`46.46°` VFOV) | Architectural lens (`~23.5 mm` 35mm eq.) for low perspective distortion |
| **Configurable Range** | `70.0° – 120.0°` HFOV (`42.9°–88.3°` VFOV) | Covers narrow tactical to wide competitive PC framing |
| **Ultrawide (`21:9`) at `90°`** | `~106.7°` effective HFOV | Computed via `camera::hfov_deg_for_aspect(90.0, 21.0/9.0)` |
| **Near / Far Clip** | `0.06 m` / `160.0 m` | `6 cm` near clearance prevents held-prop clipping while preserving `Depth32Float` precision |

### POV & Camera Input

| Parameter | Value / Range | Standard & Rationale |
| --- | --- | --- |
| **Standing POV (`eye_height`)** | `1.65 m` above floor (`1.50 – 1.74 m`) | Real-world standing eye datum (`91.7%` of `1.80 m` stature); stored floor-relative (`eye_offset = eye_height - 0.90 m`), always inside the capsule |
| **Lateral Offset (`camera_side`)** | `0.00 m` (`±0.20 m`) | Centered binocular POV (`±20 cm` shoulder offset) |
| **Head Bob (`HEAD_BOB_AMPLITUDE`)** | `0.008 m` (`8 mm`) | Subtle gait oscillation (`sin(phase·2π)`), phase-locked via `locomotion::FootstepGait` so footsteps trigger at the exact trough (`phase = 0.75`) and on landing (`≥ 1.80 m/s`), toggleable for comfort |
| **Pitch Clamp (`MAX_PITCH_DEG`)** | `±85.0°` (`±1.4835 rad`) | Prevents zenith/nadir gimbal flip while allowing full vertical inspection |
| **Mouse Scale (`MOUSE_DEG_PER_COUNT`)** | `0.110°/count` (`5 × 0.022°`) | Source/Quake `m_yaw = 0.022°` lineage (`~10.4 cm / 360°` at `1.0×` and 800 DPI) |
| **Sensitivity & Y-Axis** | `0.20× – 3.00×` (`1.00×` default), `invert_y` | `~3.5 – 52 cm / 360°` at 800 DPI; optional vertical inversion |

