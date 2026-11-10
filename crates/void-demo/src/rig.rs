//! One rigid tool frame, metre-scale hands and two-bone arm IK.
//! Grip anchors are authored in GLB space; hand dimensions never inherit tool scale.
use glam::{Mat3, Mat4, Quat, Vec3};
use void_engine::{
    mesh::{self, MeshData},
    render::{Camera, Draw, Material, MeshId, Renderer},
    textures,
};

pub const TOOL_LENGTH: f32 = 0.56;
pub const VIEW_FOV: f32 = 62.0;
const RIGHT_WRIST: Vec3 = Vec3::new(-0.075, -0.225, -0.400);
const LEFT_WRIST: Vec3 = Vec3::new(0.100, -0.155, 0.015);
const EMITTER: Vec3 = Vec3::new(0.0, 0.018, 0.472);

pub struct Rig {
    palm: MeshId,
    guard: MeshId,
    tip: MeshId,
    finger: MeshId,
    sleeve: MeshId,
    cuff: MeshId,
    pub kick: f32,
    kick_speed: f32,
    lag: glam::Vec2,
    pub heat: f32,
    pub curl: f32,
    pub root: Mat4,
    phase: f32,
}
fn frame(y: Vec3, z: Vec3) -> Quat {
    let y = y.normalize();
    let z = (z - y * z.dot(y)).normalize();
    let x = y.cross(z);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}
