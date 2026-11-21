mod settings;
mod ui;

use anyhow::{Context, Result};
use glam::{Quat, Vec2, Vec3};
use settings::Settings;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
use ui::{HudSnapshot, ScreenState, SettingsTab, SunState};
use void_engine::{
    audio::Audio,
    physics::{self, FIXED_DT},
    render::{Camera, Draw, Renderer},
    scene::{self, Scene},
};
use winit::{
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, Event, MouseScrollDelta, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowBuilder},
};

const SPARKLINE_SAMPLES: usize = 64;

#[derive(Default)]
struct Options {
    backend: String,
    adapter: Option<usize>,
    autostart: bool,
    showcase: bool,
    settings: bool,
    sun: bool,
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
            } else if let Some(v) = arg.strip_prefix("--time=") {
                s.time_of_day = v.parse::<f32>()?.rem_euclid(24.0);
            } else {
                match arg.as_str() {
                    "--autostart" => o.autostart = true,
                    "--showcase" => {
                        o.showcase = true;
                        o.autostart = true;
                    }
                    "--settings" => o.settings = true,
                    "--sun" => o.sun = true,
                    "--novsync" => s.vsync = false,
                    "--fullscreen" => o.fullscreen = true,
                    "--selftest" => o.selftest = true,
                    "--validation" => o.validation = true,
                    "--no-prompt" => {}
                    "--help" | "-h" => {
                        println!("V.O.I.D. Engine\n--rhi=dx12|vulkan|auto --adapter=N --autostart --showcase --settings\n--width=1600 --height=900 --novsync --fullscreen --validation\n--smoke=N --benchmark=N --screenshot=path.png --selftest");
                        std::process::exit(0);
                    }
                    _ => anyhow::bail!("Unknown option {arg}"),
                }
            }
        }
        Ok(o)
    }
}

struct Game {
    window: Arc<Window>,
    renderer: Renderer,
    scene: Scene,
    audio: Audio,
    camera: Camera,
    options: Options,
    settings: Settings,
    state: ScreenState,
    return_state: ScreenState,
    settings_tab: SettingsTab,
    sun: SunState,
    keys: HashSet<KeyCode>,
    look: Vec2,
    slow: bool,
    accumulator: f32,
    time: f32,
    last: Instant,
    frames: u32,
    frame_times: Vec<f32>,
    recent_ms: VecDeque<f32>,
    fps: f32,
    player_speed: f32,
    player_pos: Vec3,
    grounded: bool,
    hovered_light: Option<usize>,
    held_light: Option<usize>,
    hold_dist: f32,
    show_controls: bool,
    egui: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    footstep: f32,
    exit: bool,
    focused: bool,
}

