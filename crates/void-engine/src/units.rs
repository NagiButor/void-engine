//! Canonical coordinate system, real-world SI metric units, and human/world/prop ratios.
//!
//! # Coordinate System
//! - **Scale:** `1.0` engine unit = `1.0` meter (`1:1` SI metric).
//! - **Handedness:** Right-handed 3D Cartesian space (`+X` right, `+Y` up, `-Z` forward).
//! - **Datum:** Ground plane at `Y = 0.0` m; gravity along `-Y` (`9.80665` m/s²).
//! - **Angles:** Radians; `yaw = 0` looks along `-Z`, positive `yaw` rotates CCW around `+Y`.

use crate::mesh::MeshData;
use glam::Vec3;

/// Engine units per real-world meter (`1:1` SI metric).
pub const UNITS_PER_METER: f32 = 1.0;
/// Real-world meters per engine unit (`1:1` SI metric).
pub const METERS_PER_UNIT: f32 = 1.0;

/// Canonical coordinate axes and physical constants.
pub mod coord {
    use glam::Vec3;

    /// +X axis: world Right / East.
    pub const RIGHT: Vec3 = Vec3::X;
    /// +Y axis: world Up / Zenith.
    pub const UP: Vec3 = Vec3::Y;
    /// -Z axis: default Forward / North (`yaw = 0`, `pitch = 0`).
    pub const FORWARD: Vec3 = Vec3::NEG_Z;
    /// +Z axis: world Backward / South.
    pub const BACKWARD: Vec3 = Vec3::Z;
    /// -Y axis: gravity direction.
    pub const GRAVITY_DIR: Vec3 = Vec3::NEG_Y;

    /// Reference floor datum height in meters.
    pub const GROUND_Y: f32 = 0.0;
    /// Standard Earth gravitational acceleration (m/s²).
    pub const GRAVITY_MPS2: f32 = 9.80665;
    /// Maximum camera pitch magnitude in radians (~84.8°).
    pub const MAX_PITCH_RAD: f32 = 1.48;

    /// World-space repeat length of a `1×1` dev UV tile (meters).
    pub const TILE_SIZE_M: f32 = 1.0;
    /// Width of one cell in the `4×4` dev grid texture (meters).
    pub const GRID_CELL_M: f32 = TILE_SIZE_M / 4.0;

    /// Computes a unit forward direction from `yaw` (around `+Y`, 0 = `-Z`) and `pitch`.
    #[inline]
    pub fn forward_from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
        Vec3::new(
            -yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        )
    }

    /// Computes the horizontal forward direction on the XZ plane from `yaw`.
    #[inline]
    pub fn horizontal_forward(yaw: f32) -> Vec3 {
        Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
    }

    /// Computes the horizontal right vector from `yaw`.
    #[inline]
    pub fn right_from_yaw(yaw: f32) -> Vec3 {
        Vec3::new(yaw.cos(), 0.0, -yaw.sin())
    }

    /// Computes `yaw` in radians for a horizontal direction (`0` along `-Z`).
    #[inline]
    pub fn yaw_from_dir(dir: Vec3) -> f32 {
        (-dir.x).atan2(-dir.z)
    }

    /// Computes `(yaw, pitch)` aiming from `from` toward `to`.
    #[inline]
    pub fn yaw_pitch_to_target(from: Vec3, to: Vec3) -> (f32, f32) {
        let dir = (to - from).normalize_or_zero();
        (yaw_from_dir(dir), dir.y.clamp(-1.0, 1.0).asin())
    }
}

/// Real-world human anthropometric & first-person locomotion proportions (`1:1`).
pub mod human {
    /// Standing adult human reference height (m).
    pub const HEIGHT: f32 = 1.80;
    /// Collision capsule horizontal radius (m); diameter `0.60` m fits `0.90–1.00` m doors.
    pub const RADIUS: f32 = 0.30;
    /// Capsule cylindrical half-segment (`2 * 0.60 + 2 * 0.30 = 1.80` m total height).
    pub const CAPSULE_HALF_SEGMENT: f32 = HEIGHT * 0.5 - RADIUS;
    /// Capsule center height above floor when standing (`0.90` m).
    pub const CENTER_HEIGHT: f32 = HEIGHT * 0.5;
    /// Vertical spawn clearance above floor datum (m).
    pub const SPAWN_CLEARANCE: f32 = 0.02;
    /// Default spawn center Y coordinate (m).
    pub const SPAWN_Y: f32 = CENTER_HEIGHT + SPAWN_CLEARANCE;
    /// Downward ground-check raycast length from capsule center (m).
    pub const GROUND_PROBE_DIST: f32 = CENTER_HEIGHT + 0.08;

