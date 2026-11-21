use std::{collections::VecDeque, path::Path};
use void_engine::{
    audio::Audio,
    mesh::{self, MeshData},
    physics::{Physics, RigidBodyHandle},
    render::{Camera, Draw, Material, MeshId, PointLight, Renderer},
    scene::{ImpactSurface, Scene},
    textures as tex,
    viewmodel::RiggedModel,
};

use glam::{Mat4, Quat, Vec2, Vec3};

const MAG_CAPACITY: u32 = 15;
const MAX_BULLET_HOLES: usize = 128;
const FIRE_COOLDOWN: f32 = 0.135;
const SPEED_OF_SOUND: f32 = 343.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReloadKind {
    Easy,
    Full,
}

struct BulletHole {
    body: RigidBodyHandle,
    local_pos: Vec3,
    local_rot: Quat,
    size: f32,
    layer: u32,
    roughness: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParticleKind {
    Spark,
    Dust,
    Chip,
    WoodSplinter,
    Casing,
}

struct Particle {
    kind: ParticleKind,
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    ang_vel: Vec3,
    scale: Vec3,
    age: f32,
    lifetime: f32,
    material: Material,
    bounced: bool,
}

struct Tracer {
    transform: Mat4,
    age: f32,
    lifetime: f32,
}

pub struct WeaponHud {
    pub equipped: bool,
    pub ammo: u32,
    pub chambered: bool,
    pub reloading_progress: Option<f32>,
    pub ads: f32,
    pub spread_px: f32,
    pub hitmarker: f32,
    pub indoor_mix: f32,
}

pub struct WeaponSystem {
    model: RiggedModel,
    submesh_ids: Vec<MeshId>,
    submesh_materials: Vec<Material>,
    skin_mats: Vec<Mat4>,
    scratch_mesh: MeshData,

    decal_mesh: MeshId,
    spark_mesh: MeshId,
    dust_mesh: MeshId,
    chip_mesh: MeshId,
    casing_mesh: MeshId,
    tracer_mesh: MeshId,
    flash_mesh: MeshId,

    pub equipped: bool,
    ammo: u32,
    chambered: bool,
    fire_cooldown: f32,
    reloading: Option<(ReloadKind, f32, bool)>,
    fire_anim: Option<(usize, f32)>,
    fire_variant: usize,
    idle_time: f32,
    walk_time: f32,
    walk_blend: f32,
    inspect_timer: f32,

    pub ads_held: bool,
    ads_t: f32,
    sprint_t: f32,
    holster_t: f32,

    // Procedural recoil, sway & screen shake
    recoil_pitch: f32,
    recoil_yaw: f32,
    recoil_pitch_vel: f32,
    recoil_yaw_vel: f32,
    kick_back: f32,
    shake_trauma: f32,
    sway: Vec2,
    strafe_roll: f32,
    land_dip: f32,
    was_grounded: bool,
    prev_vy: f32,
    spread_bloom: f32,
    hitmarker: f32,
    indoor_mix: f32,

    muzzle_flash_timer: f32,
    muzzle_flash_pos: Vec3,
    muzzle_flash_rot: Quat,
    muzzle_flash_scale: f32,
    impact_light_timer: f32,
    impact_light_pos: Vec3,

    rng_state: u32,
    bullet_holes: VecDeque<BulletHole>,
    particles: Vec<Particle>,
    tracers: Vec<Tracer>,
}

impl WeaponSystem {
    pub fn new(renderer: &mut Renderer, assets: &Path) -> anyhow::Result<Self> {
        let model = RiggedModel::load(&assets.join("models/fps_pistol.glb"))?;
        let mut submesh_ids = Vec::with_capacity(model.submeshes.len());
        let mut submesh_materials = Vec::with_capacity(model.submeshes.len());
        for sub in &model.submeshes {
            submesh_ids.push(renderer.dynamic_mesh(&sub.rest));
            let mat = match sub.name.as_str() {
                "arms" => Material::new(tex::ARMS, [1.0, 1.0, 1.0], 0.85, 0.05),
                "pistol_slide" => Material::new(tex::PISTOL, [1.05, 1.05, 1.08], 0.36, 0.85),
                "pistol_mag" => Material::new(tex::PISTOL, [1.0, 1.0, 1.0], 0.48, 0.65),
                _ => Material::new(tex::PISTOL, [1.0, 1.0, 1.0], 0.72, 0.14),
            };
            submesh_materials.push(mat);
        }

        let decal_mesh = renderer.mesh(&mesh::quad(0.5, 0.5));
        let spark_mesh = renderer.mesh(&mesh::bevel_box(Vec3::new(0.004, 0.004, 0.022), 0.001));
        let dust_mesh = renderer.mesh(&mesh::sphere(0.026, 6, 4));
        let chip_mesh = renderer.mesh(&mesh::bevel_box(Vec3::splat(0.011), 0.002));
        let casing_mesh = renderer.mesh(&mesh::cylinder(0.0048, 0.019, 8));
        let tracer_mesh = renderer.mesh(&mesh::bevel_box(Vec3::new(0.003, 0.003, 0.5), 0.001));

        let mut flash = mesh::quad(0.075, 0.075);
        flash.append(&mesh::quad(0.075, 0.075), Mat4::from_rotation_z(0.785));
        flash.append(
            &mesh::quad(0.075, 0.045),
            Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );
        let flash_mesh = renderer.mesh(&flash);

        Ok(Self {
            model,
            submesh_ids,
            submesh_materials,
            skin_mats: Vec::new(),
            scratch_mesh: MeshData::default(),
            decal_mesh,
            spark_mesh,
            dust_mesh,
            chip_mesh,
            casing_mesh,
            tracer_mesh,
            flash_mesh,
            equipped: true,
            ammo: MAG_CAPACITY,
            chambered: true,
            fire_cooldown: 0.0,
            reloading: None,
            fire_anim: None,
            fire_variant: 0,
            idle_time: 0.0,
            walk_time: 0.0,
            walk_blend: 0.0,
            inspect_timer: 0.0,
            ads_held: false,
            ads_t: 0.0,
            sprint_t: 0.0,
            holster_t: 0.0,
            recoil_pitch: 0.0,
            recoil_yaw: 0.0,
            recoil_pitch_vel: 0.0,
            recoil_yaw_vel: 0.0,
            kick_back: 0.0,
            shake_trauma: 0.0,
            sway: Vec2::ZERO,
            strafe_roll: 0.0,
            land_dip: 0.0,
            was_grounded: true,
            prev_vy: 0.0,
            spread_bloom: 0.0,
            hitmarker: 0.0,
            indoor_mix: 0.1,
            muzzle_flash_timer: 0.0,
            muzzle_flash_pos: Vec3::ZERO,
            muzzle_flash_rot: Quat::IDENTITY,
            muzzle_flash_scale: 1.0,
            impact_light_timer: 0.0,
            impact_light_pos: Vec3::ZERO,
            rng_state: 0x1357_9BDF,
            bullet_holes: VecDeque::with_capacity(MAX_BULLET_HOLES),
            particles: Vec::new(),
            tracers: Vec::new(),
        })
    }

