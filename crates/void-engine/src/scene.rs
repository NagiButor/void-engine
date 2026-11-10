//! Physics Lab: reusable detailed prop assemblies and a metre-scale industrial hall.
use crate::{
    glb,
    mesh::{self, MeshData},
    physics::{self, Physics, Shape},
    render::{Draw, Material, MeshId, Renderer},
    textures as tex,
};
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::RigidBodyHandle;
pub type BodyHandle = RigidBodyHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Crate,
    Barrel,
    Sphere,
    Plank,
    Brick,
    Can,
    Domino,
    Cone,
    Pallet,
    Balance,
}
impl PropKind {
    pub const ALL: [Self; 10] = [
        Self::Crate,
        Self::Barrel,
        Self::Sphere,
        Self::Plank,
        Self::Brick,
        Self::Can,
        Self::Domino,
        Self::Cone,
        Self::Pallet,
        Self::Balance,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Crate => "Ящик",
            Self::Barrel => "Бочка",
            Self::Sphere => "Стальной шар",
            Self::Plank => "Доска",
            Self::Brick => "Кирпич",
            Self::Can => "Канистра",
            Self::Domino => "Домино",
            Self::Cone => "Конус",
            Self::Pallet => "Паллета",
            Self::Balance => "Балансир",
        }
    }
    pub fn sound(self) -> &'static str {
        match self {
            Self::Barrel | Self::Sphere | Self::Can => "impact_metal",
            Self::Brick => "impact_concrete",
            Self::Cone | Self::Domino => "impact_plastic",
            _ => "impact_wood",
        }
    }
    pub fn mass(self) -> f32 {
        match self {
            Self::Crate => 13.0,
            Self::Barrel => 22.0,
            Self::Sphere => 18.0,
            Self::Plank => 7.0,
            Self::Brick => 2.8,
            Self::Can => 4.0,
            Self::Domino => 1.1,
            Self::Cone => 2.5,
            Self::Pallet => 19.0,
            Self::Balance => 15.0,
        }
    }
    pub fn shape(self) -> Shape {
        match self {
            Self::Crate => Shape::Box(Vec3::splat(0.45)),
            Self::Barrel => Shape::Cylinder(0.30, 0.88),
            Self::Sphere => Shape::Sphere(0.28),
            Self::Plank => Shape::Box(Vec3::new(1.1, 0.055, 0.17)),
            Self::Brick => Shape::Box(Vec3::new(0.24, 0.11, 0.115)),
            Self::Can => Shape::Box(Vec3::new(0.17, 0.23, 0.105)),
            Self::Domino => Shape::Box(Vec3::new(0.16, 0.32, 0.065)),
            Self::Cone => Shape::Cone(0.24, 0.65),
            Self::Pallet => Shape::Compound(pallet_parts()),
            Self::Balance => Shape::Box(Vec3::new(2.1, 0.07, 0.30)),
        }
    }
}

pub struct Prop {
    pub kind: PropKind,
    pub body: RigidBodyHandle,
    pub spawn: Vec3,
    pub rotation: Quat,
    pub initial: bool,
    pub tint: f32,
}
#[derive(Clone)]
pub struct Part {
    pub mesh: MeshId,
    pub material: Material,
}
struct Assembly {
    groups: Vec<(Material, MeshData)>,
}
impl Assembly {
    fn new() -> Self {
        Self { groups: Vec::new() }
    }
    fn add(&mut self, mesh: &MeshData, transform: Mat4, material: Material) {
        let index = self.groups.iter().position(|(m, _)| {
            m.layer == material.layer
                && m.color == material.color
                && m.metallic == material.metallic
                && m.emissive == material.emissive
        });
        if let Some(i) = index {
            self.groups[i].1.append(mesh, transform);
        } else {
            let mut out = MeshData::default();
            out.append(mesh, transform);
            self.groups.push((material, out));
        }
    }
    fn block(&mut self, half: Vec3, p: Vec3, material: Material) {
        self.add(
            &mesh::bevel_box(half, 0.008),
            Mat4::from_translation(p),
            material,
        );
    }
    fn finish(self, r: &mut Renderer) -> Vec<Part> {
        self.groups
            .into_iter()
            .map(|(material, m)| Part {
                mesh: r.mesh(&m),
                material,
            })
            .collect()
    }
}
fn wood() -> Material {
    Material::new(tex::WOOD, [0.80, 0.72, 0.58], 0.84, 0.0)
}
fn steel() -> Material {
    Material::new(tex::METAL, [0.29, 0.33, 0.37], 0.42, 0.88)
}
fn black() -> Material {
    Material::new(tex::RUBBER, [0.32, 0.34, 0.36], 0.85, 0.0)
}
fn paint(c: [f32; 3]) -> Material {
    Material::new(tex::PAINT, c, 0.48, 0.28)
}