    /// Standing eye level above floor datum (m, ~91.7% of stature).
    pub const EYE_HEIGHT: f32 = 1.65;
    /// Minimum configurable standing eye height above floor (m, `83.3%` of stature).
    pub const EYE_HEIGHT_MIN: f32 = 1.50;
    /// Maximum configurable standing eye height above floor (m, strictly below `1.80` m crown).
    pub const EYE_HEIGHT_MAX: f32 = 1.74;
    /// Default eye offset above capsule center (`1.65 - 0.90 = 0.75` m).
    pub const EYE_OFFSET: f32 = EYE_HEIGHT - CENTER_HEIGHT;
    /// Minimum eye offset above capsule center (`1.50 - 0.90 = 0.60` m).
    pub const EYE_OFFSET_MIN: f32 = EYE_HEIGHT_MIN - CENTER_HEIGHT;
    /// Maximum eye offset above capsule center (`1.74 - 0.90 = 0.84` m).
    pub const EYE_OFFSET_MAX: f32 = EYE_HEIGHT_MAX - CENTER_HEIGHT;
    /// Maximum lateral camera shoulder offset (m).
    pub const CAMERA_SIDE_MAX: f32 = 0.20;

    /// Average adult body mass (kg).
    pub const MASS_KG: f32 = 78.0;
    /// Brisk base movement speed (m/s, ~13.0 km/h).
    pub const WALK_SPEED: f32 = 3.6;
    /// Sprint speed (m/s, ~23.0 km/h).
    pub const SPRINT_SPEED: f32 = 6.4;
    /// Vertical jump impulse speed (m/s; ~1.08 m apex under `9.80665` m/s²).
    pub const JUMP_VELOCITY: f32 = 4.6;
    /// Distance traveled per full left+right gait cycle (m).
    pub const STEP_STRIDE: f32 = 1.80;
    /// Subtle vertical camera bob amplitude while walking (m).
    pub const HEAD_BOB_AMPLITUDE: f32 = 0.008;

    /// Anatomical arm reach (m).
    pub const ARM_REACH: f32 = 0.80;
    /// Maximum prop pick/interaction range (m).
    pub const INTERACT_RANGE: f32 = 3.5;
    /// Minimum held-prop distance from eye (m).
    pub const HOLD_DIST_MIN: f32 = 1.0;
    /// Maximum held-prop distance from eye (m).
    pub const HOLD_DIST_MAX: f32 = 3.5;
    /// Maximum point-light pick range (m).
    pub const LIGHT_PICK_RANGE: f32 = 12.0;
    /// Minimum held point-light distance from eye (m).
    pub const LIGHT_HOLD_MIN: f32 = 1.0;
    /// Maximum held point-light distance from eye (m).
    pub const LIGHT_HOLD_MAX: f32 = 10.0;
    /// Forward throw speed for held props (m/s).
    pub const THROW_SPEED: f32 = 13.0;
    /// Upward throw lift for held props (m/s).
    pub const THROW_LIFT: f32 = 1.5;
}

