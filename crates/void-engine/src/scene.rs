//! 30x30m real-world-scaled (1:1 SI metric) test hall with architectural proportions, props, and point lights.
use crate::{
    audio::FootstepSurface,
    glb,
    mesh::{self, MeshData},
    physics::{self, Physics, Shape},
    render::{Draw, Material, MeshId, PointLight, Renderer, MAX_POINT_LIGHTS},
    textures as tex,
    units::{self, coord, human, props, world},
};
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::RigidBodyHandle;
use std::{collections::HashMap, path::Path};

pub const SPAWN: Vec3 = Vec3::new(0.0, human::SPAWN_Y, 10.5);
const TILE_METERS: f32 = coord::TILE_SIZE_M; // 4x4 grid texture -> 0.25m per grid cell.
const ROOM_HALF: f32 = world::ROOM_HALF;

pub const BARREL_RADIUS: f32 = props::BARREL_RADIUS;
pub const BARREL_HEIGHT: f32 = props::BARREL_HEIGHT;
pub const BARREL_TAG: u128 = 1;

#[derive(Clone, Copy)]
pub struct LightPreset {
    pub name: &'static str,
    pub radius: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

pub const LIGHT_PRESETS: [LightPreset; 6] = [
    LightPreset {
        name: "Янтарь",
        radius: 12.0,
        color: [1.0, 0.65, 0.30],
        intensity: 28.0,
    },
    LightPreset {
        name: "Синий",
        radius: 12.0,
        color: [0.42, 0.72, 1.0],
        intensity: 26.0,
    },
    LightPreset {
        name: "Белый",
        radius: 12.0,
        color: [1.0, 0.97, 0.92],
        intensity: 26.0,
    },
    LightPreset {
        name: "Коралл",
        radius: 10.0,
        color: [1.0, 0.38, 0.28],
        intensity: 24.0,
    },
    LightPreset {
        name: "Бирюза",
        radius: 12.0,
        color: [0.32, 0.95, 0.78],
        intensity: 24.0,
    },
    LightPreset {
        name: "Фиолет",
        radius: 11.0,
        color: [0.74, 0.50, 1.0],
        intensity: 28.0,
    },
];

#[derive(Clone)]
pub struct TestLight {
    pub pos: Vec3,
    pub default_pos: Vec3,
    pub radius: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImpactSurface {
    Concrete,
    Metal,
    Wood,
}

#[derive(Clone)]
pub struct Prop {
    pub body: RigidBodyHandle,
    pub mesh: MeshId,
    pub material: Material,
}

pub struct Scene {
    pub physics: Physics,
    pub player: RigidBodyHandle,
    pub statics: Vec<Draw>,
    pub props: Vec<Prop>,
    pub lights: Vec<TestLight>,
    surfaces: HashMap<RigidBodyHandle, FootstepSurface>,
    impact_surfaces: HashMap<RigidBodyHandle, ImpactSurface>,
    pub cube: MeshId,
    pub barrel_mesh: MeshId,
}

fn scaled_barrel_mesh(raw: &MeshData) -> MeshData {
    let scale = units::mesh_scale_for_height(raw, BARREL_HEIGHT);
    let mut scaled = MeshData::default();
    scaled.append(raw, Mat4::from_scale(Vec3::splat(scale)));
    scaled
}

impl Scene {
    pub fn new(r: &mut Renderer, assets: &Path) -> anyhow::Result<Self> {
        let cube = r.mesh(&mesh::bevel_box(Vec3::splat(0.5), 0.02));
        let barrel_model = glb::load(&assets.join("models/old_tin_barrel.glb"))?;
        let barrel_mesh = r.mesh(&scaled_barrel_mesh(&barrel_model.mesh));
        let mut physics = Physics::default();
        let player = physics.player(SPAWN);
        let mut scene = Self {
            physics,
            player,
            statics: Vec::new(),
            props: Vec::new(),
            lights: default_lights(),
            surfaces: HashMap::new(),
            impact_surfaces: HashMap::new(),
            cube,
            barrel_mesh,
        };
        scene.build(r);
        scene.spawn_default_props();
        scene.physics.refresh_queries();
        Ok(scene)
    }