fn transform(p: Vec3, q: Quat, s: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(s, q, p)
}
impl Rig {
    pub fn new(r: &mut Renderer) -> Self {
        // Broad metacarpal surface, tapered wrist, joined finger pads and stitched cuff.
        let mut palm = MeshData::default();
        palm.append(
            &mesh::bevel_box(Vec3::new(0.039, 0.043, 0.014), 0.012),
            Mat4::from_translation(Vec3::new(0.0, 0.053, 0.0)),
        );
        palm.append(
            &mesh::segment(
                Vec3::new(0.0, 0.004, 0.0),
                Vec3::new(0.0, 0.038, 0.0),
                0.025,
                0.031,
            ),
            Mat4::from_scale(Vec3::new(1.0, 1.0, 0.55)),
        );
        let palm = r.mesh(&palm);
        let guard = r.mesh(&mesh::bevel_box(Vec3::new(0.027, 0.020, 0.003), 0.003));
        let tip = r.mesh(&mesh::sphere(1.0, 12, 8));
        let finger = r.mesh(&mesh::lathe(
            &[(0.0, 0.96), (0.12, 1.0), (0.88, 0.83), (1.0, 0.78)],
            12,
            true,
        ));
        let sleeve = r.mesh(&mesh::lathe(
            &[
                (0.0, 1.0),
                (0.2, 0.98),
                (0.3, 1.03),
                (0.42, 0.91),
                (0.58, 0.93),
                (0.7, 0.82),
                (0.82, 0.86),
                (1.0, 0.73),
            ],
            24,
            true,
        ));
        let cuff = r.mesh(&mesh::cylinder(1.0, 1.0, 28));
        Self {
            palm,
            guard,
            tip,
            finger,
            sleeve,
            cuff,
            kick: 0.0,
            kick_speed: 0.0,
            lag: glam::Vec2::ZERO,
            heat: 0.0,
            curl: 0.0,
            root: Mat4::IDENTITY,
            phase: 0.0,
        }
    }
    pub fn recoil(&mut self) {
        self.kick_speed += 1.4;
    }
    pub fn update(
        &mut self,
        dt: f32,
        time: f32,
        look: glam::Vec2,
        speed: f32,
        held: bool,
        bob: bool,
    ) {
        let dt = dt.min(0.04);
        let blend = 1.0 - (-12.0 * dt).exp();
        self.lag = self.lag.lerp(
            (look * 0.001).clamp(glam::Vec2::splat(-0.045), glam::Vec2::splat(0.045)),
            blend,
        );
        self.kick_speed += (-self.kick * 145.0 - self.kick_speed * 21.0) * dt;
        self.kick += self.kick_speed * dt;
        self.heat += ((if held { 1.0 } else { 0.12 }) - self.heat) * blend;
        self.curl += ((if held { 1.0 } else { 0.0 }) - self.curl) * blend;
        self.phase += speed * dt * 1.6;
        let walk = if bob { (speed / 5.0).min(1.0) } else { 0.0 };
        let p = Vec3::new(
            0.225 - self.lag.x * 0.3 + self.phase.sin() * 0.004 * walk,
            -0.160 - self.lag.y * 0.25
                + (time * 1.5).sin() * 0.0015
                + (self.phase * 2.0).sin() * 0.003 * walk,
            -0.75 + self.kick * 0.05,
        );
        let q = Quat::from_rotation_z(self.phase.sin() * 0.008 * walk)
            * Quat::from_rotation_x(self.lag.y - self.kick * 0.18)
            * Quat::from_rotation_y(std::f32::consts::PI + 0.10 + self.lag.x);
        self.root = Mat4::from_rotation_translation(q, p);
    }
    pub fn emitter(&self, camera: &Camera) -> Vec3 {
        let mut local = self.root.transform_point3(EMITTER * TOOL_LENGTH);
        // Match the muzzle's screen position across the independent projections.
        let ratio = (camera.fov.to_radians() * 0.5).tan() / (VIEW_FOV.to_radians() * 0.5).tan();
        local.x *= ratio;
        local.y *= ratio;
        camera.world().transform_point3(local)
    }
    fn item(draws: &mut Vec<Draw>, mesh: MeshId, world: Mat4, local: Mat4, material: Material) {
        let mut draw = Draw::new(mesh, world * local, material);
        draw.viewmodel = true;
        draw.shadow = false;
        draws.push(draw);
    }
    pub fn draw(&self, draws: &mut Vec<Draw>, camera: &Camera, tool: MeshId) {
        let world = camera.world();
        let mut tool_material = Material::new(textures::VIEWMODEL, [1.0; 3], 0.7, 0.0);
        tool_material.baked = true;
        Self::item(
            draws,
            tool,
            world,
            self.root * Mat4::from_scale(Vec3::splat(TOOL_LENGTH)),
            tool_material,
        );
        // Both wrists inherit the entire tool transform including sway and recoil.
        // Only the GLB anchor positions use TOOL_LENGTH; fingers remain adult-sized.
        for left in [false, true] {
            let anchor = if left { LEFT_WRIST } else { RIGHT_WRIST } * TOOL_LENGTH;
            let direction = if left {
                Vec3::new(-0.20, 0.82, 0.40)
            } else {
                Vec3::new(0.02, 0.93, 0.38)
            };
            let back = if left {
                Vec3::new(1.0, 0.05, 0.0)
            } else {
                Vec3::new(-1.0, 0.05, 0.0)
            };
            let grip = Mat4::from_rotation_translation(frame(direction, back), anchor);
            let mirror = if left { -1.0 } else { 1.0 };
            let hand = self.root * grip * Mat4::from_scale(Vec3::new(mirror, 1.0, 1.0));
            let glove = Material::new(textures::RUBBER, [0.24, 0.28, 0.31], 0.85, 0.0);
            let armor = Material::new(textures::PAINT, [0.19, 0.23, 0.25], 0.53, 0.15);
            Self::item(draws, self.palm, world, hand, glove);
            Self::item(
                draws,
                self.guard,
                world,
                hand * Mat4::from_translation(Vec3::new(0.0, 0.065, 0.018)),
                armor,
            );
            let widths = [0.0090, 0.0094, 0.0087, 0.0075];
            let lengths = [
                [0.038, 0.024, 0.018],
                [0.043, 0.027, 0.019],
                [0.040, 0.025, 0.018],
                [0.031, 0.020, 0.016],
            ];
            for i in 0..4 {
                let x = -0.027 + i as f32 * 0.018;
                let y = if i == 3 { 0.083 } else { 0.092 };
                let mut joint = hand * Mat4::from_translation(Vec3::new(x, y, -0.002));
                let angles = if !left && i == 0 {
                    [0.48 + self.curl * 0.14, 0.66 + self.curl * 0.18, 0.28]
                } else {
                    [0.92 + self.curl * 0.10, 1.35 + self.curl * 0.10, 0.72]
                };
                for j in 0..3 {
                    joint *= Mat4::from_rotation_x(-angles[j]);
                    let len = lengths[i][j];
                    let radius = widths[i] * (1.0 - j as f32 * 0.12);
                    Self::item(
                        draws,
                        self.tip,
                        world,
                        joint * Mat4::from_scale(Vec3::splat(radius)),
                        glove,
                    );
                    Self::item(
                        draws,
                        self.finger,
                        world,
                        joint * Mat4::from_scale(Vec3::new(radius, len, radius)),
                        glove,
                    );
                    joint *= Mat4::from_translation(Vec3::Y * len);
                }
                Self::item(
                    draws,
                    self.tip,
                    world,
                    joint * Mat4::from_scale(Vec3::splat(widths[i] * 0.68)),
                    glove,
                );
            }
            let mut thumb = hand
                * Mat4::from_translation(Vec3::new(-0.031, 0.028, -0.003))
                * Mat4::from_quat(frame(
                    Vec3::new(-0.50, 0.60, -0.55),
                    Vec3::new(-0.3, 0.3, 0.8),
                ));
            for (i, len) in [0.030, 0.026, 0.023].into_iter().enumerate() {
                thumb *= Mat4::from_rotation_x(-0.25 - i as f32 * 0.12);
                let radius = 0.011 - i as f32 * 0.0014;
                Self::item(
                    draws,
                    self.tip,
                    world,
                    thumb * Mat4::from_scale(Vec3::splat(radius)),
                    glove,
                );
                Self::item(
                    draws,
                    self.finger,
                    world,
                    thumb * Mat4::from_scale(Vec3::new(radius, len, radius)),
                    glove,
                );
                thumb *= Mat4::from_translation(Vec3::Y * len);
            }
            // Solve a real elbow instead of drawing detached cylinders toward the floor.
            let wrist = hand.transform_point3(Vec3::ZERO);
            let shoulder = if left {
                Vec3::new(-0.16, -0.25, -0.20)
            } else {
                Vec3::new(0.26, -0.23, -0.04)
            };
            let pole = if left {
                Vec3::new(-0.36, -0.65, -0.10)
            } else {
                Vec3::new(0.46, -0.58, -0.02)
            };
            let elbow = solve_elbow(shoulder, wrist, pole, 0.33, 0.30);
            let sleeve = Material::new(textures::RUBBER, [0.37, 0.40, 0.36], 0.94, 0.0);
            for (a, b, r0) in [(shoulder, elbow, 0.058), (elbow, wrist, 0.045)] {
                let d = b - a;
                Self::item(
                    draws,
                    self.sleeve,
                    world,
                    transform(
                        a,
                        Quat::from_rotation_arc(Vec3::Y, d.normalize()),
                        Vec3::new(r0, d.length(), r0),
                    ),
                    sleeve,
                );
            }
            let cuff_pos = wrist + (elbow - wrist).normalize() * 0.018;
            Self::item(
                draws,
                self.cuff,
                world,
                transform(
                    cuff_pos,
                    Quat::from_rotation_arc(Vec3::Y, (wrist - elbow).normalize()),
                    Vec3::new(0.031, 0.032, 0.031),
                ),
                glove,
            );
            Self::item(
                draws,
                self.cuff,
                world,
                transform(
                    cuff_pos,
                    Quat::from_rotation_arc(Vec3::Y, (wrist - elbow).normalize()),
                    Vec3::new(0.0315, 0.004, 0.0315),
                ),
                Material::new(0, [0.74, 0.43, 0.12], 0.65, 0.0),
            );
        }
        Self::item(
            draws,
            self.tip,
            world,
            self.root
                * Mat4::from_translation(EMITTER * TOOL_LENGTH)
                * Mat4::from_scale(Vec3::splat(0.009 + self.heat * 0.004)),
            Material::glow([1.0, 0.39, 0.075], 2.0 + self.heat * 3.0),
        );
    }
}

pub fn solve_elbow(shoulder: Vec3, wrist: Vec3, pole: Vec3, upper: f32, lower: f32) -> Vec3 {
    let delta = wrist - shoulder;
    let distance = delta
        .length()
        .clamp((upper - lower).abs() + 0.001, upper + lower - 0.001);
    let axis = delta.normalize();
    let along = (upper * upper - lower * lower + distance * distance) / (2.0 * distance);
    let height = (upper * upper - along * along).max(0.0).sqrt();
    let pole = pole - shoulder;
    let perpendicular = (pole - axis * pole.dot(axis)).normalize_or_zero();
    shoulder + axis * along + perpendicular * height
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ik_preserves_anatomical_lengths_and_tool_scale() {
        let s = Vec3::new(0.25, -0.20, 0.1);
        let w = Vec3::new(0.28, -0.34, -0.35);
        let e = solve_elbow(s, w, Vec3::new(0.4, -0.6, 0.0), 0.30, 0.28);
        assert!(((e - s).length() - 0.30).abs() < 0.001);
        assert!(((w - e).length() - 0.28).abs() < 0.001);
        assert!((TOOL_LENGTH - 0.56).abs() < 0.001);
    }
}