    pub fn reset(&mut self) {
        self.ammo = MAG_CAPACITY;
        self.chambered = true;
        self.fire_cooldown = 0.0;
        self.reloading = None;
        self.fire_anim = None;
        self.inspect_timer = 0.0;
        self.ads_held = false;
        self.ads_t = 0.0;
        self.recoil_pitch = 0.0;
        self.recoil_yaw = 0.0;
        self.recoil_pitch_vel = 0.0;
        self.recoil_yaw_vel = 0.0;
        self.kick_back = 0.0;
        self.shake_trauma = 0.0;
        self.sway = Vec2::ZERO;
        self.land_dip = 0.0;
        self.spread_bloom = 0.0;
        self.hitmarker = 0.0;
        self.muzzle_flash_timer = 0.0;
        self.impact_light_timer = 0.0;
        self.bullet_holes.clear();
        self.particles.clear();
        self.tracers.clear();
    }

    fn rand_f32(&mut self) -> f32 {
        rand01(&mut self.rng_state)
    }

    fn rand_signed(&mut self) -> f32 {
        rand_s(&mut self.rng_state)
    }

    pub fn toggle_equipped(&mut self, audio: &Audio) {
        self.equipped = !self.equipped;
        self.reloading = None;
        self.ads_held = false;
        self.inspect_timer = 0.0;
        audio.play("ui_click", None, 0.28);
    }

    pub fn trigger_inspect(&mut self) {
        if self.equipped && self.reloading.is_none() && self.holster_t < 0.2 {
            self.inspect_timer = 2.1;
        }
    }

    pub fn start_reload(&mut self, audio: &Audio) {
        if !self.equipped || self.reloading.is_some() || self.holster_t > 0.25 {
            return;
        }
        let max_total = MAG_CAPACITY + if self.chambered { 1 } else { 0 };
        if self.ammo >= max_total {
            return;
        }
        self.inspect_timer = 0.0;
        self.ads_held = false;
        let kind = if self.ammo == 0 {
            ReloadKind::Full
        } else {
            ReloadKind::Easy
        };
        self.reloading = Some((kind, 0.0, false));
        let sfx = match kind {
            ReloadKind::Easy => "pistol_reload_easy",
            ReloadKind::Full => "pistol_reload_full",
        };
        audio.play(sfx, None, 0.72);
    }

    pub fn add_look_delta(&mut self, delta: Vec2) {
        let sens = 0.00055 * (1.0 - self.ads_t * 0.72);
        self.sway.x = (self.sway.x - delta.x * sens).clamp(-0.045, 0.045);
        self.sway.y = (self.sway.y - delta.y * sens).clamp(-0.035, 0.035);
    }