    fn block(
        &mut self,
        r: &mut Renderer,
        size: Vec3,
        pos: Vec3,
        rot: Quat,
        material: Material,
        collider: bool,
    ) {
        let mesh = r.mesh(&mesh::bevel_box_tiled(size * 0.5, 0.02, TILE_METERS));
        self.statics.push(Draw::new(
            mesh,
            Mat4::from_rotation_translation(rot, pos),
            material,
        ));
        if collider {
            let body = self
                .physics
                .add(Shape::Box(size * 0.5), pos, rot, 0.0, 0.75, 0);
            let surface = if material.metallic >= 0.5 {
                FootstepSurface::MetalSteps
            } else {
                FootstepSurface::PavementTiles
            };
            self.surfaces.insert(body, surface);
            let impact = if material.metallic >= 0.5 {
                ImpactSurface::Metal
            } else if material.layer == tex::DEV_ORANGE || material.layer == tex::DEV_ORANGE_INV {
                ImpactSurface::Wood
            } else {
                ImpactSurface::Concrete
            };
            self.impact_surfaces.insert(body, impact);
        }
    }

    fn build(&mut self, r: &mut Renderer) {
        let id = Quat::IDENTITY;
        let floor_mat = Material::new(tex::DEV_GRAY_INV, [0.96, 0.96, 0.96], 1.05, 0.0);
        let wall_mat = Material::new(tex::DEV_GRAY_INV, [0.90, 0.90, 0.90], 1.15, 0.0);
        let orange = Material::new(tex::DEV_ORANGE, [1.0, 1.0, 1.0], 0.7, 0.0);
        let orange_inv = Material::new(tex::DEV_ORANGE_INV, [1.0, 1.0, 1.0], 0.75, 0.0);
        let gray_dark = Material::new(tex::DEV_GRAY, [1.0, 1.0, 1.0], 0.65, 0.0);
        let gray_light = Material::new(tex::DEV_GRAY_INV, [0.95, 0.95, 0.95], 0.8, 0.0);

        let wall_t = world::WALL_THICKNESS;
        let wall_h = world::WALL_HEIGHT;
        let slab_t = world::SLAB_THICKNESS;

        // 30x30m floor slab (top datum at y = 0.00 m).
        self.block(
            r,
            Vec3::new(ROOM_HALF * 2.0, slab_t, ROOM_HALF * 2.0),
            Vec3::new(0.0, coord::GROUND_Y - slab_t * 0.5, 0.0),
            id,
            floor_mat,
            true,
        );

        // Perimeter walls (4.50m tall, 0.25m thick; open top for directional sunlight & shadows).
        for z in [-ROOM_HALF - wall_t * 0.5, ROOM_HALF + wall_t * 0.5] {
            self.block(
                r,
                Vec3::new(ROOM_HALF * 2.0, wall_h, wall_t),
                Vec3::new(0.0, wall_h * 0.5, z),
                id,
                wall_mat,
                true,
            );
        }
        for x in [-ROOM_HALF - wall_t * 0.5, ROOM_HALF + wall_t * 0.5] {
            self.block(
                r,
                Vec3::new(wall_t, wall_h, ROOM_HALF * 2.0 + wall_t * 2.0),
                Vec3::new(x, wall_h * 0.5, 0.0),
                id,
                wall_mat,
                true,
            );
        }

        // Center hero blocks: 1.20m barrier + 0.60m crate = 1.80m (1:1 human height), 0.90m counter, 0.45m bench.
        self.block(
            r,
            Vec3::splat(world::BARRIER_HEIGHT),
            Vec3::new(-1.2, world::BARRIER_HEIGHT * 0.5, -1.5),
            id,
            orange,
            true,
        );
        self.block(
            r,
            Vec3::splat(0.60),
            Vec3::new(-1.2, world::BARRIER_HEIGHT + 0.30, -1.5),
            Quat::from_rotation_y(0.35),
            orange_inv,
            true,
        );
        self.block(
            r,
            Vec3::splat(world::COUNTER_HEIGHT),
            Vec3::new(1.3, world::COUNTER_HEIGHT * 0.5, -1.2),
            Quat::from_rotation_y(0.55),
            gray_light,
            true,
        );
        self.block(
            r,
            Vec3::splat(world::BENCH_HEIGHT),
            Vec3::new(0.1, world::BENCH_HEIGHT * 0.5, 0.9),
            id,
            orange,
            true,
        );

        // West zone: 4.20m architectural column + 2.10m door-height monolith + 2.40m clear canopy arch.
        self.block(
            r,
            Vec3::new(0.80, 4.20, 0.80),
            Vec3::new(-7.0, 2.10, -4.0),
            id,
            orange,
            true,
        );
        self.block(
            r,
            Vec3::new(0.70, world::DOOR_HEIGHT, 0.70),
            Vec3::new(-6.8, world::DOOR_HEIGHT * 0.5, 0.2),
            Quat::from_rotation_y(-0.3),
            gray_dark,
            true,
        );
        let canopy_clear = 2.40;
        for z in [3.2, 6.0] {
            for x in [-8.2, -5.8] {
                self.block(
                    r,
                    Vec3::new(0.30, canopy_clear, 0.30),
                    Vec3::new(x, canopy_clear * 0.5, z),
                    id,
                    gray_light,
                    true,
                );
            }
        }
        self.block(
            r,
            Vec3::new(2.90, wall_t, 3.30),
            Vec3::new(-7.0, canopy_clear + wall_t * 0.5, 4.6),
            id,
            orange_inv,
            true,
        );

        // North-West shaded alcove: 5.0x5.0m workshop booth, 2.80m clear ceiling, 2.20m wide entrance.
        let ceil = world::CEILING_CLEARANCE;
        self.block(
            r,
            Vec3::new(5.0 + wall_t, wall_t, 5.0 + wall_t),
            Vec3::new(
                -12.5 + wall_t * 0.5,
                ceil + wall_t * 0.5,
                -12.5 + wall_t * 0.5,
            ),
            id,
            gray_dark,
            true,
        );
        self.block(
            r,
            Vec3::new(wall_t, ceil, 5.0),
            Vec3::new(-10.0 + wall_t * 0.5, ceil * 0.5, -12.5),
            id,
            gray_dark,
            true,
        );
        let south_len = 5.0 - world::GATE_WIDTH;
        self.block(
            r,
            Vec3::new(south_len, ceil, wall_t),
            Vec3::new(-15.0 + south_len * 0.5, ceil * 0.5, -10.0 + wall_t * 0.5),
            id,
            gray_dark,
            true,
        );
        self.block(
            r,
            Vec3::new(0.80, world::COUNTER_HEIGHT, 0.80),
            Vec3::new(-13.2, world::COUNTER_HEIGHT * 0.5, -13.0),
            Quat::from_rotation_y(0.4),
            orange,
            true,
        );

        // East zone: PBR roughness & metallic calibration row on a 0.45m bench plinth.
        self.block(
            r,
            Vec3::new(1.20, world::BENCH_HEIGHT, 7.50),
            Vec3::new(7.5, world::BENCH_HEIGHT * 0.5, -0.5),
            id,
            gray_dark,
            true,
        );
        let calibration = [
            Material::new(tex::DEV_GRAY_INV, [0.95; 3], 1.25, 0.0),
            Material::new(tex::DEV_ORANGE, [1.0; 3], 0.65, 0.0),
            Material::new(tex::DEV_GRAY, [1.0; 3], 0.25, 0.0),
            Material::new(tex::WHITE, [0.82, 0.84, 0.88], 0.52, 0.92),
            Material::new(tex::WHITE, [0.94, 0.95, 0.97], 0.20, 1.0),
        ];
        let sample_size = 0.60;
        for (i, mat) in calibration.into_iter().enumerate() {
            let z = -3.5 + i as f32 * 1.5;
            self.block(
                r,
                Vec3::splat(sample_size),
                Vec3::new(7.5, world::BENCH_HEIGHT + sample_size * 0.5, z),
                Quat::from_rotation_y(0.25 * i as f32),
                mat,
                true,
            );
        }

        // North zone: ergonomic real-world height gauge (riser 0.20, bench 0.45, counter 0.90, barrier 1.20, human 1.80).
        for (i, (h, mat)) in [
            (world::STAIR_RISER, orange),
            (world::BENCH_HEIGHT, orange_inv),
            (world::COUNTER_HEIGHT, gray_light),
            (world::BARRIER_HEIGHT, gray_dark),
            (human::HEIGHT, orange),
        ]
        .into_iter()
        .enumerate()
        {
            self.block(
                r,
                Vec3::new(1.0, h, 1.0),
                Vec3::new(-2.0 + i as f32 * 1.0, h * 0.5, -8.5),
                id,
                mat,
                true,
            );
        }
    }

