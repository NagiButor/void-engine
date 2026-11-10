mod rig;
mod settings;

use anyhow::{Context, Result};
use glam::{Mat4, Quat, Vec2, Vec3};
use settings::Settings;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
use void_engine::{
    audio::Audio,
    physics::{self, FIXED_DT},
    render::{Camera, Draw, Material, Renderer},
    scene::{Lab, PropKind},
};
use winit::{
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowBuilder},
};

#[derive(Default)]
struct Options {
    backend: String,
    adapter: Option<usize>,
    autostart: bool,
    showcase: bool,
    settings: bool,
    smoke: u32,
    benchmark: u32,
    screenshot: Option<PathBuf>,
    validation: bool,
    fullscreen: bool,
    selftest: bool,
}
impl Options {
    fn parse(s: &mut Settings) -> Result<Self> {
        let mut o = Self::default();
        for arg in std::env::args().skip(1) {
            if let Some(v) = arg.strip_prefix("--rhi=") {
                if !["dx12", "vulkan", "auto"].contains(&v) {
                    anyhow::bail!("Unknown backend {v}");
                }
                o.backend = v.to_owned();
            } else if let Some(v) = arg.strip_prefix("--adapter=") {
                o.adapter = Some(v.parse()?);
            } else if let Some(v) = arg.strip_prefix("--width=") {
                s.width = v.parse::<u32>()?.clamp(640, 3840);
            } else if let Some(v) = arg.strip_prefix("--height=") {
                s.height = v.parse::<u32>()?.clamp(360, 2160);
            } else if let Some(v) = arg.strip_prefix("--smoke=") {
                o.smoke = v.parse()?;
            } else if let Some(v) = arg.strip_prefix("--benchmark=") {
                o.benchmark = v.parse()?;
                o.autostart = true;
            } else if let Some(v) = arg.strip_prefix("--screenshot=") {
                o.screenshot = Some(v.into());
            } else {
                match arg.as_str() {
                    "--autostart" => o.autostart = true,
                    "--showcase" => {
                        o.showcase = true;
                        o.autostart = true;
                    }
                    "--settings" => o.settings = true,
                    "--novsync" => s.vsync = false,
                    "--fullscreen" => o.fullscreen = true,
                    "--selftest" => o.selftest = true,
                    "--validation" => o.validation = true,
                    "--no-prompt" => {}
                    "--help" | "-h" => {
                        println!("V.O.I.D. Rust Physics Lab\n--rhi=dx12|vulkan|auto --adapter=N --autostart --showcase --settings\n--width=1600 --height=900 --novsync --fullscreen --validation\n--smoke=N --benchmark=N --screenshot=path.png --selftest");
                        std::process::exit(0);
                    }
                    _ => anyhow::bail!("Unknown option {arg}"),
                }
            }
        }
        Ok(o)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Menu,
    Playing,
    Paused,
    Settings,
}
struct Spark {
    p: Vec3,
    v: Vec3,
    life: f32,
}
struct Game {
    window: Arc<Window>,
    renderer: Renderer,
    lab: Lab,
    rig: rig::Rig,
    audio: Audio,
    camera: Camera,
    options: Options,
    settings: Settings,
    state: State,
    return_state: State,
    keys: HashSet<KeyCode>,
    look: Vec2,
    held: Option<rapier_handle::Handle>,
    hover: Option<rapier_handle::Handle>,
    hold_distance: f32,
    slow: bool,
    accumulator: f32,
    time: f32,
    last: Instant,
    frames: u32,
    frame_times: Vec<f32>,
    fps: f32,
    egui: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    sparks: Vec<Spark>,
    impact_cooldown: f32,
    footstep: f32,
    toast: String,
    toast_timer: f32,
    keep_held: bool,
    exit: bool,
    focused: bool,
}
// Re-export the public handle through the engine so the game does not depend on physics internals.
mod rapier_handle {
    pub type Handle = void_engine::scene::BodyHandle;
}

impl Game {
    async fn new(window: Arc<Window>, settings: Settings, options: Options) -> Result<Self> {
        let assets = void_engine::asset_dir();
        let mut renderer = Renderer::new(
            window.clone(),
            &options.backend,
            options.adapter,
            &assets,
            settings.vsync,
            options.validation,
        )
        .await?;
        let lab = Lab::new(&mut renderer)?;
        let rig = rig::Rig::new(&mut renderer);
        let audio = Audio::new(&assets);
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Roboto".into(),
            egui::FontData::from_owned(std::fs::read(assets.join("fonts/Roboto-Regular.ttf"))?),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "Roboto".into());
        ctx.set_fonts(fonts);
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(176, 111, 37);
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        ctx.set_style(style);
        let egui_state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&renderer.device, renderer.config.format, None, 1);
        let mut g = Self {
            window,
            renderer,
            lab,
            rig,
            audio,
            camera: Camera::default(),
            options,
            settings,
            state: State::Menu,
            return_state: State::Menu,
            keys: HashSet::new(),
            look: Vec2::ZERO,
            held: None,
            hover: None,
            hold_distance: 2.5,
            slow: false,
            accumulator: 0.0,
            time: 0.0,
            last: Instant::now(),
            frames: 0,
            frame_times: Vec::new(),
            fps: 60.0,
            egui: ctx,
            egui_state,
            egui_renderer,
            sparks: Vec::new(),
            impact_cooldown: 0.0,
            footstep: 0.0,
            toast: "СЕКТОР 07 // СИСТЕМЫ ГОТОВЫ".into(),
            toast_timer: 3.0,
            keep_held: false,
            exit: false,
            focused: true,
        };
        if g.options.autostart {
            g.start();
        }
        if g.options.showcase {
            g.showcase();
        }
        if g.options.settings {
            g.open_settings();
        }
        Ok(g)
    }
    fn capture_mouse(&self, on: bool) {
        if self.options.smoke > 0 || self.options.benchmark > 0 {
            return;
        }
        if on {
            if self.window.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                let _ = self.window.set_cursor_grab(CursorGrabMode::Confined);
            }
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.window.set_cursor_visible(!on);
    }
    fn start(&mut self) {
        self.state = State::Playing;
        self.camera = Camera {
            fov: self.settings.fov,
            ..Default::default()
        };
        self.capture_mouse(true);
        self.accumulator = 0.0;
    }
    fn pause(&mut self) {
        if self.state == State::Playing {
            self.drop(false);
            self.state = State::Paused;
            self.capture_mouse(false);
            self.keys.clear();
            self.look = Vec2::ZERO;
        }
    }
    fn open_settings(&mut self) {
        self.return_state = self.state;
        if self.state == State::Playing {
            self.pause();
        }
        self.state = State::Settings;
        self.capture_mouse(false);
    }
    fn close_settings(&mut self) {
        self.settings.save();
        self.state = self.return_state;
        self.capture_mouse(self.state == State::Playing);
    }
    fn reset(&mut self) {
        self.drop(false);
        self.lab.reset();
        self.camera = Camera {
            fov: self.settings.fov,
            ..Default::default()
        };
        self.keep_held = false;
        self.sparks.clear();
        self.toast = "СЦЕНА ВОССТАНОВЛЕНА".into();
        self.toast_timer = 2.0;
    }
    fn showcase(&mut self) {
        self.lab.physics.bodies[self.lab.player]
            .set_translation(physics::vector(Vec3::new(3.0, 0.81, 6.8)), true);
        self.camera.position = Vec3::new(3.0, 1.66, 6.8);
        self.camera.pitch = -0.23;
        self.lab.physics.refresh_queries();
        self.update_hover();
        self.grab();
        self.keep_held = true;
    }
    fn update_hover(&mut self) {
        self.hover = self
            .lab
            .physics
            .raycast(
                self.camera.position,
                self.camera.forward(),
                7.0,
                self.lab.player,
                false,
            )
            .map(|(h, _)| h)
            .filter(|h| self.lab.props.iter().any(|p| p.body == *h));
    }
    fn burst(&mut self, p: Vec3) {
        for i in 0..18 {
            let a = i as f32 * 2.39996 + self.time;
            let v = Vec3::new(a.cos(), 0.3 + (i % 4) as f32 * 0.25, a.sin())
                * (0.6 + (i % 5) as f32 * 0.3);
            self.sparks.push(Spark {
                p,
                v,
                life: 0.35 + (i % 4) as f32 * 0.07,
            });
        }
        if self.sparks.len() > 150 {
            self.sparks.drain(..self.sparks.len() - 150);
        }
    }
    fn grab(&mut self) {
        if self.held.is_some() {
            return;
        }
        if let Some(h) = self.hover {
            self.held = Some(h);
            self.hold_distance = (physics::vec3(self.lab.physics.bodies[h].translation())
                - self.camera.position)
                .length()
                .clamp(1.4, 5.0);
            self.lab.physics.hold_filter(h, true);
            self.audio.play("grab", None, 0.65);
            let p = physics::vec3(self.lab.physics.bodies[h].translation());
            self.burst(p);
        }
    }
    fn drop(&mut self, throw: bool) {
        if let Some(h) = self.held.take() {
            self.lab.physics.release(
                h,
                if throw {
                    Some(self.camera.forward() * 17.0 + Vec3::Y * 1.6)
                } else {
                    None
                },
            );
            if throw {
                self.rig.recoil();
                self.audio.play("throw", None, 0.8);
                let p = physics::vec3(self.lab.physics.bodies[h].translation());
                self.burst(p);
            }
        }
    }
    fn shove(&mut self) {
        if self.held.is_some() {
            self.drop(true);
        } else if let Some(h) = self.hover {
            let b = &mut self.lab.physics.bodies[h];
            b.apply_impulse(
                physics::vector(self.camera.forward() * b.mass() * 8.0),
                true,
            );
            self.rig.recoil();
            self.audio.play("throw", None, 0.5);
        }
    }
    fn spawn(&mut self, kind: PropKind) {
        let spawned = self.lab.props.iter().filter(|p| !p.initial).count();
        if spawned >= 34 {
            if let Some(i) = self
                .lab
                .props
                .iter()
                .position(|p| !p.initial && Some(p.body) != self.held)
            {
                let p = self.lab.props.remove(i);
                self.lab.physics.remove(p.body);
            }
        }
        let ray = self.lab.physics.raycast(
            self.camera.position,
            self.camera.forward(),
            3.5,
            self.lab.player,
            false,
        );
        let distance = ray.map(|(_, d)| (d - 0.85).max(1.3)).unwrap_or(3.5);
        let p = self.camera.position + self.camera.forward() * distance + Vec3::Y * 0.25;
        self.lab
            .spawn(kind, p, Quat::from_rotation_y(self.camera.yaw), false);
        self.audio.play("spawn", Some(p), 0.5);
        self.burst(p);
        self.toast = format!("{} // СОЗДАН", kind.name().to_uppercase());
        self.toast_timer = 1.6;
    }
    fn key(&mut self, key: KeyCode, down: bool, repeat: bool) {
        if down {
            self.keys.insert(key);
        } else {
            self.keys.remove(&key);
        }
        if !down || repeat {
            return;
        }
        if key == KeyCode::Escape {
            match self.state {
                State::Playing => self.pause(),
                State::Paused => {
                    self.state = State::Playing;
                    self.capture_mouse(true);
                }
                State::Settings => self.close_settings(),
                State::Menu => self.exit = true,
            }
            return;
        }
        if key == KeyCode::F3 {
            if self.state == State::Settings {
                self.close_settings();
            } else {
                self.open_settings();
            }
            return;
        }
        if self.state == State::Menu && key == KeyCode::Enter {
            self.start();
            return;
        }
        if self.state != State::Playing {
            return;
        }
        match key {
            KeyCode::KeyF => self.drop(true),
            KeyCode::KeyR => self.reset(),
            KeyCode::KeyQ => {
                self.slow = !self.slow;
                self.audio.play(
                    if self.slow { "slowmo_on" } else { "slowmo_off" },
                    None,
                    0.5,
                );
            }
            KeyCode::KeyM => self.settings.muted = !self.settings.muted,
            KeyCode::KeyG => self.spawn(PropKind::ALL[(self.frames as usize * 7) % 10]),
            KeyCode::Digit1 => self.spawn(PropKind::Crate),
            KeyCode::Digit2 => self.spawn(PropKind::Barrel),
            KeyCode::Digit3 => self.spawn(PropKind::Sphere),
            KeyCode::Digit4 => self.spawn(PropKind::Plank),
            KeyCode::Digit5 => self.spawn(PropKind::Brick),
            KeyCode::Digit6 => self.spawn(PropKind::Can),
            KeyCode::Digit7 => self.spawn(PropKind::Domino),
            KeyCode::BracketLeft => self.settings.fov = (self.settings.fov - 1.0).max(65.0),
            KeyCode::BracketRight => self.settings.fov = (self.settings.fov + 1.0).min(110.0),
            KeyCode::Comma => {
                self.settings.eye_height = (self.settings.eye_height - 0.01).max(0.70)
            }
            KeyCode::Period => {
                self.settings.eye_height = (self.settings.eye_height + 0.01).min(1.0)
            }
            _ => {}
        }
    }
    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.toast_timer -= dt;
        self.camera.fov = self.settings.fov;
        self.audio.settings(
            if self.settings.muted {
                0.0
            } else {
                self.settings.volume
            },
            self.settings.hdr_audio,
            self.slow,
        );
        if self.state == State::Playing {
            self.camera.yaw -= self.look.x * 0.002 * self.settings.sensitivity;
            self.camera.pitch = (self.camera.pitch
                - self.look.y * 0.002 * self.settings.sensitivity)
                .clamp(-1.48, 1.48);
            let mut wish = Vec3::ZERO;
            let forward = Vec3::new(-self.camera.yaw.sin(), 0.0, -self.camera.yaw.cos());
            let right = self.camera.right();
            for (key, direction) in [
                (KeyCode::KeyW, forward),
                (KeyCode::KeyS, -forward),
                (KeyCode::KeyD, right),
                (KeyCode::KeyA, -right),
            ] {
                if self.keys.contains(&key) {
                    wish += direction;
                }
            }
            let speed = if self.keys.contains(&KeyCode::ShiftLeft) {
                7.0
            } else {
                4.2
            };
            let velocity = wish.normalize_or_zero() * speed;
            let p = physics::vec3(self.lab.physics.bodies[self.lab.player].translation());
            let grounded = self
                .lab
                .physics
                .raycast(p, -Vec3::Y, 0.87, self.lab.player, false)
                .is_some();
            let b = &mut self.lab.physics.bodies[self.lab.player];
            let mut v = physics::vec3(b.linvel());
            let blend = 1.0 - (-if grounded { 22.0 } else { 6.0 } * dt).exp();
            v.x += (velocity.x - v.x) * blend;
            v.z += (velocity.z - v.z) * blend;
            if grounded && self.keys.remove(&KeyCode::Space) {
                v.y = 5.0;
            }
            b.set_linvel(physics::vector(v), true);
            self.accumulator += (dt * if self.slow { 0.28 } else { 1.0 }).min(0.1);
            while self.accumulator >= FIXED_DT {
                if let Some(h) = self.held {
                    self.lab.physics.spring(
                        h,
                        self.camera.position + self.camera.forward() * self.hold_distance,
                    );
                }
                self.lab.physics.step(FIXED_DT);
                self.accumulator -= FIXED_DT;
                self.impact_cooldown -= FIXED_DT;
                if self.impact_cooldown <= 0.0 {
                    if let Some(&(tag, p, impulse)) = self
                        .lab
                        .physics
                        .impacts
                        .iter()
                        .max_by(|a, b| a.2.total_cmp(&b.2))
                    {
                        if let Some(prop) = self
                            .lab
                            .props
                            .iter()
                            .find(|prop| self.lab.physics.bodies[prop.body].user_data == tag)
                        {
                            self.audio.play(
                                prop.kind.sound(),
                                Some(p),
                                (impulse / 120.0).clamp(0.03, 0.75),
                            );
                            self.impact_cooldown = 0.07;
                        }
                    }
                }
            }
            let p = physics::vec3(self.lab.physics.bodies[self.lab.player].translation());
            if p.y < -10.0 {
                self.reset();
            }
            self.camera.position =
                p + Vec3::Y * self.settings.eye_height + right * self.settings.camera_side;
            if self.settings.head_bob && grounded && wish.length_squared() > 0.0 {
                self.camera.position.y += (self.time * speed * 2.6).sin() * 0.006;
            }
            self.update_hover();
            self.rig.update(
                dt,
                self.time,
                self.look,
                v.xz_length(),
                self.held.is_some(),
                self.settings.head_bob,
            );
            self.footstep -= dt;
            if grounded && v.xz_length() > 1.0 && self.footstep <= 0.0 {
                self.audio.play(
                    if self.frames.is_multiple_of(2) {
                        "footstep_1"
                    } else {
                        "footstep_2"
                    },
                    Some(p - Vec3::Y * 0.8),
                    0.25,
                );
                self.footstep = 2.0 / v.xz_length();
            }
        } else if self.state == State::Menu {
            self.camera.position = Vec3::new(7.5 + (self.time * 0.08).sin() * 0.3, 3.5, 10.8);
            let dir = (Vec3::new(0.0, 1.0, -2.0) - self.camera.position).normalize();
            self.camera.yaw = (-dir.x).atan2(-dir.z);
            self.camera.pitch = dir.y.asin();
            self.camera.fov = 66.0;
        }
        for spark in &mut self.sparks {
            spark.life -= dt;
            spark.v.y -= 6.0 * dt;
            spark.p += spark.v * dt;
        }
        self.sparks.retain(|s| s.life > 0.0);
        self.audio.listener = self.camera.position;
        self.audio.right = self.camera.right();
        self.look = Vec2::ZERO;
    }
    fn scene_draws(&self) -> Vec<Draw> {
        let mut draws = self.lab.draws();
        if self.state == State::Playing {
            self.rig.draw(&mut draws, &self.camera, self.lab.tool);
            if let Some(h) = self.held {
                let end = physics::vec3(self.lab.physics.bodies[h].translation());
                let start = self.rig.emitter(&self.camera);
                let delta = end - start;
                for i in 0..14 {
                    let t0 = i as f32 / 14.0;
                    let t1 = (i + 1) as f32 / 14.0;
                    let wave = |t: f32| {
                        self.camera.right()
                            * (t * std::f32::consts::PI).sin()
                            * (t * 28.0 - self.time * 18.0).sin()
                            * 0.014
                    };
                    let a = start + delta * t0 + wave(t0);
                    let b = start + delta * t1 + wave(t1);
                    let d = b - a;
                    let mut draw = Draw::new(
                        self.lab.cylinder,
                        Mat4::from_scale_rotation_translation(
                            Vec3::new(0.006, d.length(), 0.006),
                            Quat::from_rotation_arc(Vec3::Y, d.normalize()),
                            (a + b) * 0.5,
                        ),
                        Material::glow([1.0, 0.48, 0.11], 5.5),
                    );
                    draw.shadow = false;
                    draws.push(draw);
                }
                for i in 0..8 {
                    let angle = self.time * 1.2 + i as f32 * std::f32::consts::TAU / 8.0;
                    let p = end + Vec3::new(angle.cos() * 0.58, angle.sin() * 0.58, 0.0);
                    let mut d = Draw::new(
                        self.lab.sphere,
                        Mat4::from_scale_rotation_translation(
                            Vec3::splat(0.012),
                            Quat::IDENTITY,
                            p,
                        ),
                        Material::glow([1.0, 0.52, 0.16], 3.5),
                    );
                    d.shadow = false;
                    draws.push(d);
                }
            }
        }
        for s in &self.sparks {
            let mut d = Draw::new(
                self.lab.sphere,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.008 * s.life.min(0.2) / 0.2),
                    Quat::IDENTITY,
                    s.p,
                ),
                Material::glow([1.0, 0.55, 0.17], 4.0),
            );
            d.shadow = false;
            draws.push(d);
        }
        draws
    }
    fn ui(&mut self, ctx: &egui::Context) {
        let amber = egui::Color32::from_rgb(229, 167, 76);
        let muted = egui::Color32::from_gray(175);
        let rect = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("world labels"),
        ));
        // Text decal projected from its real wall position.
        let vp = Mat4::perspective_rh(
            self.camera.fov.to_radians(),
            rect.width() / rect.height(),
            0.06,
            120.0,
        ) * self.camera.view();
        let p = vp * Vec3::new(0.5, 4.7, -13.52).extend(1.0);
        if p.w > 0.1 {
            let ndc = p.truncate() / p.w;
            let pos = rect.min
                + egui::vec2(
                    (ndc.x * 0.5 + 0.5) * rect.width(),
                    (-ndc.y * 0.5 + 0.5) * rect.height(),
                );
            let size = (rect.height() * 1.8 / p.w).clamp(9.0, 80.0);
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                "V.O.I.D.  /  PHYSICS LAB",
                egui::FontId::proportional(size * 0.32),
                amber,
            );
            painter.text(
                pos + egui::vec2(0.0, size * 0.42),
                egui::Align2::CENTER_CENTER,
                "SECTOR 07  ·  INTERACTIVE DYNAMICS",
                egui::FontId::proportional(size * 0.15),
                muted,
            );
        }
        egui::Area::new("identity".into())
            .fixed_pos(egui::pos2(24.0, 20.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("V.O.I.D.").size(22.0).color(amber));
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("PHYSICS LAB").size(14.0));
                        ui.label(
                            egui::RichText::new("RUST RUNTIME / SECTOR 07")
                                .size(10.0)
                                .color(muted),
                        );
                    });
                });
            });
        egui::Area::new("stats".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-24.0, 20.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "{:.0} FPS   /   {} ОБЪЕКТОВ",
                    self.fps,
                    self.lab.props.len()
                ));
                ui.label(
                    egui::RichText::new(&self.renderer.adapter_name)
                        .size(10.0)
                        .color(muted),
                );
                let (voices, gain) = self.audio.stats();
                ui.label(
                    egui::RichText::new(format!("HDR {gain:+.1} dB · {voices} voices"))
                        .size(10.0)
                        .color(muted),
                );
            });
        match self.state {
            State::Menu | State::Paused => {
                let menu = self.state == State::Menu;
                egui::Window::new(if menu {
                    "V.O.I.D. / PHYSICS LAB"
                } else {
                    "ПАУЗА"
                })
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::LEFT_CENTER, egui::vec2(72.0, 0.0))
                .fixed_size(egui::vec2(340.0, 310.0))
                .frame(
                    egui::Frame::window(&ctx.style())
                        .fill(egui::Color32::from_black_alpha(220))
                        .inner_margin(28.0),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("VIRTUAL OBJECTS &\nINTERACTIVE DYNAMICS")
                            .size(13.0)
                            .color(amber),
                    );
                    ui.add_space(16.0);
                    ui.heading(if menu {
                        "ФИЗИЧЕСКАЯ ЛАБОРАТОРИЯ"
                    } else {
                        "СИМУЛЯЦИЯ ПРИОСТАНОВЛЕНА"
                    });
                    ui.label("Нативный Rust · PBR · Rapier 3D");
                    ui.add_space(22.0);
                    if ui
                        .add_sized(
                            [280.0, 40.0],
                            egui::Button::new(if menu {
                                "НАЧАТЬ ЭКСПЕРИМЕНТ"
                            } else {
                                "ПРОДОЛЖИТЬ"
                            }),
                        )
                        .clicked()
                    {
                        if menu {
                            self.start();
                        } else {
                            self.state = State::Playing;
                            self.capture_mouse(true);
                        }
                    }
                    if ui
                        .add_sized([280.0, 36.0], egui::Button::new("НАСТРОЙКИ"))
                        .clicked()
                    {
                        self.open_settings();
                    }
                    if ui
                        .add_sized([280.0, 36.0], egui::Button::new("ВЫХОД"))
                        .clicked()
                    {
                        self.exit = true;
                    }
                });
            }
            State::Playing => {
                let p = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    "crosshair".into(),
                ));
                let center = rect.center();
                let color = if self.hover.is_some() || self.held.is_some() {
                    amber
                } else {
                    egui::Color32::from_white_alpha(180)
                };
                for (a, b) in [
                    (egui::vec2(-7.0, 0.0), egui::vec2(-3.0, 0.0)),
                    (egui::vec2(3.0, 0.0), egui::vec2(7.0, 0.0)),
                    (egui::vec2(0.0, -7.0), egui::vec2(0.0, -3.0)),
                    (egui::vec2(0.0, 3.0), egui::vec2(0.0, 7.0)),
                ] {
                    p.line_segment([center + a, center + b], egui::Stroke::new(1.2_f32, color));
                }
                if let Some(h) = self.held.or(self.hover) {
                    if let Some(prop) = self.lab.props.iter().find(|p| p.body == h) {
                        p.text(
                            center + egui::vec2(0.0, 24.0),
                            egui::Align2::CENTER_TOP,
                            format!("{}  /  {:.1} кг", prop.kind.name(), prop.kind.mass()),
                            egui::FontId::proportional(13.0),
                            color,
                        );
                    }
                }
                egui::Area::new("controls".into()).anchor(egui::Align2::LEFT_BOTTOM,egui::vec2(24.0,-24.0)).show(ctx,|ui|{ui.label(egui::RichText::new("WASD  движение     SHIFT  бег     SPACE  прыжок\nЛКМ  захват     ПКМ / F  бросок     КОЛЕСО  дистанция\nR  сброс     Q  замедление     F3  настройки").size(11.0).color(egui::Color32::from_white_alpha(190)));});
                egui::Area::new("hotbar".into())
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            for (i, kind) in PropKind::ALL[..7].iter().enumerate() {
                                egui::Frame::none()
                                    .fill(egui::Color32::from_black_alpha(160))
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} {}",
                                                i + 1,
                                                kind.name()
                                            ))
                                            .size(11.0)
                                            .color(amber),
                                        );
                                    });
                            }
                        });
                    });
                if self.toast_timer > 0.0 {
                    egui::Area::new("toast".into())
                        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -70.0))
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .fill(egui::Color32::from_black_alpha(175))
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    ui.label(&self.toast);
                                });
                        });
                }
                if self.slow {
                    p.text(
                        egui::pos2(rect.center().x, 28.0),
                        egui::Align2::CENTER_TOP,
                        "ЗАМЕДЛЕНИЕ ×0.28",
                        egui::FontId::proportional(14.0),
                        amber,
                    );
                }
            }
            State::Settings => {
                self.settings_ui(ctx);
            }
        }
    }
    fn settings_ui(&mut self, ctx: &egui::Context) {
        let old_vsync = self.settings.vsync;
        let old_size = (self.settings.width, self.settings.height);
        egui::Window::new("НАСТРОЙКИ / V.O.I.D.")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(590.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ctx.screen_rect().height() - 140.0)
                    .show(ui, |ui| {
                        ui.heading("Графика");
                        egui::ComboBox::from_label("Разрешение")
                            .selected_text(format!(
                                "{} × {}",
                                self.settings.width, self.settings.height
                            ))
                            .show_ui(ui, |ui| {
                                for (w, h) in [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)]
                                {
                                    if ui
                                        .selectable_label(
                                            (w, h) == (self.settings.width, self.settings.height),
                                            format!("{w} × {h}"),
                                        )
                                        .clicked()
                                    {
                                        self.settings.width = w;
                                        self.settings.height = h;
                                    }
                                }
                            });
                        ui.checkbox(&mut self.settings.vsync, "Вертикальная синхронизация");
                        egui::ComboBox::from_label("Тени")
                            .selected_text(
                                ["Выкл", "Жёсткие", "Мягкие PCF"][self.settings.shadows as usize],
                            )
                            .show_ui(ui, |ui| {
                                for (i, name) in
                                    ["Выкл", "Жёсткие", "Мягкие PCF"].into_iter().enumerate()
                                {
                                    ui.selectable_value(&mut self.settings.shadows, i as u32, name);
                                }
                            });
                        ui.checkbox(&mut self.settings.ao, "Контактное затенение (SSAO)");
                        ui.checkbox(&mut self.settings.bloom, "Свечение (bloom)");
                        ui.add(
                            egui::Slider::new(&mut self.settings.exposure, 0.5..=1.6)
                                .text("Экспозиция"),
                        );
                        ui.add(egui::Slider::new(&mut self.settings.fog, 0.0..=2.0).text("Туман"));
                        ui.separator();
                        ui.heading("Камера");
                        ui.add(
                            egui::Slider::new(&mut self.settings.fov, 65.0..=110.0)
                                .text("FOV мира, °"),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Оружие: отдельная проекция {:.0}° · длина {:.0} см",
                                rig::VIEW_FOV,
                                rig::TOOL_LENGTH * 100.0
                            ))
                            .small(),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.settings.eye_height, 0.70..=1.0)
                                .text("Высота глаз над центром капсулы"),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.settings.camera_side, -0.2..=0.2)
                                .text("Смещение камеры"),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.settings.sensitivity, 0.2..=3.0)
                                .text("Чувствительность"),
                        );
                        ui.checkbox(&mut self.settings.head_bob, "Покачивание при ходьбе");
                        ui.separator();
                        ui.heading("Звук");
                        ui.add(
                            egui::Slider::new(&mut self.settings.volume, 0.0..=1.0)
                                .text("Громкость"),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.settings.hdr_audio, 0.0..=1.0)
                                .text("HDR-аудио"),
                        );
                        ui.checkbox(&mut self.settings.muted, "Без звука");
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("СОХРАНИТЬ И НАЗАД").clicked() {
                                self.close_settings();
                            }
                            if ui.button("СБРОСИТЬ НАСТРОЙКИ").clicked() {
                                self.settings = Settings::default();
                            }
                        });
                    });
            });
        if self.settings.vsync != old_vsync {
            self.renderer.vsync(self.settings.vsync);
        }
        if old_size != (self.settings.width, self.settings.height) {
            let _ = self
                .window
                .request_inner_size(PhysicalSize::new(self.settings.width, self.settings.height));
        }
    }
    fn frame(&mut self) -> Result<()> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let now = Instant::now();
        let raw = now.duration_since(self.last).as_secs_f32();
        self.last = now;
        self.fps += (1.0 / raw.max(0.0001) - self.fps) * 0.04;
        self.update(if self.options.smoke > 0 {
            FIXED_DT
        } else {
            raw.min(0.05)
        });
        let input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui.clone();
        let output = ctx.run(input, |ctx| self.ui(ctx));
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        for (id, delta) in &output.textures_delta.set {
            self.egui_renderer.update_texture(
                &self.renderer.device,
                &self.renderer.queue,
                *id,
                delta,
            );
        }
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.renderer.config.width, self.renderer.config.height],
            pixels_per_point: output.pixels_per_point,
        };
        let frame = match self.renderer.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.renderer
                    .resize(self.renderer.config.width, self.renderer.config.height);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder =
            self.renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("V.O.I.D. frame"),
                });
        let command_buffers = self.egui_renderer.update_buffers(
            &self.renderer.device,
            &self.renderer.queue,
            &mut encoder,
            &primitives,
            &screen,
        );
        let draws = self.scene_draws();
        self.renderer.encode(
            &mut encoder,
            &view,
            &self.camera,
            &draws,
            &self.settings.rendering(),
            self.time,
        );
        self.render_ui(&mut encoder, &view, &primitives, &screen);
        self.renderer.queue.submit(
            command_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        frame.present();
        for id in &output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        self.frames += 1;
        if self.frames > 30 {
            self.frame_times.push(raw * 1000.0);
        }
        let done = (self.options.smoke > 0 && self.frames >= self.options.smoke)
            || (self.options.benchmark > 0
                && self.frame_times.len() >= self.options.benchmark as usize);
        if done {
            if let Some(path) = self.options.screenshot.clone() {
                self.screenshot(&path, &draws, &primitives, &screen)?;
            }
            if self.options.benchmark > 0 {
                let mean = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
                self.frame_times.sort_by(f32::total_cmp);
                let p95 = self.frame_times[(self.frame_times.len() as f32 * 0.95) as usize];
                let worst = &self.frame_times[(self.frame_times.len() as f32 * 0.99) as usize..];
                let low = 1000.0 / (worst.iter().sum::<f32>() / worst.len() as f32);
                println!(
                    "BENCHMARK {}: {:.1} FPS, 1% low {:.1}, p95 {:.2} ms / {} draws",
                    self.renderer.adapter_name,
                    1000.0 / mean,
                    low,
                    p95,
                    draws.len()
                );
            }
            self.exit = true;
        }
        Ok(())
    }
    fn render_ui(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        primitives: &[egui::ClippedPrimitive],
        screen: &egui_wgpu::ScreenDescriptor,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UI"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        self.egui_renderer.render(&mut pass, primitives, screen);
    }
    fn screenshot(
        &mut self,
        path: &std::path::Path,
        draws: &[Draw],
        primitives: &[egui::ClippedPrimitive],
        screen: &egui_wgpu::ScreenDescriptor,
    ) -> Result<()> {
        let w = self.renderer.config.width;
        let h = self.renderer.config.height;
        let tex = self
            .renderer
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Screenshot"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.renderer.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
        let view = tex.create_view(&Default::default());
        let stride = (w * 4 + 255) & !255;
        let buffer = self.renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback"),
            size: (stride * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .renderer
            .device
            .create_command_encoder(&Default::default());
        self.renderer.encode(
            &mut encoder,
            &view,
            &self.camera,
            draws,
            &self.settings.rendering(),
            self.time,
        );
        self.render_ui(&mut encoder, &view, primitives, screen);
        encoder.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.renderer.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.renderer.device.poll(wgpu::Maintain::Wait);
        rx.recv()??;
        let data = buffer.slice(..).get_mapped_range();
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        let bgra = matches!(
            self.renderer.config.format,
            wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm
        );
        for row in data.chunks(stride as usize) {
            for pixel in row[..(w * 4) as usize].chunks(4) {
                if bgra {
                    rgba.extend([pixel[2], pixel[1], pixel[0], 255]);
                } else {
                    rgba.extend_from_slice(pixel);
                }
            }
        }
        drop(data);
        buffer.unmap();
        void_engine::save_rgba(path, w, h, &rgba)?;
        println!("Screenshot: {}", path.display());
        Ok(())
    }
}