    pub fn fire(
        &mut self,
        scene: &mut Scene,
        audio: &Audio,
        camera: &Camera,
        player_speed: f32,
        grounded: bool,
    ) -> bool {
        if !self.equipped || self.holster_t > 0.25 || self.reloading.is_some() {
            return false;
        }
        if self.fire_cooldown > 0.0 {
            return false;
        }
        if self.ammo == 0 {
            self.fire_cooldown = 0.18;
            audio.play("pistol_dry_fire", Some(camera.position), 0.75);
            return false;
        }

        self.ammo -= 1;
        if self.ammo == 0 {
            self.chambered = false;
        }
        self.fire_cooldown = FIRE_COOLDOWN;
        self.inspect_timer = 0.0;

        let anim_name = match self.fire_variant % 3 {
            0 => "fire_0",
            1 => "fire_1",
            _ => "fire_2",
        };
        self.fire_variant += 1;
        self.fire_anim = Some((self.model.anim_id(anim_name), 0.0));

        // Recoil kick + screen shake
        let ads_mul = 1.0 - self.ads_t * 0.42;
        self.recoil_pitch_vel += 0.34 * ads_mul + self.rand_f32() * 0.08 * ads_mul;
        self.recoil_yaw_vel += self.rand_signed() * 0.14 * ads_mul;
        self.kick_back = (self.kick_back + 0.028 * ads_mul).min(0.055);
        self.shake_trauma = (self.shake_trauma + 0.38 * (0.75 + 0.25 * ads_mul)).min(1.0);
        self.spread_bloom = (self.spread_bloom + 0.008 * ads_mul).min(0.032);

        // World-space viewmodel basis for muzzle & ejection port
        let vm_mat = self.viewmodel_world_matrix(camera);
        let muzzle_local = Vec3::new(0.0265, -0.088, -0.525);
        let eject_local = Vec3::new(0.048, -0.082, -0.375);
        let muzzle_world = vm_mat.transform_point3(muzzle_local);
        let eject_world = vm_mat.transform_point3(eject_local);

        // Acoustic ray probe for MW2019-style indoor vs outdoor gunshot + wall slapback echoes
        let (indoor_mix, echo_taps) =
            probe_acoustics(&scene.physics, scene.player, camera.position);
        self.indoor_mix = indoor_mix;
        let pitch_jitter = 0.975 + self.rand_f32() * 0.05;
        audio.play_gunshot(muzzle_world, indoor_mix, &echo_taps, pitch_jitter);

        // Muzzle flash
        self.muzzle_flash_timer = 0.045;
        self.muzzle_flash_pos = muzzle_world;
        self.muzzle_flash_rot = Quat::from_rotation_y(camera.yaw)
            * Quat::from_rotation_z(self.rand_f32() * std::f32::consts::TAU);
        self.muzzle_flash_scale = 0.78 + self.rand_f32() * 0.42;

        // Eject spinning brass casing
        let right = camera.right();
        let up = Vec3::Y;
        let fwd = camera.forward();
        let eject_vel = right * (2.1 + self.rand_f32() * 0.65)
            + up * (1.7 + self.rand_f32() * 0.55)
            - fwd * (0.5 + self.rand_f32() * 0.4);
        let casing_rot = Quat::from_euler(
            glam::EulerRot::XYZ,
            self.rand_f32() * std::f32::consts::TAU,
            self.rand_f32() * std::f32::consts::TAU,
            self.rand_f32() * std::f32::consts::TAU,
        );
        let casing_ang = Vec3::new(
            14.0 + self.rand_signed() * 8.0,
            18.0 + self.rand_signed() * 10.0,
            22.0 + self.rand_signed() * 12.0,
        );
        self.particles.push(Particle {
            kind: ParticleKind::Casing,
            pos: eject_world,
            vel: eject_vel,
            rot: casing_rot,
            ang_vel: casing_ang,
            scale: Vec3::ONE,
            age: 0.0,
            lifetime: 2.2,
            material: Material::new(tex::WHITE, [0.90, 0.72, 0.28], 0.24, 0.95),
            bounced: false,
        });

        // Ballistic raycast with dynamic spread
        let base_spread = if self.ads_t > 0.75 { 0.0008 } else { 0.0032 };
        let move_spread = (player_speed / 6.4) * 0.007 * (1.0 - self.ads_t * 0.65);
        let air_spread = if grounded { 0.0 } else { 0.014 };
        let total_spread = base_spread + move_spread + air_spread + self.spread_bloom;
        let sx = self.rand_signed() * total_spread;
        let sy = self.rand_signed() * total_spread;
        let aim_dir = (self.aimed_forward(camera)
            + camera.right() * sx
            + camera.world().transform_vector3(Vec3::Y).normalize() * sy)
            .normalize();

        let max_range = 120.0;
        let hit_pos = if let Some(hit) =
            scene
                .physics
                .raycast_detailed(camera.position, aim_dir, max_range, scene.player)
        {
            let surface = scene.impact_surface_of(hit.body);
            let is_prop = scene.prop_index_for_body(hit.body).is_some();
            if is_prop {
                self.hitmarker = 0.16;
                scene
                    .physics
                    .apply_impulse_at_point(hit.body, aim_dir * 6.2, hit.point);
            }
            self.spawn_bullet_hole(scene, hit.body, hit.point, hit.normal, surface);
            self.spawn_impact_effects(audio, hit.point, hit.normal, aim_dir, surface);
            hit.point
        } else {
            camera.position + aim_dir * 60.0
        };

        // Tracer streak
        let delta = hit_pos - muzzle_world;
        let len = delta.length().max(0.2);
        let dir = delta / len;
        let tracer_rot = Quat::from_rotation_arc(Vec3::Z, dir);
        let tracer_mid = (muzzle_world + hit_pos) * 0.5;
        self.tracers.push(Tracer {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(1.0, 1.0, len),
                tracer_rot,
                tracer_mid,
            ),
            age: 0.0,
            lifetime: 0.042,
        });
        true
    }