    fn spawn_default_props(&mut self) {
        let y = BARREL_HEIGHT * 0.5 + 0.01;
        self.spawn_barrel(Vec3::new(1.05, y, 1.65), Quat::from_rotation_y(0.45));
        self.spawn_barrel(Vec3::new(-0.45, y, 2.15), Quat::from_rotation_y(-1.1));
        self.spawn_barrel(Vec3::new(-11.8, y, -12.6), Quat::from_rotation_y(0.8));
    }

    pub fn spawn_barrel(&mut self, pos: Vec3, rot: Quat) -> usize {
        let body = self.physics.add(
            Shape::Cylinder(BARREL_RADIUS, BARREL_HEIGHT),
            pos,
            rot,
            props::BARREL_MASS_KG,
            0.65,
            BARREL_TAG,
        );
        self.physics.refresh_queries();
        self.surfaces.insert(body, FootstepSurface::MetalSteps);
        self.impact_surfaces.insert(body, ImpactSurface::Metal);
        let idx = self.props.len();
        self.props.push(Prop {
            body,
            mesh: self.barrel_mesh,
            material: Material::new(tex::BARREL, [1.0, 1.0, 1.0], 1.15, 0.35),
        });
        idx
    }

    pub fn spawn_barrel_in_front(&mut self, origin: Vec3, dir: Vec3) -> usize {
        let dir = dir.normalize_or_zero();
        let desired_dist = 2.0;
        let mut p = match self.physics.raycast(
            origin,
            dir,
            desired_dist + BARREL_RADIUS,
            self.player,
            false,
        ) {
            Some((_, hit_dist)) => {
                let d = (hit_dist - BARREL_RADIUS - 0.08).clamp(0.75, desired_dist);
                origin + dir * d
            }
            None => origin + dir * desired_dist,
        };
        let min_y = BARREL_HEIGHT * 0.5 + 0.02;
        p.x = p.x.clamp(-ROOM_HALF + 0.6, ROOM_HALF - 0.6);
        p.y = p.y.clamp(min_y, 8.0);
        p.z = p.z.clamp(-ROOM_HALF + 0.6, ROOM_HALF - 0.6);
        let yaw = coord::yaw_from_dir(dir);
        self.spawn_barrel(p, Quat::from_rotation_y(yaw))
    }