fn pallet_parts() -> Vec<(Vec3, Vec3)> {
    let mut p = Vec::new();
    for x in [-0.48, 0.0, 0.48] {
        p.push((Vec3::new(x, -0.025, 0.0), Vec3::new(0.065, 0.065, 0.49)));
    }
    for z in [-0.43, -0.215, 0.0, 0.215, 0.43] {
        p.push((Vec3::new(0.0, 0.065, z), Vec3::new(0.60, 0.025, 0.08)));
    }
    p
}

fn build_prop(r: &mut Renderer, kind: PropKind) -> Vec<Part> {
    let mut a = Assembly::new();
    match kind {
        PropKind::Crate => {
            let mut frame = wood();
            frame.color = [0.43, 0.31, 0.19, 1.0];
            for rotation in [
                Quat::IDENTITY,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ] {
                for side in [-1.0, 1.0] {
                    let transform = Mat4::from_quat(rotation);
                    for i in 0..5 {
                        let x = -0.328 + i as f32 * 0.164;
                        a.add(
                            &mesh::bevel_box(Vec3::new(0.079, 0.382, 0.025), 0.004),
                            transform * Mat4::from_translation(Vec3::new(x, 0.0, side * 0.409)),
                            wood(),
                        );
                    }
                    for x in [-0.397, 0.397] {
                        a.add(
                            &mesh::bevel_box(Vec3::new(0.050, 0.45, 0.04), 0.007),
                            transform * Mat4::from_translation(Vec3::new(x, 0.0, side * 0.410)),
                            frame,
                        );
                    }
                    for y in [-0.397, 0.397] {
                        a.add(
                            &mesh::bevel_box(Vec3::new(0.35, 0.05, 0.04), 0.007),
                            transform * Mat4::from_translation(Vec3::new(0.0, y, side * 0.410)),
                            frame,
                        );
                    }
                    for x in [-0.40, 0.40] {
                        for y in [-0.39, 0.39] {
                            a.add(
                                &mesh::cylinder(0.009, 0.006, 8),
                                transform
                                    * Mat4::from_translation(Vec3::new(x, y, side * 0.453))
                                    * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
                                steel(),
                            );
                        }
                    }
                }
            }
            // Actual steel edge protectors and a small shipping label.
            for x in [-0.45, 0.45] {
                for z in [-0.45, 0.45] {
                    a.block(
                        Vec3::new(0.018, 0.092, 0.018),
                        Vec3::new(x, 0.35, z),
                        steel(),
                    );
                }
            }
            a.block(
                Vec3::new(0.11, 0.067, 0.002),
                Vec3::new(-0.15, 0.10, 0.437),
                Material::new(0, [0.74, 0.70, 0.58], 0.94, 0.0),
            );
            for i in 0..12 {
                a.block(
                    Vec3::new(0.0015 + (i % 3) as f32 * 0.001, 0.035, 0.001),
                    Vec3::new(-0.23 + i as f32 * 0.013, 0.10, 0.440),
                    black(),
                );
            }
        }
        PropKind::Barrel => {
            let profile = [
                (-0.44, 0.275),
                (-0.43, 0.299),
                (-0.405, 0.30),
                (-0.39, 0.283),
                (-0.26, 0.283),
                (-0.245, 0.299),
                (-0.22, 0.299),
                (-0.205, 0.284),
                (0.205, 0.284),
                (0.22, 0.299),
                (0.245, 0.299),
                (0.26, 0.283),
                (0.39, 0.283),
                (0.405, 0.30),
                (0.43, 0.299),
                (0.44, 0.275),
            ];
            a.add(
                &mesh::lathe(&profile, 64, true),
                Mat4::IDENTITY,
                paint([0.12, 0.34, 0.39]),
            );
            for y in [-0.423, -0.234, 0.234, 0.423] {
                a.add(
                    &mesh::cylinder(0.302, 0.018, 64),
                    Mat4::from_translation(Vec3::Y * y),
                    steel(),
                );
            }
            a.add(
                &mesh::cylinder(0.039, 0.009, 20),
                Mat4::from_translation(Vec3::new(0.14, 0.446, 0.07)),
                steel(),
            );
            a.add(
                &mesh::cylinder(0.017, 0.012, 12),
                Mat4::from_translation(Vec3::new(-0.12, 0.447, -0.06)),
                steel(),
            );
            a.add(
                &mesh::cylinder(0.285, 0.065, 64),
                Mat4::from_translation(Vec3::new(0.0, 0.13, 0.0)),
                paint([0.72, 0.56, 0.19]),
            );
        }
        PropKind::Sphere => {
            a.add(
                &mesh::sphere(0.28, 48, 32),
                Mat4::IDENTITY,
                Material::new(tex::METAL, [0.66, 0.70, 0.73], 0.34, 0.97),
            );
        }
        PropKind::Plank | PropKind::Balance => {
            let half = if kind == PropKind::Plank {
                Vec3::new(1.1, 0.055, 0.17)
            } else {
                Vec3::new(2.1, 0.07, 0.3)
            };
            a.block(half, Vec3::ZERO, wood());
        }
        PropKind::Brick => {
            a.block(
                Vec3::new(0.24, 0.11, 0.115),
                Vec3::ZERO,
                Material::new(tex::BRICK, [0.83, 0.63, 0.52], 0.96, 0.0),
            );
            for x in [-0.14, 0.0, 0.14] {
                a.add(
                    &mesh::cylinder(0.029, 0.002, 16),
                    Mat4::from_translation(Vec3::new(x, 0.111, 0.0)),
                    Material::new(tex::BRICK, [0.20, 0.16, 0.12], 1.0, 0.0),
                );
            }
        }
        PropKind::Can => {
            a.block(
                Vec3::new(0.17, 0.20, 0.105),
                Vec3::new(0.0, -0.03, 0.0),
                paint([0.40, 0.44, 0.22]),
            );
            a.block(
                Vec3::new(0.125, 0.026, 0.025),
                Vec3::new(0.0, 0.205, 0.0),
                paint([0.34, 0.38, 0.18]),
            );
            for x in [-0.105, 0.105] {
                a.block(
                    Vec3::new(0.02, 0.053, 0.025),
                    Vec3::new(x, 0.17, 0.0),
                    paint([0.34, 0.38, 0.18]),
                );
            }
            for z in [-0.108, 0.108] {
                for angle in [-0.62, 0.62] {
                    a.add(
                        &mesh::bevel_box(Vec3::new(0.125, 0.009, 0.007), 0.005),
                        Mat4::from_rotation_translation(
                            Quat::from_rotation_z(angle),
                            Vec3::new(0.0, -0.02, z),
                        ),
                        paint([0.33, 0.37, 0.18]),
                    );
                }
            }
            a.add(
                &mesh::cylinder(0.035, 0.026, 24),
                Mat4::from_translation(Vec3::new(-0.10, 0.185, 0.065)),
                black(),
            );
        }
        PropKind::Domino => {
            a.block(
                Vec3::new(0.16, 0.32, 0.065),
                Vec3::ZERO,
                Material::new(0, [0.80, 0.78, 0.67], 0.42, 0.0),
            );
            a.block(
                Vec3::new(0.143, 0.003, 0.002),
                Vec3::new(0.0, 0.0, 0.066),
                black(),
            );
            for z in [-0.066, 0.066] {
                for (x, y) in [
                    (-0.075, 0.23),
                    (0.075, 0.23),
                    (0.0, 0.15),
                    (-0.075, 0.07),
                    (0.075, 0.07),
                    (0.0, -0.16),
                ] {
                    a.add(
                        &mesh::cylinder(0.018, 0.003, 16),
                        Mat4::from_translation(Vec3::new(x, y, z))
                            * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
                        black(),
                    );
                }
            }
        }
        PropKind::Cone => {
            a.block(
                Vec3::new(0.27, 0.025, 0.27),
                Vec3::new(0.0, -0.30, 0.0),
                black(),
            );
            a.add(
                &mesh::lathe(&[(-0.275, 0.225), (0.28, 0.048), (0.325, 0.035)], 48, true),
                Mat4::IDENTITY,
                paint([0.94, 0.24, 0.025]),
            );
            a.add(
                &mesh::lathe(&[(-0.02, 0.145), (0.10, 0.107)], 48, false),
                Mat4::IDENTITY,
                Material::new(0, [0.87, 0.86, 0.78], 0.42, 0.0),
            );
        }
        PropKind::Pallet => {
            for (p, h) in pallet_parts() {
                a.block(h, p, wood());
            }
            for x in [-0.48, 0.0, 0.48] {
                for z in [-0.43, 0.43] {
                    a.add(
                        &mesh::cylinder(0.007, 0.003, 8),
                        Mat4::from_translation(Vec3::new(x, 0.092, z)),
                        steel(),
                    );
                }
            }
        }
    }
    a.finish(r)
}