    fn spawn_bullet_hole(
        &mut self,
        scene: &Scene,
        body: RigidBodyHandle,
        world_pos: Vec3,
        world_normal: Vec3,
        surface: ImpactSurface,
    ) {
        let Some(rb) = scene.physics.bodies.get(body) else {
            return;
        };
        let iso = rb.position();
        let body_pos = void_engine::physics::vec3(&iso.translation.vector);
        let body_rot = void_engine::physics::quaternion(&iso.rotation);
        let inv_rot = body_rot.inverse();

        let n = world_normal.normalize_or_zero();
        let spin = Quat::from_axis_angle(n, self.rand_f32() * std::f32::consts::TAU);
        let world_rot = spin * Quat::from_rotation_arc(Vec3::Z, n);

        let local_pos = inv_rot * (world_pos + n * 0.0035 - body_pos);
        let local_rot = inv_rot * world_rot;
        let (layer, roughness, size) = match surface {
            ImpactSurface::Concrete => (tex::DECAL_CONCRETE, 0.90, 0.068 + self.rand_f32() * 0.022),
            ImpactSurface::Metal => (tex::DECAL_METAL, 0.34, 0.056 + self.rand_f32() * 0.016),
            ImpactSurface::Wood => (tex::DECAL_WOOD, 0.84, 0.072 + self.rand_f32() * 0.020),
        };

        if self.bullet_holes.len() >= MAX_BULLET_HOLES {
            self.bullet_holes.pop_front();
        }
        self.bullet_holes.push_back(BulletHole {
            body,
            local_pos,
            local_rot,
            size,
            layer,
            roughness,
        });
    }