/// Real-world architectural and test-environment dimensions (`1:1`).
pub mod world {
    /// Standard architectural story height (m).
    pub const STORY_HEIGHT: f32 = 3.00;
    /// Standard room clear ceiling height (m).
    pub const CEILING_CLEARANCE: f32 = 2.80;
    /// Standard single doorway width (m).
    pub const DOOR_WIDTH: f32 = 1.00;
    /// Standard single doorway height (m).
    pub const DOOR_HEIGHT: f32 = 2.10;
    /// Industrial gate/alcove entrance width (m).
    pub const GATE_WIDTH: f32 = 2.20;
    /// Industrial gate/alcove entrance height (m).
    pub const GATE_HEIGHT: f32 = 2.50;
    /// Standard masonry/concrete wall thickness (m).
    pub const WALL_THICKNESS: f32 = 0.25;
    /// Structural floor/roof slab thickness (m).
    pub const SLAB_THICKNESS: f32 = 0.30;
    /// Standard stair riser height (m).
    pub const STAIR_RISER: f32 = 0.20;
    /// Bench / plinth seating height (m, `0.25×` human height).
    pub const BENCH_HEIGHT: f32 = 0.45;
    /// Workbench / counter height (m, `0.50×` human height).
    pub const COUNTER_HEIGHT: f32 = 0.90;
    /// Chest-high barrier / railing height (m, `0.67×` human height).
    pub const BARRIER_HEIGHT: f32 = 1.20;

    /// Half-extent of the `30×30` m test hall (m).
    pub const ROOM_HALF: f32 = 15.0;
    /// Perimeter wall height (m, `1.5×` story height).
    pub const WALL_HEIGHT: f32 = 4.50;
    /// Directional shadow orthographic half-extent covering the hall (m).
    pub const SHADOW_EXTENT: f32 = 24.0;
    /// Camera near clip plane (m).
    pub const CAMERA_NEAR: f32 = super::camera::NEAR_CLIP_M;
    /// Camera far clip plane (m).
    pub const CAMERA_FAR: f32 = super::camera::FAR_CLIP_M;
    /// Luminaire indicator cube base edge length (m).
    pub const LIGHT_NODE_SIZE: f32 = 0.16;
}

/// Industry-standard first-person camera, POV, and `Hor+` (16:9) projection conventions.
pub mod camera {
    use std::f32::consts::PI;

    /// Reference widescreen aspect ratio for `Hor+` FOV scaling (`16:9`).
    pub const REFERENCE_ASPECT: f32 = 16.0 / 9.0;
    /// Default first-person horizontal FOV at `16:9` (degrees, `~58.7°` vertical FOV).
    pub const DEFAULT_HFOV_DEG: f32 = 90.0;
    /// Architectural showcase/menu horizontal FOV at `16:9` (degrees, `~46.5°` vertical FOV).
    pub const MENU_HFOV_DEG: f32 = 75.0;
    /// Minimum user horizontal FOV at `16:9` (degrees).
    pub const HFOV_MIN_DEG: f32 = 70.0;
    /// Maximum user horizontal FOV at `16:9` (degrees).
    pub const HFOV_MAX_DEG: f32 = 120.0;

    /// Near clip plane distance (m); avoids held-prop clipping while keeping 32-bit depth precision.
    pub const NEAR_CLIP_M: f32 = 0.06;
    /// Far clip plane distance (m).
    pub const FAR_CLIP_M: f32 = 160.0;

    /// Maximum camera pitch magnitude in degrees (`±85°` avoids zenith/nadir gimbal lock).
    pub const MAX_PITCH_DEG: f32 = 85.0;
    /// Maximum camera pitch magnitude in radians.
    pub const MAX_PITCH_RAD: f32 = MAX_PITCH_DEG * PI / 180.0;

    /// Lateral POV shoulder offset limit (m).
    pub const SIDE_OFFSET_MAX_M: f32 = super::human::CAMERA_SIDE_MAX;

    /// Base mouse yaw/pitch increment per raw device count (`5 × 0.022°` Source/Quake baseline).
    pub const MOUSE_DEG_PER_COUNT: f32 = 0.110;
    /// Base mouse increment in radians per raw count (`~0.00192 rad`).
    pub const MOUSE_RAD_PER_COUNT: f32 = MOUSE_DEG_PER_COUNT * PI / 180.0;
    /// Minimum mouse sensitivity multiplier.
    pub const SENSITIVITY_MIN: f32 = 0.20;
    /// Maximum mouse sensitivity multiplier.
    pub const SENSITIVITY_MAX: f32 = 3.00;
    /// Default mouse sensitivity multiplier.
    pub const SENSITIVITY_DEFAULT: f32 = 1.00;