impl Game {
    async fn new(window: Arc<Window>, settings: Settings, options: Options) -> Result<Self> {
        let assets = void_engine::asset_dir();
        let mut renderer = Renderer::new(
            window.clone(),
            &assets,
            &options.backend,
            options.adapter,
            settings.vsync,
            options.validation,
        )
        .await?;
        let scene = Scene::new(&mut renderer)?;
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
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .unwrap()
            .insert(0, "Roboto".into());
        ctx.set_fonts(fonts);
        ui::apply_theme(&ctx);
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
            scene,
            audio,
            camera: Camera::default(),
            options,
            settings,
            state: ScreenState::Menu,
            return_state: ScreenState::Menu,
            settings_tab: SettingsTab::Graphics,
            sun: SunState::default(),
            keys: HashSet::new(),
            look: Vec2::ZERO,
            slow: false,
            accumulator: 0.0,
            time: 0.0,
            last: Instant::now(),
            frames: 0,
            frame_times: Vec::new(),
            recent_ms: VecDeque::with_capacity(SPARKLINE_SAMPLES),
            fps: 60.0,
            player_speed: 0.0,
            player_pos: scene::SPAWN,
            grounded: true,
            hovered_light: None,
            held_light: None,
            hold_dist: 3.5,
            show_controls: true,
            egui: ctx,
            egui_state,
            egui_renderer,
            footstep: 0.0,
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
        if g.options.sun {
            g.open_sun();
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
        self.state = ScreenState::Playing;
        self.camera = Camera {
            fov: self.settings.fov,
            ..Default::default()
        };
        self.capture_mouse(true);
        self.accumulator = 0.0;
    }

    fn resume(&mut self) {
        self.state = ScreenState::Playing;
        self.capture_mouse(true);
        self.accumulator = 0.0;
    }

    fn pause(&mut self) {
        if self.state == ScreenState::Playing {
            self.state = ScreenState::Paused;
            self.capture_mouse(false);
            self.keys.clear();
            self.look = Vec2::ZERO;
        }
    }

    fn open_menu(&mut self) {
        self.state = ScreenState::Menu;
        self.capture_mouse(false);
        self.keys.clear();
        self.look = Vec2::ZERO;
    }

    fn open_settings(&mut self) {
        self.return_state = self.state;
        if self.state == ScreenState::Playing {
            self.pause();
        }
        self.state = ScreenState::Settings;
        self.capture_mouse(false);
    }

    fn close_settings(&mut self) {
        self.settings.save();
        self.state = self.return_state;
        self.capture_mouse(self.state == ScreenState::Playing);
    }

    fn open_sun(&mut self) {
        self.return_state = if self.state == ScreenState::Playing {
            self.pause();
            ScreenState::Paused
        } else {
            self.state
        };
        self.state = ScreenState::Sun;
        self.capture_mouse(false);
    }

    fn close_sun(&mut self) {
        self.settings.save();
        self.state = self.return_state;
        self.capture_mouse(self.state == ScreenState::Playing);
    }

    fn reset(&mut self) {
        self.scene.reset();
        self.held_light = None;
        self.hovered_light = None;
        self.camera = Camera {
            fov: self.settings.fov,
            ..Default::default()
        };
        self.player_pos = scene::SPAWN;
        self.player_speed = 0.0;
    }

    fn showcase(&mut self) {
        let p = Vec3::new(2.2, 0.81, 8.8);
        self.scene.physics.bodies[self.scene.player].set_translation(physics::vector(p), true);
        self.player_pos = p;
        self.camera.position = p + Vec3::Y * self.settings.eye_height;
        self.camera.yaw = 0.18;
        self.camera.pitch = -0.09;
        self.scene.physics.refresh_queries();
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
                ScreenState::Playing => self.pause(),
                ScreenState::Paused => self.resume(),
                ScreenState::Settings => self.close_settings(),
                ScreenState::Sun => self.close_sun(),
                ScreenState::Menu => self.exit = true,
            }
            return;
        }
        if key == KeyCode::F3 {
            if self.state == ScreenState::Settings {
                self.close_settings();
            } else {
                self.open_settings();
            }
            return;
        }
        if self.state == ScreenState::Menu && key == KeyCode::Enter {
            self.start();
            return;
        }
        if self.state != ScreenState::Playing {
            return;
        }
        match key {
            KeyCode::KeyE => {
                if self.held_light.take().is_some() {
                    self.audio.play("ui_click", None, 0.35);
                } else if let Some((idx, dist)) =
                    self.scene
                        .pick_light(self.camera.position, self.camera.forward(), 14.0)
                {
                    self.held_light = Some(idx);
                    self.hold_dist = dist.clamp(1.4, 10.0);
                    self.hovered_light = None;
                    self.audio.play("grab", None, 0.45);
                }
            }
            KeyCode::KeyR => self.reset(),
            KeyCode::KeyQ => {
                self.slow = !self.slow;
                self.audio.play(
                    if self.slow { "slowmo_on" } else { "slowmo_off" },
                    None,
                    0.5,
                );
            }
            KeyCode::KeyM => {
                self.settings.muted = !self.settings.muted;
            }
            KeyCode::KeyH => {
                self.show_controls = !self.show_controls;
            }
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
        if self.sun.auto_cycle {
            self.settings.time_of_day =
                (self.settings.time_of_day + dt * self.sun.cycle_speed).rem_euclid(24.0);
        }
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
        if self.state == ScreenState::Playing {
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
            let p = physics::vec3(self.scene.physics.bodies[self.scene.player].translation());
            let grounded = self
                .scene
                .physics
                .raycast(p, -Vec3::Y, 0.87, self.scene.player, false)
                .is_some();
            self.grounded = grounded;
            let b = &mut self.scene.physics.bodies[self.scene.player];
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
                self.scene.physics.step(FIXED_DT);
                self.accumulator -= FIXED_DT;
            }
            let p = physics::vec3(self.scene.physics.bodies[self.scene.player].translation());
            if p.y < -10.0 {
                self.reset();
            } else {
                self.player_pos = p;
            }
            self.player_speed = v.xz_length();
            self.camera.position = self.player_pos
                + Vec3::Y * self.settings.eye_height
                + right * self.settings.camera_side;
            if self.settings.head_bob && grounded && wish.length_squared() > 0.0 {
                self.camera.position.y += (self.time * speed * 2.6).sin() * 0.006;
            }
            self.footstep -= dt;
            if grounded && self.player_speed > 1.0 && self.footstep <= 0.0 {
                self.audio.play(
                    if self.frames.is_multiple_of(2) {
                        "footstep_1"
                    } else {
                        "footstep_2"
                    },
                    Some(self.player_pos - Vec3::Y * 0.8),
                    0.25,
                );
                self.footstep = 2.0 / self.player_speed;
            }
            if let Some(idx) = self.held_light {
                let target = self.scene.clamp_light_target(
                    self.camera.position,
                    self.camera.forward(),
                    self.hold_dist,
                );
                let alpha = 1.0 - (-18.0 * dt).exp();
                self.scene.lights[idx].pos = self.scene.lights[idx].pos.lerp(target, alpha);
                self.hovered_light = None;
            } else {
                self.hovered_light = self
                    .scene
                    .pick_light(self.camera.position, self.camera.forward(), 14.0)
                    .map(|(i, _)| i);
            }
        } else if self.state == ScreenState::Menu
            || (self.state == ScreenState::Sun && self.return_state == ScreenState::Menu)
        {
            self.hovered_light = None;
            self.camera.position = Vec3::new(9.5 + (self.time * 0.08).sin() * 0.35, 4.6, 12.8);
            let dir = (Vec3::new(-1.2, 1.3, -2.5) - self.camera.position).normalize();
            self.camera.yaw = (-dir.x).atan2(-dir.z);
            self.camera.pitch = dir.y.asin();
            self.camera.fov = 66.0;
        }
        self.audio.listener = self.camera.position;
        self.audio.right = self.camera.right();
        self.look = Vec2::ZERO;
    }

    fn scene_draws(&self) -> Vec<Draw> {
        self.scene.draws(self.hovered_light, self.held_light)
    }

    fn ui(&mut self, ctx: &egui::Context) {
        let old_vsync = self.settings.vsync;
        let old_size = (self.settings.width, self.settings.height);
        let (audio_voices, audio_gain) = self.audio.stats();
        let recent: Vec<f32> = self.recent_ms.iter().copied().collect();

        let actions = {
            let snap = HudSnapshot {
                state: self.state,
                fps: self.fps,
                recent_ms: &recent,
                adapter: &self.renderer.adapter_name,
                audio_voices,
                audio_gain,
                slow: self.slow,
                speed: self.player_speed,
                grounded: self.grounded,
                time: self.time,
                light_hovered: self.hovered_light.is_some(),
                light_held: self.held_light.is_some(),
                show_controls: self.show_controls,
            };
            ui::render(
                ctx,
                &snap,
                &mut self.settings,
                &mut self.settings_tab,
                &mut self.sun,
            )
        };

        if actions.click_sfx {
            self.audio.play("ui_click", None, 0.35);
        }
        if actions.reset_settings {
            self.settings = Settings::default();
        }
        if self.settings.vsync != old_vsync {
            self.renderer.vsync(self.settings.vsync);
        }
        if old_size != (self.settings.width, self.settings.height) {
            let _ = self
                .window
                .request_inner_size(PhysicalSize::new(self.settings.width, self.settings.height));
        }
        if actions.start {
            self.start();
        }
        if actions.resume {
            self.resume();
        }
        if actions.reset_scene {
            self.reset();
            self.resume();
        }
        if actions.open_settings {
            self.open_settings();
        }
        if actions.close_settings {
            self.close_settings();
        }
        if actions.open_sun {
            self.open_sun();
        }
        if actions.close_sun {
            self.close_sun();
        }
        if actions.reset_sun {
            let defaults = Settings::default();
            self.settings.time_of_day = defaults.time_of_day;
            self.settings.sun_azimuth = defaults.sun_azimuth;
            self.settings.sun_tilt = defaults.sun_tilt;
            self.sun = SunState::default();
        }
        if actions.to_menu {
            self.open_menu();
        }
        if actions.exit {
            self.exit = true;
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
        let ms = raw * 1000.0;
        self.fps += (1.0 / raw.max(0.0001) - self.fps) * 0.04;
        if self.recent_ms.len() >= SPARKLINE_SAMPLES {
            self.recent_ms.pop_front();
        }
        self.recent_ms.push_back(ms.clamp(0.5, 60.0));

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
        let lights = self.scene.render_lights();
        self.renderer.encode(
            &mut encoder,
            &view,
            &self.camera,
            &draws,
            &lights,
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
            self.frame_times.push(ms);
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
        let lights = self.scene.render_lights();
        let mut encoder = self
            .renderer
            .device
            .create_command_encoder(&Default::default());
        self.renderer.encode(
            &mut encoder,
            &view,
            &self.camera,
            draws,
            &lights,
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
            .with_title("Void engine")
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
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            if !consumed || code == KeyCode::Escape || code == KeyCode::F3 {
                                game.key(code, event.state == ElementState::Pressed, event.repeat);
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        if game.state == ScreenState::Playing && game.held_light.is_some() {
                            let y = match delta {
                                MouseScrollDelta::LineDelta(_, y) => y,
                                MouseScrollDelta::PixelDelta(p) => p.y as f32 / 30.0,
                            };
                            game.hold_dist = (game.hold_dist + y * 0.55).clamp(1.2, 12.0);
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
                if game.state == ScreenState::Playing && game.focused {
                    game.look += Vec2::new(delta.0 as f32, delta.1 as f32);
                }
            }
            Event::AboutToWait => game.window.request_redraw(),
            _ => {}
        }
        if game.exit {
            if game.options.smoke == 0 && game.options.benchmark == 0 {
                game.settings.save();
            }
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
        physics::Shape::Box(Vec3::splat(0.45)),
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
    println!("SELFTEST PASS: gravity, settling, picking, grab spring, throw");
    Ok(())
}