trait HorizontalSpeed {
    fn xz_length(self) -> f32;
}
impl HorizontalSpeed for Vec3 {
    fn xz_length(self) -> f32 {
        (self.x * self.x + self.z * self.z).sqrt()
    }
}

fn main() -> Result<()> {
    let mut settings = Settings::load();
    let options = Options::parse(&mut settings)?;
    if options.selftest {
        return selftest();
    }
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("V.O.I.D. Engine · Rust · Physics Lab")
            .with_inner_size(PhysicalSize::new(settings.width, settings.height))
            .build(&event_loop)?,
    );
    if options.fullscreen {
        window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
    }
    let mut game = pollster_main(window, settings, options)?;
    let failed = Arc::new(AtomicBool::new(false));
    let runtime_failed = failed.clone();
    event_loop.run(move |event, target| {
        match event {
            Event::WindowEvent { event, .. } => {
                let consumed = game
                    .egui_state
                    .on_window_event(&game.window, &event)
                    .consumed;
                match event {
                    WindowEvent::CloseRequested => game.exit = true,
                    WindowEvent::Resized(size) => game.renderer.resize(size.width, size.height),
                    WindowEvent::Focused(focused) => {
                        game.focused = focused;
                        if !focused && game.options.smoke == 0 && game.options.benchmark == 0 {
                            game.pause();
                            game.keys.clear();
                            game.drop(false);
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            if !consumed || code == KeyCode::Escape || code == KeyCode::F3 {
                                game.key(code, event.state == ElementState::Pressed, event.repeat);
                            }
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if game.state == State::Playing {
                            match (button, state) {
                                (MouseButton::Left, ElementState::Pressed) => game.grab(),
                                (MouseButton::Left, ElementState::Released) => {
                                    if !game.keep_held {
                                        game.drop(false)
                                    }
                                }
                                (MouseButton::Right, ElementState::Pressed) => game.shove(),
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        if game.state == State::Playing {
                            let amount = match delta {
                                MouseScrollDelta::LineDelta(_, y) => y,
                                MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.02,
                            };
                            game.hold_distance =
                                (game.hold_distance + amount * 0.25).clamp(1.3, 6.0);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let Err(e) = game.frame() {
                            eprintln!("Runtime error: {e:#}");
                            runtime_failed.store(true, Ordering::Relaxed);
                            game.exit = true;
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta },
                ..
            } => {
                if game.state == State::Playing && game.focused {
                    game.look += Vec2::new(delta.0 as f32, delta.1 as f32);
                }
            }
            Event::AboutToWait => game.window.request_redraw(),
            _ => {}
        }
        if game.exit {
            game.settings.save();
            game.capture_mouse(false);
            target.exit();
        }
    })?;
    anyhow::ensure!(
        !failed.load(Ordering::Relaxed),
        "Runtime frame failed; see the error above"
    );
    Ok(())
}
fn pollster_main(window: Arc<Window>, settings: Settings, options: Options) -> Result<Game> {
    void_engine::block_on(Game::new(window, settings, options))
}
fn selftest() -> Result<()> {
    let mut p = physics::Physics::default();
    p.add(
        physics::Shape::Box(Vec3::new(10.0, 0.2, 10.0)),
        Vec3::new(0.0, -0.2, 0.0),
        Quat::IDENTITY,
        0.0,
        0.7,
        0,
    );
    let player = p.player(Vec3::new(4.0, 1.0, 4.0));
    let b = p.add(
        PropKind::Crate.shape(),
        Vec3::new(0.0, 2.0, 0.0),
        Quat::IDENTITY,
        13.0,
        0.6,
        1,
    );
    for _ in 0..180 {
        p.step(FIXED_DT);
    }
    anyhow::ensure!(
        (p.bodies[b].translation().y - 0.45).abs() < 0.06,
        "Gravity/settling failed"
    );
    let hit = p
        .raycast(
            Vec3::new(0.0, 1.5, 4.0),
            Vec3::new(0.0, -1.05, -4.0),
            8.0,
            player,
            false,
        )
        .context("Picking failed")?;
    anyhow::ensure!(hit.0 == b, "Wrong picked body");
    p.hold_filter(b, true);
    for _ in 0..120 {
        p.spring(b, Vec3::new(0.0, 2.0, 0.0));
        p.step(FIXED_DT);
    }
    anyhow::ensure!(
        (p.bodies[b].translation().y - 2.0).abs() < 0.1,
        "Grab spring failed"
    );
    p.release(b, Some(Vec3::new(0.0, 2.0, -17.0)));
    anyhow::ensure!(p.bodies[b].linvel().z < -16.0, "Throw failed");
    let model =
        void_engine::glb::load(&void_engine::asset_dir().join("models/void-manipulator.glb"))?;
    anyhow::ensure!(
        model.image.is_some() && !model.mesh.vertices.is_empty(),
        "GLB failed"
    );
    println!("SELFTEST PASS: gravity, settling, picking, grab spring, throw, GLB + texture ({} triangles)",model.mesh.indices.len()/3);
    Ok(())
}