    /// Converts a `Hor+` (`16:9` reference) horizontal FOV in degrees into vertical FOV in radians.
    #[inline]
    pub fn vfov_rad_from_hfov_16_9(hfov_deg: f32) -> f32 {
        let clamped = hfov_deg.clamp(HFOV_MIN_DEG, HFOV_MAX_DEG);
        2.0 * ((clamped.to_radians() * 0.5).tan() / REFERENCE_ASPECT).atan()
    }

    /// Computes actual horizontal FOV in degrees for an arbitrary viewport `aspect` under `Hor+`.
    #[inline]
    pub fn hfov_deg_for_aspect(hfov_16_9_deg: f32, aspect: f32) -> f32 {
        let vfov = vfov_rad_from_hfov_16_9(hfov_16_9_deg);
        (2.0 * ((vfov * 0.5).tan() * aspect.max(0.1)).atan()).to_degrees()
    }

    /// Equivalent 35mm full-frame (`36 mm` sensor width) focal length in millimeters.
    #[inline]
    pub fn focal_length_35mm(hfov_deg: f32) -> f32 {
        18.0 / (hfov_deg.clamp(10.0, 170.0).to_radians() * 0.5).tan()
    }
}

/// Real-world physical prop specifications (`1:1`).
pub mod props {
    /// ISO 15750-2 200 L / 55-gal steel drum outer radius (`585 mm` diameter, m).
    pub const BARREL_RADIUS: f32 = 0.2925;
    /// ISO 15750-2 200 L / 55-gal steel drum height (`880 mm`, m).
    pub const BARREL_HEIGHT: f32 = 0.880;
    /// Empty heavy-gauge steel drum mass (kg).
    pub const BARREL_MASS_KG: f32 = 20.0;
}

/// Converts millimeters to engine world units (meters).
#[inline]
pub const fn mm(millimeters: f32) -> f32 {
    millimeters * 0.001
}

/// Converts centimeters to engine world units (meters).
#[inline]
pub const fn cm(centimeters: f32) -> f32 {
    centimeters * 0.01
}

/// Converts meters to engine world units (`1:1`).
#[inline]
pub const fn m(meters: f32) -> f32 {
    meters * UNITS_PER_METER
}

/// Converts kilometers to engine world units (meters).
#[inline]
pub const fn km(kilometers: f32) -> f32 {
    kilometers * 1000.0
}

/// Converts inches to engine world units (meters).
#[inline]
pub const fn inches(inch: f32) -> f32 {
    inch * 0.0254
}

/// Converts feet to engine world units (meters).
#[inline]
pub const fn feet(ft: f32) -> f32 {
    ft * 0.3048
}

/// Converts engine world units (meters) to centimeters.
#[inline]
pub const fn to_cm(meters: f32) -> f32 {
    meters * METERS_PER_UNIT * 100.0
}

/// Converts engine world units (meters) to millimeters.
#[inline]
pub const fn to_mm(meters: f32) -> f32 {
    meters * METERS_PER_UNIT * 1000.0
}

/// Converts km/h to m/s.
#[inline]
pub const fn kmh_to_mps(kmh: f32) -> f32 {
    kmh / 3.6
}

/// Converts m/s to km/h.
#[inline]
pub const fn mps_to_kmh(mps: f32) -> f32 {
    mps * 3.6
}

/// Computes the ratio of a height in meters relative to `human::HEIGHT`.
#[inline]
pub const fn ratio_to_human(height_m: f32) -> f32 {
    height_m / human::HEIGHT
}

/// Axis-aligned bounding volume of a mesh in world meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RealWorldBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl RealWorldBounds {
    /// Computes bounds from a mesh's vertex positions, returning `None` if empty.
    pub fn from_mesh(mesh: &MeshData) -> Option<Self> {
        if mesh.vertices.is_empty() {
            return None;
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in &mesh.vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        Some(Self { min, max })
    }

    /// Full extent `(width, height, depth)` in meters.
    #[inline]
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    /// Geometric center in local mesh space (m).
    #[inline]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Vertical height along `+Y` (m).
    #[inline]
    pub fn height(self) -> f32 {
        self.size().y
    }

    /// Maximum horizontal half-extent in the XZ plane (m).
    #[inline]
    pub fn horizontal_radius(self) -> f32 {
        let s = self.size();
        s.x.max(s.z) * 0.5
    }

    /// Uniform scale factor required so that vertical height equals `target_height_m`.
    #[inline]
    pub fn scale_for_height(self, target_height_m: f32) -> f32 {
        let h = self.height();
        if h > 1e-6 {
            target_height_m / h
        } else {
            1.0
        }
    }

    /// Height ratio relative to a standing `1.80` m human.
    #[inline]
    pub fn human_height_ratio(self) -> f32 {
        ratio_to_human(self.height())
    }
}

