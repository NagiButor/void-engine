//! Expanded 40x40m dev-textured test room with shaded alcove, calibration cubes, and draggable point lights.
use crate::{
    mesh,
    physics::{self, Physics, Shape},
    render::{Draw, Material, MeshId, PointLight, Renderer},
    textures as tex,
};
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::RigidBodyHandle;

pub const SPAWN: Vec3 = Vec3::new(0.0, 0.81, 13.0);
const TILE_METERS: f32 = 1.0; // 4x4 grid texture -> 0.25m per grid cell.
const ROOM_HALF: f32 = 20.0;

#[derive(Clone)]
pub struct TestLight {
    pub pos: Vec3,
    pub default_pos: Vec3,
    pub radius: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

pub struct Scene {
    pub physics: Physics,
    pub player: RigidBodyHandle,
    pub statics: Vec<Draw>,
    pub lights: Vec<TestLight>,
    pub cube: MeshId,
}

impl Scene {
    pub fn new(r: &mut Renderer) -> anyhow::Result<Self> {
        let cube = r.mesh(&mesh::bevel_box(Vec3::splat(0.5), 0.02));
        let mut physics = Physics::default();
        let player = physics.player(SPAWN);
        let mut scene = Self {
            physics,
            player,
            statics: Vec::new(),
            lights: default_lights(),
            cube,
        };
        scene.build(r);
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
            self.physics
                .add(Shape::Box(size * 0.5), pos, rot, 0.0, 0.75, 0);
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

        // 40x40m white-tiled floor (top at y = 0.0).
        self.block(
            r,
            Vec3::new(ROOM_HALF * 2.0, 0.5, ROOM_HALF * 2.0),
            Vec3::new(0.0, -0.25, 0.0),
            id,
            floor_mat,
            true,
        );

        // Perimeter walls (6m tall, open top for directional sunlight & shadows).
        for z in [-ROOM_HALF - 0.25, ROOM_HALF + 0.25] {
            self.block(
                r,
                Vec3::new(ROOM_HALF * 2.0, 6.0, 0.5),
                Vec3::new(0.0, 3.0, z),
                id,
                wall_mat,
                true,
            );
        }
        for x in [-ROOM_HALF - 0.25, ROOM_HALF + 0.25] {
            self.block(
                r,
                Vec3::new(0.5, 6.0, ROOM_HALF * 2.0 + 1.0),
                Vec3::new(x, 3.0, 0.0),
                id,
                wall_mat,
                true,
            );
        }

        // Center hero cubes for shadow, SSAO, and multi-light testing.
        self.block(
            r,
            Vec3::splat(2.0),
            Vec3::new(-1.5, 1.0, -2.0),
            id,
            orange,
            true,
        );
        self.block(
            r,
            Vec3::splat(1.0),
            Vec3::new(-1.5, 2.5, -2.0),
            Quat::from_rotation_y(0.35),
            orange_inv,
            true,
        );
        self.block(
            r,
            Vec3::splat(1.5),
            Vec3::new(1.5, 0.75, -1.5),
            Quat::from_rotation_y(0.55),
            gray_light,
            true,
        );
        self.block(
            r,
            Vec3::splat(0.5),
            Vec3::new(0.0, 0.25, 1.0),
            id,
            orange,
            true,
        );

        // West zone: tall shadow tower + dark monolith + open overhang arch.
        self.block(
            r,
            Vec3::new(1.0, 5.0, 1.0),
            Vec3::new(-9.0, 2.5, -5.0),
            id,
            orange,
            true,
        );
        self.block(
            r,
            Vec3::new(1.0, 2.5, 1.0),
            Vec3::new(-8.5, 1.25, 0.5),
            Quat::from_rotation_y(-0.3),
            gray_dark,
            true,
        );
        for z in [4.0, 7.5] {
            for x in [-10.5, -7.5] {
                self.block(
                    r,
                    Vec3::new(0.5, 2.5, 0.5),
                    Vec3::new(x, 1.25, z),
                    id,
                    gray_light,
                    true,
                );
            }
        }
        self.block(
            r,
            Vec3::new(3.5, 0.5, 4.0),
            Vec3::new(-9.0, 2.75, 5.75),
            id,
            orange_inv,
            true,
        );

        // North-West shaded alcove (covered booth to test point lights out of direct sun).
        self.block(
            r,
            Vec3::new(7.0, 0.5, 7.0),
            Vec3::new(-16.5, 3.75, -16.5),
            id,
            gray_dark,
            true,
        );
        self.block(
            r,
            Vec3::new(0.5, 3.5, 7.0),
            Vec3::new(-13.25, 1.75, -16.5),
            id,
            gray_dark,
            true,
        );
        self.block(
            r,
            Vec3::new(3.5, 3.5, 0.5),
            Vec3::new(-18.25, 1.75, -13.25),
            id,
            gray_dark,
            true,
        );
        self.block(
            r,
            Vec3::splat(1.0),
            Vec3::new(-17.0, 0.5, -17.0),
            Quat::from_rotation_y(0.4),
            orange,
            true,
        );

        // East zone: PBR roughness & metallic calibration row on a plinth.
        self.block(
            r,
            Vec3::new(2.0, 0.5, 11.0),
            Vec3::new(9.5, 0.25, -0.5),
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
        for (i, mat) in calibration.into_iter().enumerate() {
            let z = -4.5 + i as f32 * 2.0;
            self.block(
                r,
                Vec3::splat(1.0),
                Vec3::new(9.5, 1.0, z),
                Quat::from_rotation_y(0.25 * i as f32),
                mat,
                true,
            );
        }

        // North zone: 4 stepped cubes for elevation & step-shadow testing.
        for (i, (h, mat)) in [
            (0.5, orange),
            (1.0, orange_inv),
            (1.5, gray_light),
            (2.0, gray_dark),
        ]
        .into_iter()
        .enumerate()
        {
            self.block(
                r,
                Vec3::new(1.5, h, 1.5),
                Vec3::new(-2.25 + i as f32 * 1.5, h * 0.5, -11.0),
                id,
                mat,
                true,
            );
        }
    }

    pub fn pick_light(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(usize, f32)> {
        let dir = dir.normalize_or_zero();
        let pick_radius = 0.45f32;
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
            p.y.clamp(0.35, 12.0),
            p.z.clamp(-ROOM_HALF + 0.35, ROOM_HALF - 0.35),
        )
    }

    pub fn reset(&mut self) {
        let b = &mut self.physics.bodies[self.player];
        b.set_translation(physics::vector(SPAWN), true);
        b.set_linvel(physics::vector(Vec3::ZERO), true);
        for light in &mut self.lights {
            light.pos = light.default_pos;
        }
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
        for (i, l) in self.lights.iter().enumerate() {
            let active = held == Some(i) || hovered == Some(i);
            let scale = if held == Some(i) {
                0.28
            } else if hovered == Some(i) {
                0.26
            } else {
                0.22
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
            });
        }
        out
    }
}

fn default_lights() -> Vec<TestLight> {
    let entries = [
        (Vec3::new(4.2, 2.5, 2.0), 14.0, [1.0, 0.65, 0.30], 34.0),
        (Vec3::new(-4.5, 2.5, 2.5), 14.0, [0.42, 0.72, 1.0], 32.0),
        (Vec3::new(7.2, 2.6, -1.5), 14.0, [1.0, 0.97, 0.92], 32.0),
        (Vec3::new(-9.0, 2.1, 5.75), 12.0, [1.0, 0.38, 0.28], 28.0),
        (Vec3::new(0.0, 2.6, -8.5), 14.0, [0.32, 0.95, 0.78], 28.0),
        (Vec3::new(-15.5, 2.4, -15.5), 14.0, [0.74, 0.50, 1.0], 34.0),
    ];
    entries
        .into_iter()
        .map(|(pos, radius, color, intensity)| TestLight {
            pos,
            default_pos: pos,
            radius,
            color,
            intensity,
        })
        .collect()
}