    fn spawn_impact_effects(
        &mut self,
        audio: &Audio,
        pos: Vec3,
        normal: Vec3,
        in_dir: Vec3,
        surface: ImpactSurface,
    ) {
        let n = normal.normalize_or_zero();
        let up = if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
        let t1 = n.cross(up).normalize();
        let t2 = n.cross(t1).normalize();
        let reflect = (in_dir - 2.0 * in_dir.dot(n) * n).normalize_or_zero();
        let grazing = in_dir.dot(n).abs() < 0.42;

        let rng = &mut self.rng_state;
        let pitch = 0.94 + rand01(rng) * 0.12;
        let var = (next_u32(rng) % 2) + 1;
        match surface {
            ImpactSurface::Metal => {
                let sfx = if grazing {
                    format!("bullet_ricochet_{var:02}")
                } else {
                    format!("bullet_metal_{var:02}")
                };
                audio.play_pitched(&sfx, Some(pos), 0.72, pitch);
                self.impact_light_timer = 0.05;
                self.impact_light_pos = pos + n * 0.06;

                for _ in 0..14 {
                    let spread = t1 * rand_s(rng) * 0.85 + t2 * rand_s(rng) * 0.85;
                    let dir = (reflect * 0.65 + n * 0.55 + spread).normalize();
                    let spd = 4.5 + rand01(rng) * 8.5;
                    let warm = rand01(rng);
                    self.particles.push(Particle {
                        kind: ParticleKind::Spark,
                        pos: pos + n * 0.015,
                        vel: dir * spd,
                        rot: Quat::from_rotation_arc(Vec3::Z, dir),
                        ang_vel: Vec3::ZERO,
                        scale: Vec3::new(0.7, 0.7, 0.9 + spd * 0.15),
                        age: 0.0,
                        lifetime: 0.18 + rand01(rng) * 0.24,
                        material: Material::glow([1.0, 0.52 + warm * 0.35, 0.14], 14.0),
                        bounced: false,
                    });
                }
            }
            ImpactSurface::Concrete => {
                audio.play_pitched(&format!("bullet_concrete_{var:02}"), Some(pos), 0.68, pitch);
                for _ in 0..9 {
                    let spread = t1 * rand_s(rng) * 0.7 + t2 * rand_s(rng) * 0.7;
                    let dir = (n * 0.9 + spread).normalize();
                    let spd = 1.0 + rand01(rng) * 2.2;
                    let s = 0.55 + rand01(rng) * 0.6;
                    self.particles.push(Particle {
                        kind: ParticleKind::Dust,
                        pos: pos + n * 0.02,
                        vel: dir * spd + Vec3::Y * 0.45,
                        rot: Quat::IDENTITY,
                        ang_vel: Vec3::ZERO,
                        scale: Vec3::splat(s),
                        age: 0.0,
                        lifetime: 0.35 + rand01(rng) * 0.35,
                        material: Material::new(tex::WHITE, [0.58, 0.56, 0.52], 0.96, 0.0),
                        bounced: false,
                    });
                }
                for _ in 0..8 {
                    let spread = t1 * rand_s(rng) * 0.9 + t2 * rand_s(rng) * 0.9;
                    let dir = (n + spread + Vec3::Y * 0.3).normalize();
                    let spd = 2.4 + rand01(rng) * 4.2;
                    self.particles.push(Particle {
                        kind: ParticleKind::Chip,
                        pos: pos + n * 0.015,
                        vel: dir * spd,
                        rot: Quat::IDENTITY,
                        ang_vel: Vec3::new(
                            rand_s(rng) * 14.0,
                            rand_s(rng) * 14.0,
                            rand_s(rng) * 14.0,
                        ),
                        scale: Vec3::splat(0.65 + rand01(rng) * 0.7),
                        age: 0.0,
                        lifetime: 0.55 + rand01(rng) * 0.4,
                        material: Material::new(tex::WHITE, [0.36, 0.35, 0.33], 0.92, 0.0),
                        bounced: false,
                    });
                }
            }
            ImpactSurface::Wood => {
                audio.play_pitched(&format!("bullet_wood_{var:02}"), Some(pos), 0.68, pitch);
                for _ in 0..10 {
                    let spread = t1 * rand_s(rng) * 0.85 + t2 * rand_s(rng) * 0.85;
                    let dir = (n * 0.95 + spread + Vec3::Y * 0.25).normalize();
                    let spd = 2.2 + rand01(rng) * 4.5;
                    self.particles.push(Particle {
                        kind: ParticleKind::WoodSplinter,
                        pos: pos + n * 0.015,
                        vel: dir * spd,
                        rot: Quat::from_rotation_arc(Vec3::Z, dir),
                        ang_vel: Vec3::new(
                            rand_s(rng) * 16.0,
                            rand_s(rng) * 16.0,
                            rand_s(rng) * 16.0,
                        ),
                        scale: Vec3::new(0.45, 0.45, 1.5 + rand01(rng) * 1.2),
                        age: 0.0,
                        lifetime: 0.50 + rand01(rng) * 0.35,
                        material: Material::new(tex::WHITE, [0.76, 0.46, 0.20], 0.85, 0.0),
                        bounced: false,
                    });
                }
                for _ in 0..5 {
                    let spread = t1 * rand_s(rng) * 0.6 + t2 * rand_s(rng) * 0.6;
                    let dir = (n * 0.85 + spread).normalize();
                    self.particles.push(Particle {
                        kind: ParticleKind::Dust,
                        pos: pos + n * 0.02,
                        vel: dir * (0.9 + rand01(rng) * 1.4) + Vec3::Y * 0.35,
                        rot: Quat::IDENTITY,
                        ang_vel: Vec3::splat(0.0),
                        scale: Vec3::splat(0.55),
                        age: 0.0,
                        lifetime: 0.30 + rand01(rng) * 0.25,
                        material: Material::new(tex::WHITE, [0.80, 0.62, 0.42], 0.95, 0.0),
                        bounced: false,
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        renderer: &Renderer,
        scene: &Scene,
        audio: &Audio,
        camera: &Camera,
        dt: f32,
        sim_dt: f32,
        player_speed: f32,
        grounded: bool,
        vertical_vel: f32,
        sprinting: bool,
        strafe_input: f32,
        holding_something: bool,
    ) {
        // Continuous acoustic probing so the HUD reflects indoor vs outdoor space
        let (indoor_now, _) = probe_acoustics(&scene.physics, scene.player, camera.position);
        self.indoor_mix += (indoor_now - self.indoor_mix) * (1.0 - (-10.0 * dt).exp());

        // Prune decals on deleted props
        self.bullet_holes
            .retain(|h| scene.physics.body_exists(h.body));

        self.fire_cooldown = (self.fire_cooldown - sim_dt).max(0.0);
        self.hitmarker = (self.hitmarker - dt).max(0.0);
        self.muzzle_flash_timer = (self.muzzle_flash_timer - sim_dt).max(0.0);
        self.impact_light_timer = (self.impact_light_timer - sim_dt).max(0.0);
        self.spread_bloom = (self.spread_bloom - dt * 0.065).max(0.0);

        // Landing dip & shake
        if grounded && !self.was_grounded && self.prev_vy < -2.2 {
            let impact = ((-self.prev_vy - 2.2) / 6.0).clamp(0.0, 1.0);
            self.land_dip = (self.land_dip + impact * 0.025).min(0.032);
            self.shake_trauma = (self.shake_trauma + impact * 0.28).min(1.0);
        }
        self.was_grounded = grounded;
        self.prev_vy = vertical_vel;
        self.land_dip *= (-14.0 * dt).exp();

        // Spring-damper recoil recovery
        let k = 165.0;
        let c = 19.0;
        let ap = -k * self.recoil_pitch - c * self.recoil_pitch_vel;
        let ay = -k * self.recoil_yaw - c * self.recoil_yaw_vel;
        self.recoil_pitch_vel += ap * dt;
        self.recoil_yaw_vel += ay * dt;
        self.recoil_pitch = (self.recoil_pitch + self.recoil_pitch_vel * dt).clamp(-0.08, 0.16);
        self.recoil_yaw = (self.recoil_yaw + self.recoil_yaw_vel * dt).clamp(-0.06, 0.06);
        self.kick_back *= (-18.0 * dt).exp();
        self.shake_trauma = (self.shake_trauma - 2.6 * dt).max(0.0);

        // Sway & strafe roll damping
        self.sway *= (-14.0 * dt).exp();
        let target_roll = strafe_input * 0.022;
        self.strafe_roll += (target_roll - self.strafe_roll) * (1.0 - (-12.0 * dt).exp());

        // Holster / ADS / Sprint blends
        let want_holster = if !self.equipped || holding_something {
            1.0
        } else {
            0.0
        };
        self.holster_t += (want_holster - self.holster_t) * (1.0 - (-14.0 * dt).exp());

        let can_ads =
            self.equipped && !holding_something && self.reloading.is_none() && self.holster_t < 0.2;
        let ads_target = if self.ads_held && can_ads { 1.0 } else { 0.0 };
        self.ads_t += (ads_target - self.ads_t) * (1.0 - (-16.0 * dt).exp());

        let want_sprint = if sprinting
            && player_speed > 4.2
            && grounded
            && self.ads_t < 0.2
            && self.reloading.is_none()
            && self.fire_anim.is_none()
            && self.inspect_timer <= 0.0
        {
            1.0
        } else {
            0.0
        };
        self.sprint_t += (want_sprint - self.sprint_t) * (1.0 - (-10.0 * dt).exp());

        if self.inspect_timer > 0.0 {
            self.inspect_timer = (self.inspect_timer - sim_dt).max(0.0);
        }

        // Animation timers
        let idle_id = self.model.anim_id("idle");
        let walk_id = self.model.anim_id("walk");
        let idle_dur = self.model.animations[idle_id].duration;
        let walk_dur = self.model.animations[walk_id].duration;
        self.idle_time = (self.idle_time + sim_dt) % idle_dur;

        let move_norm = if grounded {
            (player_speed / 4.2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.walk_blend += (move_norm - self.walk_blend) * (1.0 - (-10.0 * dt).exp());
        self.walk_time = (self.walk_time + sim_dt * (0.85 + 0.55 * move_norm)) % walk_dur;

        // Reload state machine
        if let Some((kind, mut t, mut refilled)) = self.reloading {
            let clip_id = match kind {
                ReloadKind::Easy => self.model.anim_id("reload_easy"),
                ReloadKind::Full => self.model.anim_id("reload_full"),
            };
            let dur = self.model.animations[clip_id].duration;
            t += sim_dt;
            let refill_at = match kind {
                ReloadKind::Easy => 0.78,
                ReloadKind::Full => 1.15,
            };
            if !refilled && t >= refill_at {
                refilled = true;
                self.ammo = MAG_CAPACITY + if self.chambered { 1 } else { 0 };
                self.chambered = true;
            }
            if t >= dur {
                self.reloading = None;
            } else {
                self.reloading = Some((kind, t, refilled));
            }
        }

        if let Some((fid, mut ft)) = self.fire_anim {
            ft += sim_dt;
            if ft >= self.model.animations[fid].duration {
                self.fire_anim = None;
            } else {
                self.fire_anim = Some((fid, ft));
            }
        }

        // Update particles & tracers
        self.update_vfx(audio, sim_dt);

        // Evaluate and upload skinned meshes when visible
        if self.holster_t < 0.99 {
            let mut primary = (idle_id, self.idle_time);
            let mut secondary: Option<(usize, f32, f32)> = if self.walk_blend > 0.01 {
                Some((
                    walk_id,
                    self.walk_time,
                    self.walk_blend * (1.0 - self.ads_t * 0.85),
                ))
            } else {
                None
            };

            if let Some((kind, rt, _)) = self.reloading {
                let rid = match kind {
                    ReloadKind::Easy => self.model.anim_id("reload_easy"),
                    ReloadKind::Full => self.model.anim_id("reload_full"),
                };
                primary = (rid, rt);
                secondary = None;
            } else if let Some((fid, ft)) = self.fire_anim {
                let f_dur = self.model.animations[fid].duration;
                let fade = if ft > f_dur - 0.06 {
                    ((f_dur - ft) / 0.06).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                secondary = Some((fid, ft, fade));
            }

            let slide_locked = self.ammo == 0 && self.reloading.is_none();
            self.model.evaluate_pose(
                primary.0,
                primary.1,
                secondary,
                slide_locked,
                &mut self.skin_mats,
            );
            for i in 0..self.model.submeshes.len() {
                self.model
                    .skin_submesh(i, &self.skin_mats, &mut self.scratch_mesh);
                renderer.update_mesh(self.submesh_ids[i], &self.scratch_mesh);
            }
        }
    }

    fn update_vfx(&mut self, audio: &Audio, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let g = Vec3::new(0.0, -9.80665, 0.0);
        for p in &mut self.particles {
            p.age += dt;
            match p.kind {
                ParticleKind::Dust => {
                    p.vel *= (-3.8 * dt).exp();
                    p.pos += p.vel * dt;
                    p.scale += Vec3::splat(dt * 1.8);
                }
                ParticleKind::Spark | ParticleKind::Chip | ParticleKind::WoodSplinter => {
                    p.vel += g * dt;
                    p.pos += p.vel * dt;
                    if p.pos.y < 0.01 && p.vel.y < 0.0 {
                        p.pos.y = 0.01;
                        p.vel.y = -p.vel.y * 0.42;
                        p.vel.x *= 0.72;
                        p.vel.z *= 0.72;
                    }
                    if p.kind == ParticleKind::Spark {
                        if p.vel.length_squared() > 0.01 {
                            p.rot = Quat::from_rotation_arc(Vec3::Z, p.vel.normalize());
                        }
                    } else {
                        let dq = Quat::from_scaled_axis(p.ang_vel * dt);
                        p.rot = (dq * p.rot).normalize();
                    }
                }
                ParticleKind::Casing => {
                    p.vel += g * dt;
                    p.pos += p.vel * dt;
                    if p.pos.y < 0.006 && p.vel.y < 0.0 {
                        p.pos.y = 0.006;
                        p.vel.y = -p.vel.y * 0.45;
                        p.vel.x *= 0.65;
                        p.vel.z *= 0.65;
                        p.ang_vel *= 0.55;
                        if !p.bounced {
                            p.bounced = true;
                            audio.play_pitched(
                                "shell_clink",
                                Some(p.pos),
                                0.34,
                                0.94 + (p.pos.x * 13.7).sin().abs() * 0.14,
                            );
                        }
                    }
                    let dq = Quat::from_scaled_axis(p.ang_vel * dt);
                    p.rot = (dq * p.rot).normalize();
                }
            }
        }
        self.particles.retain(|p| p.age < p.lifetime);

        for t in &mut self.tracers {
            t.age += dt;
        }
        self.tracers.retain(|t| t.age < t.lifetime);
    }

    /// Computes camera pitch/yaw/roll offsets from recoil, screen shake, and strafe tilt.
    pub fn camera_offsets(&self, time: f32) -> (f32, f32, f32) {
        let t2 = self.shake_trauma * self.shake_trauma;
        let shake_pitch = t2 * 0.022 * ((time * 57.0).sin() + 0.5 * (time * 113.0).cos());
        let shake_yaw = t2 * 0.018 * ((time * 63.0).cos() + 0.5 * (time * 97.0).sin());
        let shake_roll = t2 * 0.026 * (time * 47.0).sin();
        (
            self.recoil_pitch + shake_pitch,
            self.recoil_yaw + shake_yaw,
            shake_roll - self.strafe_roll * 0.45,
        )
    }

    pub fn fov_scale(&self) -> f32 {
        1.0 - self.ads_t * 0.18
    }

    fn aimed_forward(&self, camera: &Camera) -> Vec3 {
        let yaw = camera.yaw + self.recoil_yaw;
        let pitch = camera.pitch + self.recoil_pitch;
        void_engine::units::coord::forward_from_yaw_pitch(yaw, pitch)
    }

    fn viewmodel_world_matrix(&self, camera: &Camera) -> Mat4 {
        let (_, _, roll) = self.camera_offsets(0.0);
        let cam_world = Mat4::look_at_rh(
            camera.position,
            camera.position + camera.forward(),
            Quat::from_axis_angle(camera.forward(), camera.roll + roll) * Vec3::Y,
        )
        .inverse();

        // ADS aligns rear sight notch & front fiber-optic sight dead-center along (0, 0, -Z)
        let ads_smooth = self.ads_t * self.ads_t * (3.0 - 2.0 * self.ads_t);
        let ads_offset = Vec3::new(-0.04815, 0.0928, 0.085) * ads_smooth;
        let ads_rot = Quat::IDENTITY.slerp(
            Quat::from_xyzw(-0.008613, -0.04448, -0.121182, 0.991596).normalize(),
            ads_smooth,
        );

        let sprint_offset = Vec3::new(-0.045, -0.038, 0.025) * self.sprint_t;
        let sprint_rot = Quat::from_euler(
            glam::EulerRot::YXZ,
            0.38 * self.sprint_t,
            -0.20 * self.sprint_t,
            0.32 * self.sprint_t,
        );

        let holster_offset = Vec3::new(0.0, -0.34 * self.holster_t, 0.12 * self.holster_t);
        let holster_rot = Quat::from_rotation_x(-0.65 * self.holster_t);

        let inspect_s = if self.inspect_timer > 0.0 {
            let p = (2.1 - self.inspect_timer) / 2.1;
            (p * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        let inspect_offset = Vec3::new(-0.05, 0.02, 0.06) * inspect_s;
        let inspect_rot = Quat::from_euler(
            glam::EulerRot::YXZ,
            0.55 * inspect_s,
            0.12 * inspect_s,
            -0.42 * inspect_s,
        );

        let base_hip = Vec3::new(0.012, 0.016, -0.01) * (1.0 - ads_smooth);
        let proc_offset = base_hip
            + Vec3::new(
                self.sway.x * 0.25,
                self.sway.y * 0.25 - self.land_dip,
                self.kick_back,
            )
            + ads_offset
            + sprint_offset
            + holster_offset
            + inspect_offset;

        let proc_rot = ads_rot
            * sprint_rot
            * holster_rot
            * inspect_rot
            * Quat::from_euler(
                glam::EulerRot::YXZ,
                self.sway.x,
                self.sway.y + self.kick_back * 1.4,
                -self.strafe_roll,
            );

        cam_world * Mat4::from_rotation_translation(proc_rot, proc_offset)
    }

    pub fn append_draws(&self, scene: &Scene, camera: &Camera, draws: &mut Vec<Draw>) {
        // Bullet hole decals attached to their hit rigid bodies
        for hole in &self.bullet_holes {
            let Some(rb) = scene.physics.bodies.get(hole.body) else {
                continue;
            };
            let iso = rb.position();
            let b_pos = void_engine::physics::vec3(&iso.translation.vector);
            let b_rot = void_engine::physics::quaternion(&iso.rotation);
            let w_pos = b_pos + b_rot * hole.local_pos;
            let w_rot = b_rot * hole.local_rot;
            draws.push(Draw {
                mesh: self.decal_mesh,
                transform: Mat4::from_scale_rotation_translation(
                    Vec3::splat(hole.size),
                    w_rot,
                    w_pos,
                ),
                material: Material::new(hole.layer, [1.0, 1.0, 1.0], hole.roughness, 0.0),
                shadow: false,
                viewmodel: false,
            });
        }

        // Particles & brass casings
        for p in &self.particles {
            let mesh = match p.kind {
                ParticleKind::Spark => self.spark_mesh,
                ParticleKind::Dust => self.dust_mesh,
                ParticleKind::Chip | ParticleKind::WoodSplinter => self.chip_mesh,
                ParticleKind::Casing => self.casing_mesh,
            };
            let fade = if p.kind == ParticleKind::Casing {
                1.0
            } else {
                (1.0 - p.age / p.lifetime).clamp(0.0, 1.0)
            };
            let mut mat = p.material;
            if p.kind == ParticleKind::Spark {
                mat.emissive *= fade;
            }
            let scale = if p.kind == ParticleKind::Dust {
                p.scale * (0.4 + 0.6 * fade)
            } else {
                p.scale * (0.35 + 0.65 * fade)
            };
            draws.push(Draw {
                mesh,
                transform: Mat4::from_scale_rotation_translation(scale, p.rot, p.pos),
                material: mat,
                shadow: false,
                viewmodel: false,
            });
        }

        // Tracers
        for t in &self.tracers {
            let fade = (1.0 - t.age / t.lifetime).clamp(0.0, 1.0);
            draws.push(Draw {
                mesh: self.tracer_mesh,
                transform: t.transform,
                material: Material::glow([1.0, 0.84, 0.44], 9.0 * fade),
                shadow: false,
                viewmodel: false,
            });
        }

        // Muzzle flash sprite
        if self.muzzle_flash_timer > 0.0 {
            let fade = (self.muzzle_flash_timer / 0.045).clamp(0.0, 1.0);
            draws.push(Draw {
                mesh: self.flash_mesh,
                transform: Mat4::from_scale_rotation_translation(
                    Vec3::splat(self.muzzle_flash_scale * (0.65 + 0.35 * fade)),
                    self.muzzle_flash_rot,
                    self.muzzle_flash_pos,
                ),
                material: Material::glow([1.0, 0.72, 0.28], 16.0 * fade),
                shadow: false,
                viewmodel: false,
            });
        }

        // First-person pistol + arms viewmodel (rendered in isolated depth-cleared foreground pass)
        if self.holster_t < 0.99 {
            let vm_world = self.viewmodel_world_matrix(camera);
            for (i, &mesh_id) in self.submesh_ids.iter().enumerate() {
                draws.push(Draw {
                    mesh: mesh_id,
                    transform: vm_world,
                    material: self.submesh_materials[i],
                    shadow: false,
                    viewmodel: true,
                });
            }
        }
    }

    pub fn append_lights(&self, lights: &mut Vec<PointLight>) {
        if self.muzzle_flash_timer > 0.0 && lights.len() < void_engine::render::MAX_POINT_LIGHTS {
            let fade = (self.muzzle_flash_timer / 0.045).clamp(0.0, 1.0);
            lights.push(PointLight::new(
                self.muzzle_flash_pos,
                6.5,
                [1.0, 0.68, 0.28],
                22.0 * fade,
            ));
        }
        if self.impact_light_timer > 0.0 && lights.len() < void_engine::render::MAX_POINT_LIGHTS {
            let fade = (self.impact_light_timer / 0.05).clamp(0.0, 1.0);
            lights.push(PointLight::new(
                self.impact_light_pos,
                2.8,
                [1.0, 0.58, 0.18],
                10.0 * fade,
            ));
        }
    }

    pub fn hud(&self, speed: f32, grounded: bool) -> WeaponHud {
        let move_spread = (speed / 6.4).clamp(0.0, 1.0) * 8.0 + if grounded { 0.0 } else { 7.0 };
        let bloom_px = self.spread_bloom * 480.0;
        WeaponHud {
            equipped: self.equipped && self.holster_t < 0.5,
            ammo: self.ammo,
            chambered: self.chambered,
            reloading_progress: self.reloading.map(|(kind, t, _)| {
                let dur = match kind {
                    ReloadKind::Easy => 1.317,
                    ReloadKind::Full => 1.650,
                };
                (t / dur).clamp(0.0, 1.0)
            }),
            ads: self.ads_t,
            spread_px: move_spread + bloom_px,
            hitmarker: self.hitmarker,
            indoor_mix: self.indoor_mix,
        }
    }
}

/// Probes overhead ceiling & surrounding walls to classify indoor vs outdoor acoustics and wall slapback echoes.
fn probe_acoustics(
    physics: &Physics,
    player: RigidBodyHandle,
    eye: Vec3,
) -> (f32, Vec<(Vec3, f32, f32)>) {
    // 1) Straight up + 4 angled ceiling probes
    let mut ceiling_hits = 0.0f32;
    if let Some(hit) = physics.raycast_detailed(eye, Vec3::Y, 12.0, player) {
        if hit.dist < 5.0 {
            ceiling_hits += 1.0;
        } else {
            ceiling_hits += (1.0 - (hit.dist - 5.0) / 7.0).clamp(0.0, 1.0);
        }
    }
    let diag_dirs = [
        Vec3::new(0.55, 0.72, 0.42).normalize(),
        Vec3::new(-0.55, 0.72, 0.42).normalize(),
        Vec3::new(0.55, 0.72, -0.42).normalize(),
        Vec3::new(-0.55, 0.72, -0.42).normalize(),
    ];
    let mut diag_score = 0.0f32;
    for d in diag_dirs {
        if let Some(hit) = physics.raycast_detailed(eye, d, 10.0, player) {
            diag_score += 1.0 - (hit.dist / 10.0).clamp(0.0, 1.0);
        }
    }
    diag_score *= 0.25;

    // 2) Horizontal wall probes for enclosure & slapback echoes
    let horiz_dirs = [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z];
    let mut wall_close = 0.0f32;
    let mut echoes = Vec::new();
    for d in horiz_dirs {
        if let Some(hit) = physics.raycast_detailed(eye, d, 32.0, player) {
            wall_close += 1.0 - (hit.dist / 8.0).clamp(0.0, 1.0);
            if hit.dist >= 6.5 {
                let delay = (2.0 * hit.dist) / SPEED_OF_SOUND;
                let gain = 0.30 / (1.0 + hit.dist * 0.055);
                echoes.push((hit.point, delay, gain));
            }
        }
    }
    wall_close *= 0.25;

    let indoor_mix = (ceiling_hits * 0.62 + diag_score * 0.22 + wall_close * 0.28).clamp(0.0, 1.0);
    (indoor_mix, echoes)
}

fn next_u32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn rand01(state: &mut u32) -> f32 {
    (next_u32(state) & 0x00FF_FFFF) as f32 / 16_777_215.0
}

fn rand_s(state: &mut u32) -> f32 {
    rand01(state) * 2.0 - 1.0
}