pub struct Lab {
    pub physics: Physics,
    pub player: RigidBodyHandle,
    pub props: Vec<Prop>,
    pub statics: Vec<Draw>,
    pub assemblies: Vec<Vec<Part>>,
    pub cube: MeshId,
    pub sphere: MeshId,
    pub cylinder: MeshId,
    pub tool: MeshId,
    next_tag: u128,
}
impl Lab {
    pub fn new(r: &mut Renderer) -> anyhow::Result<Self> {
        let cube = r.mesh(&mesh::bevel_box(Vec3::splat(0.5), 0.018));
        let sphere = r.mesh(&mesh::sphere(1.0, 16, 12));
        let cylinder = r.mesh(&mesh::cylinder(1.0, 1.0, 20));
        let model = glb::load(&crate::asset_dir().join("models/void-manipulator.glb"))?;
        let tool = r.mesh(&model.mesh);
        let assemblies = PropKind::ALL
            .iter()
            .map(|&kind| build_prop(r, kind))
            .collect();
        let mut physics = Physics::default();
        let player = physics.player(Vec3::new(0.0, 0.81, 9.6));
        let mut lab = Self {
            physics,
            player,
            props: Vec::new(),
            statics: Vec::new(),
            assemblies,
            cube,
            sphere,
            cylinder,
            tool,
            next_tag: 1,
        };
        lab.build_level();
        lab.populate();
        for _ in 0..140 {
            lab.physics.step(physics::FIXED_DT);
        }
        for p in &mut lab.props {
            let b = &lab.physics.bodies[p.body];
            p.spawn = physics::vec3(b.translation());
            p.rotation = physics::quaternion(b.rotation());
        }
        Ok(lab)
    }
    fn block(&mut self, size: Vec3, p: Vec3, q: Quat, m: Material, collider: bool) {
        self.statics.push(Draw::new(
            self.cube,
            Mat4::from_scale_rotation_translation(size, q, p),
            m,
        ));
        if collider {
            self.physics.add(Shape::Box(size * 0.5), p, q, 0.0, 0.72, 0);
        }
    }
    fn beam(&mut self, a: Vec3, b: Vec3, width: f32, m: Material) {
        let d = b - a;
        self.block(
            Vec3::new(width, d.length(), width),
            (a + b) * 0.5,
            Quat::from_rotation_arc(Vec3::Y, d.normalize()),
            m,
            false,
        );
    }
    fn build_level(&mut self) {
        let concrete = Material::new(tex::CONCRETE, [0.74, 0.76, 0.75], 0.88, 0.0);
        let metal = paint([0.22, 0.27, 0.29]);
        let yellow = paint([0.74, 0.48, 0.08]);
        let identity = Quat::IDENTITY;
        self.physics.add(
            Shape::Box(Vec3::new(15.0, 0.25, 14.0)),
            Vec3::new(0.0, -0.25, 0.0),
            identity,
            0.0,
            0.8,
            0,
        );
        // Large poured-concrete slabs with joints, rather than a noisy checkerboard.
        for x in -5..5 {
            for z in -5..5 {
                let mut m = concrete;
                let variation = 0.96 + ((x * 13 + z * 7i32).rem_euclid(7)) as f32 * 0.008;
                m.color = [0.74 * variation, 0.76 * variation, 0.75 * variation, 1.0];
                m.uv = [3.0, 3.0];
                self.block(
                    Vec3::new(2.996, 0.08, 2.796),
                    Vec3::new(x as f32 * 3.0 + 1.5, -0.038, z as f32 * 2.8 + 1.4),
                    identity,
                    m,
                    false,
                );
            }
        }
        for z in [-14.0, 14.0] {
            self.block(
                Vec3::new(30.5, 9.0, 0.4),
                Vec3::new(0.0, 4.5, z),
                identity,
                concrete,
                true,
            );
            self.block(
                Vec3::new(30.0, 0.18, 0.08),
                Vec3::new(0.0, 0.18, z - z.signum() * 0.23),
                identity,
                metal,
                false,
            );
            for i in -25..25 {
                self.block(
                    Vec3::new(0.055, 3.3, 0.08),
                    Vec3::new(i as f32 * 0.60, 7.30, z - z.signum() * 0.24),
                    identity,
                    metal,
                    false,
                );
            }
        }
        for x in [-15.0, 15.0] {
            self.block(
                Vec3::new(0.4, 9.0, 28.0),
                Vec3::new(x, 4.5, 0.0),
                identity,
                concrete,
                true,
            );
            for i in -23..23 {
                self.block(
                    Vec3::new(0.08, 3.3, 0.055),
                    Vec3::new(x - x.signum() * 0.24, 7.30, i as f32 * 0.60),
                    identity,
                    metal,
                    false,
                );
            }
        }
        // Structural I-beams, triangulated roof trusses and skylight framing.
        for z in [-10.5, -3.5, 3.5, 10.5] {
            for x in [-14.6, 14.6] {
                self.block(
                    Vec3::new(0.18, 8.5, 0.25),
                    Vec3::new(x, 4.25, z),
                    identity,
                    metal,
                    true,
                );
                for dx in [-0.15, 0.15] {
                    self.block(
                        Vec3::new(0.10, 8.5, 0.44),
                        Vec3::new(x + dx, 4.25, z),
                        identity,
                        metal,
                        false,
                    );
                }
                self.block(
                    Vec3::new(0.55, 1.0, 0.55),
                    Vec3::new(x, 0.50, z),
                    identity,
                    Material::new(tex::HAZARD, [0.8; 3], 0.8, 0.0),
                    false,
                );
            }
            for y in [8.0, 8.9] {
                self.block(
                    Vec3::new(29.5, 0.12, 0.16),
                    Vec3::new(0.0, y, z),
                    identity,
                    metal,
                    false,
                );
            }
            for i in -7..7 {
                self.beam(
                    Vec3::new(i as f32 * 2.0, 8.0, z),
                    Vec3::new(i as f32 * 2.0 + 1.0, 8.9, z),
                    0.07,
                    metal,
                );
                self.beam(
                    Vec3::new(i as f32 * 2.0 + 1.0, 8.9, z),
                    Vec3::new(i as f32 * 2.0 + 2.0, 8.0, z),
                    0.07,
                    metal,
                );
            }
            for x in [-7.5, 7.5] {
                self.block(
                    Vec3::new(2.3, 0.10, 0.42),
                    Vec3::new(x, 6.9, z),
                    identity,
                    metal,
                    false,
                );
                self.block(
                    Vec3::new(2.1, 0.035, 0.31),
                    Vec3::new(x, 6.83, z),
                    identity,
                    Material::glow([1.0, 0.87, 0.65], 3.5),
                    false,
                );
                for dx in [-0.85, 0.85] {
                    self.beam(
                        Vec3::new(x + dx, 8.0, z),
                        Vec3::new(x + dx, 6.95, z),
                        0.018,
                        steel(),
                    );
                }
            }
        }
        for x in [-12.0, -6.0, 0.0, 6.0, 12.0] {
            self.block(
                Vec3::new(0.09, 0.1, 28.0),
                Vec3::new(x, 9.0, 0.0),
                identity,
                metal,
                false,
            );
        }
        // Roof leaves the centre skylight open for the directional key light.
        for x in [-10.3, 10.3] {
            self.block(
                Vec3::new(9.0, 0.08, 28.0),
                Vec3::new(x, 9.07, 0.0),
                identity,
                paint([0.28, 0.32, 0.34]),
                false,
            );
        }
        for z in [-13.77, 13.77] {
            self.block(
                Vec3::new(29.5, 0.12, 0.12),
                Vec3::new(0.0, 4.4, z),
                identity,
                yellow,
                false,
            );
        }
        // Loading door, recessed frame, roller slats and warning beacons.
        self.block(
            Vec3::new(5.6, 4.8, 0.10),
            Vec3::new(-8.0, 2.4, -13.73),
            identity,
            steel(),
            false,
        );
        for i in 0..22 {
            self.block(
                Vec3::new(5.0, 0.18, 0.08),
                Vec3::new(-8.0, 0.16 + i as f32 * 0.20, -13.65),
                identity,
                paint([0.29, 0.34, 0.35]),
                false,
            );
        }
        for x in [-10.8, -5.2] {
            self.block(
                Vec3::new(0.18, 1.10, 0.18),
                Vec3::new(x, 0.55, -13.3),
                identity,
                yellow,
                true,
            );
            self.statics.push(Draw::new(
                self.sphere,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.07),
                    identity,
                    Vec3::new(x, 4.85, -13.5),
                ),
                Material::glow([1.0, 0.23, 0.04], 2.5),
            ));
        }
        // North wall identity board, suitable for a projected UI decal.
        self.block(
            Vec3::new(5.5, 1.9, 0.10),
            Vec3::new(0.5, 4.7, -13.68),
            identity,
            steel(),
            false,
        );
        self.block(
            Vec3::new(5.32, 1.72, 0.025),
            Vec3::new(0.5, 4.7, -13.61),
            identity,
            Material::new(0, [0.025, 0.038, 0.045], 0.85, 0.0),
            false,
        );
        for y in [3.80, 5.60] {
            self.block(
                Vec3::new(5.4, 0.025, 0.03),
                Vec3::new(0.5, y, -13.59),
                identity,
                yellow,
                false,
            );
        }
        // Cargo deck with complete rail posts and a usable ramp.
        self.block(
            Vec3::new(6.1, 0.24, 5.8),
            Vec3::new(11.2, 2.1, -5.0),
            identity,
            metal,
            true,
        );
        for x in [8.3, 14.1] {
            for z in [-7.7, -2.3] {
                self.block(
                    Vec3::new(0.19, 2.0, 0.19),
                    Vec3::new(x, 1.0, z),
                    identity,
                    metal,
                    true,
                );
            }
        }
        for z in [-7.85, -2.15] {
            self.block(
                Vec3::new(6.1, 0.15, 0.05),
                Vec3::new(11.2, 2.15, z),
                identity,
                Material::new(tex::HAZARD, [0.85; 3], 0.65, 0.0),
                false,
            );
        }
        for x in [8.2, 14.2] {
            for z in [-7.8, -6.0, -4.2, -2.2] {
                self.block(
                    Vec3::new(0.05, 1.05, 0.05),
                    Vec3::new(x, 2.75, z),
                    identity,
                    yellow,
                    true,
                );
            }
            for y in [2.8, 3.3] {
                self.block(
                    Vec3::new(0.05, 0.05, 5.6),
                    Vec3::new(x, y, -5.0),
                    identity,
                    yellow,
                    true,
                );
            }
        }
        let angle = (2.22f32 / 6.0).atan();
        self.block(
            Vec3::new(2.6, 0.20, 6.5),
            Vec3::new(11.2, 1.06, 1.0),
            Quat::from_rotation_x(angle),
            concrete,
            true,
        );
        for x in [9.93, 12.47] {
            self.block(
                Vec3::new(0.09, 0.055, 6.5),
                Vec3::new(x, 1.19, 1.0),
                Quat::from_rotation_x(angle),
                yellow,
                false,
            );
        }
        // Warehouse rack, back braces, pallets and workshop bench.
        for x in [-13.9, -12.5] {
            for z in [-8.8, -4.2] {
                self.block(
                    Vec3::new(0.09, 3.6, 0.09),
                    Vec3::new(x, 1.8, z),
                    identity,
                    paint([0.17, 0.28, 0.34]),
                    true,
                );
            }
        }
        for y in [0.3, 1.35, 2.40, 3.45] {
            self.block(
                Vec3::new(1.5, 0.07, 4.7),
                Vec3::new(-13.2, y, -6.5),
                identity,
                wood(),
                true,
            );
            for x in [-13.96, -12.44] {
                self.block(
                    Vec3::new(0.07, 0.16, 4.7),
                    Vec3::new(x, y, -6.5),
                    identity,
                    paint([0.76, 0.37, 0.11]),
                    false,
                );
            }
        }
        self.beam(
            Vec3::new(-13.96, 0.3, -8.7),
            Vec3::new(-13.96, 3.4, -4.3),
            0.04,
            metal,
        );
        self.beam(
            Vec3::new(-13.96, 3.4, -8.7),
            Vec3::new(-13.96, 0.3, -4.3),
            0.04,
            metal,
        );
        self.block(
            Vec3::new(0.6, 0.70, 0.7),
            Vec3::new(-7.5, 0.35, -2.0),
            identity,
            metal,
            true,
        );
        for x in [-1.7, 1.7] {
            self.block(
                Vec3::new(0.075, 0.005, 9.0),
                Vec3::new(x, 0.007, 5.8),
                identity,
                yellow,
                false,
            );
        }
        for z in [-10.0, 0.0] {
            for x in -6..7 {
                self.block(
                    Vec3::new(0.55, 0.005, 0.07),
                    Vec3::new(x as f32 * 0.9, 0.008, z),
                    identity,
                    yellow,
                    false,
                );
            }
        }
        // Conduits and a ventilation duct with expansion rings.
        for z in [-11.5, -10.9] {
            self.beam(
                Vec3::new(-14.5, 5.5, z),
                Vec3::new(14.5, 5.5, z),
                0.055,
                steel(),
            );
        }
        // Cylinder reuses the unit GPU mesh; the profile highlights read as sheet metal.
        self.statics.push(Draw::new(
            self.cylinder,
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.24, 26.0, 0.24),
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                Vec3::new(0.0, 7.2, -12.4),
            ),
            steel(),
        ));
        for x in -6..7 {
            self.statics.push(Draw::new(
                self.cylinder,
                Mat4::from_scale_rotation_translation(
                    Vec3::new(0.255, 0.045, 0.255),
                    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    Vec3::new(x as f32 * 2.0, 7.2, -12.4),
                ),
                metal,
            ));
        }
    }
    pub fn spawn(&mut self, kind: PropKind, p: Vec3, q: Quat, initial: bool) -> RigidBodyHandle {
        let body = self
            .physics
            .add(kind.shape(), p, q, kind.mass(), 0.62, self.next_tag);
        self.next_tag += 1;
        let tint = 0.90 + (self.props.len() % 5) as f32 * 0.035;
        self.props.push(Prop {
            kind,
            body,
            spawn: p,
            rotation: q,
            initial,
            tint,
        });
        self.physics.refresh_queries();
        body
    }
    fn populate(&mut self) {
        let q = Quat::IDENTITY;
        for row in 0..3 {
            for col in 0..3 - row {
                self.spawn(
                    PropKind::Crate,
                    Vec3::new(
                        -0.9 + col as f32 * 0.92 + row as f32 * 0.46,
                        0.46 + row as f32 * 0.91,
                        -7.5,
                    ),
                    q,
                    true,
                );
            }
        }
        for (x, y, z) in [
            (3.0, 0.46, 3.0),
            (3.94, 0.46, 3.15),
            (4.7, 0.46, 2.4),
            (4.7, 1.38, 2.4),
            (5.5, 0.46, 3.7),
            (10.1, 2.69, -4.1),
            (12.8, 2.69, -6.0),
            (-13.2, 1.85, -5.6),
        ] {
            self.spawn(PropKind::Crate, Vec3::new(x, y, z), q, true);
        }
        for (i, (x, z)) in [
            (-4.3, 3.4),
            (-5.0, 3.2),
            (-4.8, 4.2),
            (-5.65, 4.0),
            (-6.3, 4.6),
            (13.0, -3.1),
        ]
        .into_iter()
        .enumerate()
        {
            self.spawn(
                PropKind::Barrel,
                Vec3::new(x, if i == 5 { 2.67 } else { 0.46 }, z),
                Quat::from_rotation_y(i as f32 * 0.7),
                true,
            );
        }
        for (x, y, z) in [
            (0.1, 0.29, 5.1),
            (1.1, 0.29, 4.8),
            (2.1, 0.29, 4.5),
            (9.7, 2.52, -6.2),
            (-13.2, 3.78, -7.8),
        ] {
            self.spawn(PropKind::Sphere, Vec3::new(x, y, z), q, true);
        }
        for i in 0..9 {
            self.spawn(
                PropKind::Domino,
                Vec3::new(7.1, 0.33, 1.5 + i as f32 * 0.44),
                q,
                true,
            );
        }
        for i in 0..6 {
            self.spawn(
                PropKind::Brick,
                Vec3::new(
                    -9.3 + (i % 3) as f32 * 0.50,
                    0.12 + (i / 3) as f32 * 0.23,
                    -0.5,
                ),
                q,
                true,
            );
        }
        for (kind, p) in [
            (PropKind::Plank, Vec3::new(2.7, 0.065, 1.2)),
            (PropKind::Plank, Vec3::new(-9.5, 0.065, 2.5)),
            (PropKind::Can, Vec3::new(2.0, 0.24, 1.7)),
            (PropKind::Can, Vec3::new(2.5, 0.24, 1.8)),
            (PropKind::Cone, Vec3::new(9.1, 0.33, 4.8)),
            (PropKind::Cone, Vec3::new(8.4, 0.33, 4.2)),
            (PropKind::Pallet, Vec3::new(5.4, 0.10, 0.6)),
            (PropKind::Pallet, Vec3::new(-2.6, 0.10, -0.9)),
            (PropKind::Balance, Vec3::new(-7.5, 0.79, -2.0)),
        ] {
            self.spawn(kind, p, q, true);
        }
    }
    pub fn reset(&mut self) {
        self.props.retain(|p| {
            if !p.initial {
                self.physics.remove(p.body);
                false
            } else {
                true
            }
        });
        for p in &self.props {
            self.physics.release(p.body, None);
            let b = &mut self.physics.bodies[p.body];
            b.set_translation(physics::vector(p.spawn), true);
            b.set_rotation(physics::rotation(p.rotation), true);
            b.set_linvel(physics::vector(Vec3::ZERO), true);
            b.set_angvel(physics::vector(Vec3::ZERO), true);
        }
        let b = &mut self.physics.bodies[self.player];
        b.set_translation(physics::vector(Vec3::new(0.0, 0.81, 9.6)), true);
        b.set_linvel(physics::vector(Vec3::ZERO), true);
        self.physics.refresh_queries();
    }
    pub fn draws(&self) -> Vec<Draw> {
        let mut draws = self.statics.clone();
        for p in &self.props {
            let b = &self.physics.bodies[p.body];
            let transform = Mat4::from_rotation_translation(
                physics::quaternion(b.rotation()),
                physics::vec3(b.translation()),
            );
            for part in &self.assemblies[p.kind as usize] {
                let mut material = part.material;
                for c in &mut material.color[..3] {
                    *c *= p.tint;
                }
                draws.push(Draw::new(part.mesh, transform, material));
            }
        }
        draws
    }
}