/// Returns the uniform scale factor needed to match `mesh` to `target_height_m`.
pub fn mesh_scale_for_height(mesh: &MeshData, target_height_m: f32) -> f32 {
    RealWorldBounds::from_mesh(mesh)
        .map(|b| b.scale_for_height(target_height_m))
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh;

    #[test]
    fn coordinate_system_is_right_handed_y_up() {
        assert_eq!(coord::RIGHT.cross(coord::UP), coord::BACKWARD);
        assert_eq!(coord::UP.cross(coord::FORWARD), -coord::RIGHT);
        assert!((coord::forward_from_yaw_pitch(0.0, 0.0) - coord::FORWARD).length() < 1e-6);
        assert!((coord::right_from_yaw(0.0) - coord::RIGHT).length() < 1e-6);
        assert!(coord::yaw_from_dir(coord::FORWARD).abs() < 1e-6);
    }

    #[test]
    fn human_ergonomics_are_internally_consistent() {
        let total_capsule_h =
            std::hint::black_box(human::CAPSULE_HALF_SEGMENT) * 2.0 + human::RADIUS * 2.0;
        assert!((total_capsule_h - human::HEIGHT).abs() < 1e-5);
        let eye_from_floor = std::hint::black_box(human::CENTER_HEIGHT) + human::EYE_OFFSET;
        assert!(
            (eye_from_floor - human::EYE_HEIGHT).abs() < 1e-5,
            "eye offset must equal floor eye height minus capsule center"
        );
        const {
            assert!(human::CENTER_HEIGHT + human::EYE_OFFSET_MAX < human::HEIGHT);
            assert!(human::RADIUS * 2.0 < world::DOOR_WIDTH);
            assert!(human::HEIGHT < world::DOOR_HEIGHT);
        }
    }

    #[test]
    fn unit_conversions_and_mesh_bounds_match_real_world() {
        assert!((cm(180.0) - m(1.8)).abs() < 1e-6);
        assert!((mm(880.0) - props::BARREL_HEIGHT).abs() < 1e-6);
        assert!((inches(12.0) - feet(1.0)).abs() < 1e-5);
        assert!((to_cm(1.65) - 165.0).abs() < 1e-4);
        assert!((to_mm(0.25) - 250.0).abs() < 1e-3);
        assert!((kmh_to_mps(36.0) - 10.0).abs() < 1e-5);
        assert!((mps_to_kmh(10.0) - 36.0).abs() < 1e-4);

        let box_mesh = mesh::bevel_box(Vec3::new(0.5, 0.9, 0.5), 0.02);
        let bounds = RealWorldBounds::from_mesh(&box_mesh).unwrap();
        assert!((bounds.height() - 1.8).abs() < 1e-4);
        assert!((bounds.human_height_ratio() - 1.0).abs() < 1e-4);
        assert!((bounds.scale_for_height(0.9) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn camera_hor_plus_fov_scales_across_aspect_ratios() {
        let hfov_16_9 = camera::hfov_deg_for_aspect(90.0, 16.0 / 9.0);
        assert!((hfov_16_9 - 90.0).abs() < 1e-3);

        let vfov_deg = camera::vfov_rad_from_hfov_16_9(90.0).to_degrees();
        assert!((vfov_deg - 58.7155).abs() < 0.05);

        // Hor+: ultrawide 21:9 expands horizontal FOV while preserving vertical framing.
        let hfov_21_9 = camera::hfov_deg_for_aspect(90.0, 21.0 / 9.0);
        assert!(hfov_21_9 > 104.0 && hfov_21_9 < 108.0);

        // 35mm full-frame equivalent at 90 deg horizontal FOV is 18 mm.
        assert!((camera::focal_length_35mm(90.0) - 18.0).abs() < 0.05);
    }
}