    pub fn remove_prop(&mut self, idx: usize) -> bool {
        if idx >= self.props.len() {
            return false;
        }
        let prop = self.props.swap_remove(idx);
        self.surfaces.remove(&prop.body);
        self.impact_surfaces.remove(&prop.body);
        self.physics.remove(prop.body);
        self.physics.refresh_queries();
        true
    }

    pub fn set_surface(&mut self, body: RigidBodyHandle, surface: FootstepSurface) {
        self.surfaces.insert(body, surface);
    }

    pub fn surface_of(&self, body: RigidBodyHandle) -> FootstepSurface {
        self.surfaces
            .get(&body)
            .copied()
            .unwrap_or(FootstepSurface::PavementTiles)
    }

    pub fn impact_surface_of(&self, body: RigidBodyHandle) -> ImpactSurface {
        self.impact_surfaces
            .get(&body)
            .copied()
            .unwrap_or(ImpactSurface::Concrete)
    }

    pub fn prop_index_for_body(&self, body: RigidBodyHandle) -> Option<usize> {
        self.props.iter().position(|p| p.body == body)
    }

    pub fn pick_prop(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(usize, f32)> {
        let (body, dist) = self
            .physics
            .raycast(origin, dir, max_dist, self.player, false)?;
        self.prop_index_for_body(body).map(|idx| (idx, dist))
    }

    pub fn pick_light(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(usize, f32)> {
        let dir = dir.normalize_or_zero();
        let pick_radius = 0.40f32;
        let mut best: Option<(usize, f32)> = None;
        for (i, light) in self.lights.iter().enumerate() {
            let oc = light.pos - origin;
            let t = oc.dot(dir);
            if t < 0.3 || t > max_dist {
                continue;
            }
            let closest = origin + dir * t;
            if closest.distance(light.pos) > pick_radius {
                continue;
            }
            if let Some((_, hit_dist)) = self.physics.raycast(origin, dir, t, self.player, false) {
                if hit_dist < t - 0.15 {
                    continue;
                }
            }
            if best.is_none_or(|(_, d)| t < d) {
                best = Some((i, t));
            }
        }
        best
    }

    pub fn clamp_light_target(&self, origin: Vec3, dir: Vec3, dist: f32) -> Vec3 {
        let dir = dir.normalize_or_zero();
        let clamped_dist = match self
            .physics
            .raycast(origin, dir, dist + 0.25, self.player, false)
        {
            Some((_, hit)) => (hit - 0.25).max(0.6),
            None => dist,
        };
        let p = origin + dir * clamped_dist;
        Vec3::new(
            p.x.clamp(-ROOM_HALF + 0.35, ROOM_HALF - 0.35),
            p.y.clamp(0.30, 8.0),
            p.z.clamp(-ROOM_HALF + 0.35, ROOM_HALF - 0.35),
        )
    }

    pub fn place_light_pos(&self, origin: Vec3, dir: Vec3) -> Vec3 {
        let dir = dir.normalize_or_zero();
        let dist = match self.physics.raycast(origin, dir, 5.0, self.player, false) {
            Some((_, hit)) => (hit - 0.35).clamp(1.0, 4.5),
            None => 2.5,
        };
        self.clamp_light_target(origin, dir, dist)
    }

    pub fn add_light(&mut self, pos: Vec3, preset_idx: usize) -> Option<usize> {
        if self.lights.len() >= MAX_POINT_LIGHTS {
            return None;
        }
        let preset = LIGHT_PRESETS[preset_idx % LIGHT_PRESETS.len()];
        let idx = self.lights.len();
        self.lights.push(TestLight {
            pos,
            default_pos: pos,
            radius: preset.radius,
            color: preset.color,
            intensity: preset.intensity,
        });
        Some(idx)
    }

    pub fn remove_light(&mut self, idx: usize) -> bool {
        if idx >= self.lights.len() {
            return false;
        }
        self.lights.remove(idx);
        true
    }

    pub fn reset(&mut self) {
        let b = &mut self.physics.bodies[self.player];
        b.set_translation(physics::vector(SPAWN), true);
        b.set_linvel(physics::vector(Vec3::ZERO), true);
        self.lights = default_lights();
        for prop in self.props.drain(..) {
            self.surfaces.remove(&prop.body);
            self.impact_surfaces.remove(&prop.body);
            self.physics.remove(prop.body);
        }
        self.spawn_default_props();
        self.physics.refresh_queries();
    }

    pub fn render_lights(&self) -> Vec<PointLight> {
        self.lights
            .iter()
            .map(|l| PointLight::new(l.pos, l.radius, l.color, l.intensity))
            .collect()
    }

    pub fn draws(&self, hovered: Option<usize>, held: Option<usize>) -> Vec<Draw> {
        let mut out = self.statics.clone();
        for prop in &self.props {
            let iso = self.physics.bodies[prop.body].position();
            let transform = Mat4::from_rotation_translation(
                physics::quaternion(&iso.rotation),
                physics::vec3(&iso.translation.vector),
            );
            out.push(Draw::new(prop.mesh, transform, prop.material));
        }
        for (i, l) in self.lights.iter().enumerate() {
            let active = held == Some(i) || hovered == Some(i);
            let scale = if held == Some(i) {
                world::LIGHT_NODE_SIZE + 0.05
            } else if hovered == Some(i) {
                world::LIGHT_NODE_SIZE + 0.03
            } else {
                world::LIGHT_NODE_SIZE
            };
            let glow = if active { 5.0 } else { 3.2 };
            out.push(Draw {
                mesh: self.cube,
                transform: Mat4::from_scale_rotation_translation(
                    Vec3::splat(scale),
                    Quat::IDENTITY,
                    l.pos,
                ),
                material: Material::glow(l.color, glow),
                shadow: false,
                viewmodel: false,
            });
        }
        out
    }
}

fn default_lights() -> Vec<TestLight> {
    let positions = [
        Vec3::new(3.6, 2.2, 1.8),
        Vec3::new(-3.8, 2.2, 2.2),
        Vec3::new(5.6, 2.1, -1.0),
        Vec3::new(-7.0, 1.95, 4.6),
        Vec3::new(0.0, 2.2, -6.5),
        Vec3::new(-12.0, 2.15, -12.2),
    ];
    positions
        .into_iter()
        .zip(LIGHT_PRESETS)
        .map(|(pos, preset)| TestLight {
            pos,
            default_pos: pos,
            radius: preset.radius,
            color: preset.color,
            intensity: preset.intensity,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::RealWorldBounds;

    #[test]
    fn loads_old_tin_barrel_glb_and_scales_to_real_world_drum() {
        let model = glb::load(&crate::asset_dir().join("models/old_tin_barrel.glb"))
            .expect("barrel GLB should load");
        assert!(!model.mesh.vertices.is_empty());
        assert!(!model.mesh.indices.is_empty());

        let raw_bounds = RealWorldBounds::from_mesh(&model.mesh).unwrap();
        assert!((raw_bounds.height() - 0.863).abs() < 0.02);

        let scaled = scaled_barrel_mesh(&model.mesh);
        let bounds = RealWorldBounds::from_mesh(&scaled).unwrap();
        assert!((bounds.height() - BARREL_HEIGHT).abs() < 0.002);
        assert!((bounds.horizontal_radius() - BARREL_RADIUS).abs() < 0.005);
    }

    #[test]
    fn dynamic_lights_can_be_added_up_to_cap_and_removed() {
        let mut lights = default_lights();
        assert_eq!(lights.len(), 6);
        while lights.len() < MAX_POINT_LIGHTS {
            let p = LIGHT_PRESETS[lights.len() % LIGHT_PRESETS.len()];
            lights.push(TestLight {
                pos: Vec3::Y * 2.0,
                default_pos: Vec3::Y * 2.0,
                radius: p.radius,
                color: p.color,
                intensity: p.intensity,
            });
        }
        assert_eq!(lights.len(), MAX_POINT_LIGHTS);
        lights.remove(0);
        assert_eq!(lights.len(), MAX_POINT_LIGHTS - 1);
    }
}
