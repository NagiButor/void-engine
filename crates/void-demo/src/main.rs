mod settings;
mod ui;
mod weapon;

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
use ui::{HudSnapshot, MenuNav, ScreenState, SettingsTab, SunState};
use void_engine::{
    audio::Audio,
    input::{GamepadButton, GamepadInput, GamepadState},
    locomotion::FootstepGait,
    physics::{self, FIXED_DT},
    render::{Camera, Draw, PointLight, Renderer, MAX_POINT_LIGHTS},
    scene::{self, Scene, LIGHT_PRESETS},
    units::{camera, coord, human},
};
use weapon::WeaponSystem;
use winit::{
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
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
    ads: bool,
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
                    "--ads" => o.ads = true,
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
    hovered_prop: Option<usize>,
    held_prop: Option<usize>,
    light_preset: usize,
    hold_dist: f32,
    show_controls: bool,
    egui: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    gait: FootstepGait,
    resolved_vel: Vec3,
    weapon: WeaponSystem,
    gamepad: GamepadInput,
    gamepad_state: GamepadState,
    menu_nav: MenuNav,
    mouse_ads_held: bool,
    gamepad_sprint: bool,
    gamepad_active: bool,
    was_grounded: bool,
    prev_vy: f32,
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
        let scene = Scene::new(&mut renderer, &assets)?;
        let weapon = WeaponSystem::new(&mut renderer, &assets)?;
        let audio = Audio::new(&assets);
        let gamepad = GamepadInput::new();
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
            hovered_prop: None,
            held_prop: None,
            light_preset: 0,
            hold_dist: 2.5,
            show_controls: true,
            egui: ctx,
            egui_state,
            egui_renderer,
            gait: FootstepGait::default(),
            resolved_vel: Vec3::ZERO,
            weapon,
            gamepad,
            gamepad_state: GamepadState::default(),
            menu_nav: MenuNav::default(),
            mouse_ads_held: false,
            gamepad_sprint: false,
            gamepad_active: false,
            was_grounded: true,
            prev_vy: 0.0,
            exit: false,
            focused: true,
        };
        if g.options.autostart {
            g.start();
        }
        if g.options.showcase {
            g.showcase();
        }
        if g.options.ads {
            g.mouse_ads_held = true;
            g.weapon.ads_held = true;
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

    fn release_held(&mut self) {
        self.held_light = None;
        if let Some(idx) = self.held_prop.take() {
            if let Some(prop) = self.scene.props.get(idx) {
                self.scene.physics.release(prop.body, None);
            }
        }
    }

    fn pause(&mut self) {
        if self.state == ScreenState::Playing {
            self.release_held();
            self.gamepad.stop_rumble();
            self.mouse_ads_held = false;
            self.weapon.ads_held = false;
            self.gamepad_sprint = false;
            self.state = ScreenState::Paused;
            self.capture_mouse(false);
            self.keys.clear();
            self.look = Vec2::ZERO;
        }
    }

    fn open_menu(&mut self) {
        self.release_held();
        self.gamepad.stop_rumble();
        self.mouse_ads_held = false;
        self.weapon.ads_held = false;
        self.gamepad_sprint = false;
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
        self.held_prop = None;
        self.hovered_prop = None;
        self.camera = Camera {
            fov: self.settings.fov,
            ..Default::default()
        };
        self.player_pos = scene::SPAWN;
        self.player_speed = 0.0;
        self.resolved_vel = Vec3::ZERO;
        self.gait.reset();
        self.weapon.reset();
        self.gamepad_sprint = false;
    }

    fn showcase(&mut self) {
        let p = Vec3::new(1.8, human::SPAWN_Y, 6.8);
        self.scene.physics.bodies[self.scene.player].set_translation(physics::vector(p), true);
        self.player_pos = p;
        self.camera.position = p + Vec3::Y * (self.settings.eye_height - human::CENTER_HEIGHT);
        self.camera.yaw = 0.16;
        self.camera.pitch = -0.08;
        self.scene.physics.refresh_queries();
    }

    fn key(&mut self, key: KeyCode, down: bool, repeat: bool) {
        if down {
            self.keys.insert(key);
            self.gamepad_active = false;
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
            KeyCode::KeyE => self.toggle_grab(),
            KeyCode::KeyF => self.spawn_barrel(),
            KeyCode::KeyG | KeyCode::KeyL => self.place_light(),
            KeyCode::KeyX | KeyCode::Delete => self.delete_targeted(),
            KeyCode::KeyC => self.cycle_light_preset(1),
            KeyCode::Digit1 => self.select_light_preset(0),
            KeyCode::Digit2 => self.select_light_preset(1),
            KeyCode::Digit3 => self.select_light_preset(2),
            KeyCode::Digit4 => self.select_light_preset(3),
            KeyCode::Digit5 => self.select_light_preset(4),
            KeyCode::Digit6 => self.select_light_preset(5),
            KeyCode::KeyR => self.weapon.start_reload(&self.audio),
            KeyCode::KeyV => self.weapon.toggle_equipped(&self.audio),
            KeyCode::KeyT => self.weapon.trigger_inspect(),
            KeyCode::F5 | KeyCode::Backspace => self.reset(),
            KeyCode::KeyQ => self.toggle_slowmo(),
            KeyCode::KeyM => {
                self.settings.muted = !self.settings.muted;
            }
            KeyCode::KeyH => {
                self.show_controls = !self.show_controls;
            }
            KeyCode::BracketLeft => {
                self.settings.fov = (self.settings.fov - 1.0).max(camera::HFOV_MIN_DEG)
            }
            KeyCode::BracketRight => {
                self.settings.fov = (self.settings.fov + 1.0).min(camera::HFOV_MAX_DEG)
            }
            KeyCode::Comma => {
                self.settings.eye_height =
                    (self.settings.eye_height - 0.01).max(human::EYE_HEIGHT_MIN)
            }
            KeyCode::Period => {
                self.settings.eye_height =
                    (self.settings.eye_height + 0.01).min(human::EYE_HEIGHT_MAX)
            }
            _ => {}
        }
    }

    fn toggle_grab(&mut self) {
        if self.held_light.take().is_some() {
            self.audio.play("ui_click", None, 0.35);
        } else if let Some(idx) = self.held_prop.take() {
            let body = self.scene.props[idx].body;
            self.scene.physics.release(body, None);
            self.audio.play("ui_click", None, 0.35);
        } else {
            let light = self.scene.pick_light(
                self.camera.position,
                self.camera.forward(),
                human::LIGHT_PICK_RANGE,
            );
            let prop = self.scene.pick_prop(
                self.camera.position,
                self.camera.forward(),
                human::INTERACT_RANGE,
            );
            let pick_light_first = match (light, prop) {
                (Some((_, ld)), Some((_, pd))) => ld <= pd,
                (Some(_), None) => true,
                _ => false,
            };
            if pick_light_first {
                if let Some((idx, dist)) = light {
                    self.held_light = Some(idx);
                    self.hold_dist = dist.clamp(human::LIGHT_HOLD_MIN, human::LIGHT_HOLD_MAX);
                    self.hovered_light = None;
                    self.hovered_prop = None;
                    self.audio.play("grab", None, 0.45);
                    self.gamepad
                        .rumble(0.25, 0.45, 55, self.settings.gamepad_vibration);
                }
            } else if let Some((idx, dist)) = prop {
                let body = self.scene.props[idx].body;
                self.scene.physics.hold_filter(body, true);
                self.held_prop = Some(idx);
                self.hold_dist = dist.clamp(human::HOLD_DIST_MIN, human::HOLD_DIST_MAX);
                self.hovered_prop = None;
                self.hovered_light = None;
                self.audio.play("grab", None, 0.45);
                self.gamepad
                    .rumble(0.35, 0.40, 65, self.settings.gamepad_vibration);
            }
        }
    }

    fn spawn_barrel(&mut self) {
        let idx = self
            .scene
            .spawn_barrel_in_front(self.camera.position, self.camera.forward());
        let pos =
            physics::vec3(self.scene.physics.bodies[self.scene.props[idx].body].translation());
        self.audio.play("spawn", Some(pos), 0.45);
    }

    fn place_light(&mut self) {
        let pos = self
            .scene
            .place_light_pos(self.camera.position, self.camera.forward());
        if self.scene.add_light(pos, self.light_preset).is_some() {
            self.audio.play("spawn", Some(pos), 0.35);
        } else {
            self.audio.play("ui_click", None, 0.25);
        }
    }

    fn delete_targeted(&mut self) {
        if let Some(idx) = self.held_light.take().or(self.hovered_light.take()) {
            if self.scene.remove_light(idx) {
                self.audio.play("ui_click", None, 0.4);
            }
        } else if let Some(idx) = self.held_prop.take().or(self.hovered_prop.take()) {
            if self.scene.remove_prop(idx) {
                self.audio.play("ui_click", None, 0.4);
            }
        }
    }

    fn cycle_light_preset(&mut self, delta: isize) {
        let n = LIGHT_PRESETS.len() as isize;
        self.light_preset = ((self.light_preset as isize + delta).rem_euclid(n)) as usize;
        self.audio.play("ui_click", None, 0.25);
    }

    fn toggle_slowmo(&mut self) {
        self.slow = !self.slow;
        self.audio.play(
            if self.slow { "slowmo_on" } else { "slowmo_off" },
            None,
            0.5,
        );
    }

    fn fire_or_throw(&mut self) {
        if let Some(idx) = self.held_prop.take() {
            let body = self.scene.props[idx].body;
            let vel = self.camera.forward() * human::THROW_SPEED + Vec3::Y * human::THROW_LIFT;
            self.scene.physics.release(body, Some(vel));
            self.audio.play("throw", Some(self.camera.position), 0.5);
            self.gamepad
                .rumble(0.55, 0.35, 80, self.settings.gamepad_vibration);
        } else if self.held_light.take().is_some() {
            self.audio.play("ui_click", None, 0.35);
        } else if self.weapon.fire(
            &mut self.scene,
            &self.audio,
            &self.camera,
            self.player_speed,
            self.grounded,
        ) {
            self.gamepad
                .rumble(0.78, 0.55, 70, self.settings.gamepad_vibration);
        }
    }

    fn select_light_preset(&mut self, idx: usize) {
        self.light_preset = idx % LIGHT_PRESETS.len();
        self.audio.play("ui_click", None, 0.25);
    }

    fn mouse_button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.gamepad_active = false;
        }
        if self.state != ScreenState::Playing {
            if !down && button == MouseButton::Right {
                self.mouse_ads_held = false;
            }
            return;
        }
        if button == MouseButton::Right {
            self.mouse_ads_held = down;
            return;
        }
        if !down {
            return;
        }
        if button == MouseButton::Left {
            self.fire_or_throw();
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

        let gp = self.gamepad.poll(self.settings.gamepad_deadzone);
        if gp.connected {
            let any_axis = gp.move_axes.length_squared() > 0.01
                || gp.look_axes.length_squared() > 0.01
                || gp.left_trigger > 0.1
                || gp.right_trigger > 0.1;
            let any_btn = GamepadButton::ALL.iter().any(|&b| gp.pressed(b));
            if any_axis || any_btn {
                self.gamepad_active = true;
            }
        }

        if gp.pressed(GamepadButton::Start) {
            match self.state {
                ScreenState::Playing => self.pause(),
                ScreenState::Paused => self.resume(),
                ScreenState::Settings => self.close_settings(),
                ScreenState::Sun => self.close_sun(),
                ScreenState::Menu => self.start(),
            }
        } else if gp.pressed(GamepadButton::East) && self.state != ScreenState::Playing {
            match self.state {
                ScreenState::Paused => self.resume(),
                ScreenState::Settings => self.close_settings(),
                ScreenState::Sun => self.close_sun(),
                ScreenState::Menu | ScreenState::Playing => {}
            }
        }

        if self.state != ScreenState::Playing {
            let up_held = gp.down(GamepadButton::DPadUp) || gp.move_axes.y > 0.55;
            let down_held = gp.down(GamepadButton::DPadDown) || gp.move_axes.y < -0.55;
            let left_held = gp.down(GamepadButton::DPadLeft) || gp.move_axes.x < -0.55;
            let right_held = gp.down(GamepadButton::DPadRight) || gp.move_axes.x > 0.55;
            self.menu_nav.update(
                self.state,
                self.settings_tab,
                self.gamepad_active,
                up_held,
                down_held,
                left_held,
                right_held,
                gp.move_axes.x,
                gp.pressed(GamepadButton::South),
                gp.pressed(GamepadButton::LeftBumper),
                gp.pressed(GamepadButton::RightBumper),
                dt,
            );
        }

        self.gamepad_state = gp;
        self.weapon.ads_held = self.mouse_ads_held
            || (self.state == ScreenState::Playing && self.gamepad_state.left_trigger_down);

        if self.state == ScreenState::Playing {
            let gp = self.gamepad_state.clone();
            if gp.pressed(GamepadButton::North) {
                self.toggle_grab();
            }
            if gp.pressed(GamepadButton::West) {
                self.weapon.start_reload(&self.audio);
            }
            if gp.pressed(GamepadButton::East) {
                self.weapon.toggle_equipped(&self.audio);
            }
            if gp.pressed(GamepadButton::RightThumb) {
                self.weapon.trigger_inspect();
            }
            if gp.pressed(GamepadButton::LeftBumper) {
                self.toggle_slowmo();
            }
            if gp.pressed(GamepadButton::RightBumper) {
                self.spawn_barrel();
            }
            if gp.pressed(GamepadButton::Select) {
                self.show_controls = !self.show_controls;
            }
            if gp.pressed(GamepadButton::LeftThumb) {
                self.gamepad_sprint = !self.gamepad_sprint;
            }
            if self.held_light.is_some() || self.held_prop.is_some() {
                let min = if self.held_light.is_some() {
                    human::LIGHT_HOLD_MIN
                } else {
                    human::HOLD_DIST_MIN
                };
                let max = if self.held_light.is_some() {
                    human::LIGHT_HOLD_MAX
                } else {
                    human::HOLD_DIST_MAX
                };
                if gp.down(GamepadButton::DPadUp) {
                    self.hold_dist = (self.hold_dist + dt * 3.0).clamp(min, max);
                }
                if gp.down(GamepadButton::DPadDown) {
                    self.hold_dist = (self.hold_dist - dt * 3.0).clamp(min, max);
                }
            } else {
                if gp.pressed(GamepadButton::DPadUp) {
                    self.place_light();
                }
                if gp.pressed(GamepadButton::DPadDown) {
                    self.delete_targeted();
                }
            }
            if gp.pressed(GamepadButton::DPadRight) {
                self.cycle_light_preset(1);
            }
            if gp.pressed(GamepadButton::DPadLeft) {
                self.cycle_light_preset(-1);
            }
            if gp.right_trigger_pressed {
                self.fire_or_throw();
            }

            let eye_offset = self.settings.eye_height - human::CENTER_HEIGHT;
            let pitch_sign = if self.settings.invert_y { -1.0 } else { 1.0 };
            let ads_val = self.weapon.hud(self.player_speed, self.grounded).ads;
            let ads_sens = 1.0 - ads_val * 0.35;
            let gp_ads_sens = 1.0 - ads_val * 0.50;
            let gp_rate = 2.85 * self.settings.gamepad_sensitivity * gp_ads_sens;
            let gp_look_dx = gp.look_axes.x * gp_rate * dt;
            let gp_look_dy = gp.look_axes.y * gp_rate * dt;

            self.camera.yaw -=
                self.look.x * camera::MOUSE_RAD_PER_COUNT * self.settings.sensitivity * ads_sens
                    + gp_look_dx;
            self.camera.pitch = (self.camera.pitch
                - self.look.y
                    * camera::MOUSE_RAD_PER_COUNT
                    * self.settings.sensitivity
                    * ads_sens
                    * pitch_sign
                + gp_look_dy * pitch_sign)
                .clamp(-camera::MAX_PITCH_RAD, camera::MAX_PITCH_RAD);
            let combined_look = self.look + Vec2::new(gp_look_dx * 320.0, -gp_look_dy * 320.0);
            self.weapon.add_look_delta(combined_look);

            let mut wish = Vec3::ZERO;
            let forward = coord::horizontal_forward(self.camera.yaw);
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
            wish += right * gp.move_axes.x + forward * gp.move_axes.y;
            if gp.move_axes.length() < 0.25 {
                self.gamepad_sprint = false;
            }
            let sprinting = self.keys.contains(&KeyCode::ShiftLeft)
                || (self.gamepad_sprint && gp.move_axes.y > 0.35);
            let speed = if sprinting {
                human::SPRINT_SPEED
            } else {
                human::WALK_SPEED
            };
            let velocity = wish.clamp_length_max(1.0) * speed;
            let p = physics::vec3(self.scene.physics.bodies[self.scene.player].translation());
            let ground_hit = self.scene.physics.raycast(
                p,
                -Vec3::Y,
                human::GROUND_PROBE_DIST,
                self.scene.player,
                false,
            );
            let grounded = ground_hit.is_some();
            self.grounded = grounded;
            let b = &mut self.scene.physics.bodies[self.scene.player];
            let mut v = physics::vec3(b.linvel());
            let blend = 1.0 - (-if grounded { 22.0 } else { 6.0 } * dt).exp();
            v.x += (velocity.x - v.x) * blend;
            v.z += (velocity.z - v.z) * blend;
            if grounded && (self.keys.remove(&KeyCode::Space) || gp.pressed(GamepadButton::South)) {
                v.y = human::JUMP_VELOCITY;
            }
            b.set_linvel(physics::vector(v), true);
            let sim_dt = dt * if self.slow { 0.28 } else { 1.0 };
            self.accumulator += sim_dt.min(0.1);
            while self.accumulator >= FIXED_DT {
                if let Some(idx) = self.held_prop {
                    let p =
                        physics::vec3(self.scene.physics.bodies[self.scene.player].translation());
                    let eye = p + Vec3::Y * eye_offset + right * self.settings.camera_side;
                    let target = eye + self.camera.forward() * self.hold_dist;
                    self.scene
                        .physics
                        .spring(self.scene.props[idx].body, target);
                }
                self.scene.physics.step(FIXED_DT);
                self.resolved_vel =
                    physics::vec3(self.scene.physics.bodies[self.scene.player].linvel());
                for &(tag, pos, impulse) in &self.scene.physics.impacts {
                    if tag == scene::BARREL_TAG {
                        let gain = (impulse * 0.014).clamp(0.10, 0.72);
                        self.audio.play("impact_metal", Some(pos), gain);
                    }
                }
                self.accumulator -= FIXED_DT;
            }
            let p = physics::vec3(self.scene.physics.bodies[self.scene.player].translation());
            if p.y < -10.0 {
                self.reset();
            } else {
                self.player_pos = p;
            }
            self.player_speed = self.resolved_vel.xz_length();
            let gait = self
                .gait
                .update(sim_dt, self.player_speed, self.resolved_vel.y, grounded);
            self.camera.position =
                self.player_pos + Vec3::Y * eye_offset + right * self.settings.camera_side;
            if self.settings.head_bob {
                self.camera.position.y += gait.head_bob_y;
            }
            if let (Some((ground_body, _)), Some(step)) = (ground_hit, gait.footstep) {
                self.audio.play_footstep(
                    self.scene.surface_of(ground_body),
                    Some(self.player_pos - Vec3::Y * human::CENTER_HEIGHT),
                    step.gain,
                );
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
                self.hovered_prop = None;
            } else if self.held_prop.is_some() {
                self.hovered_light = None;
                self.hovered_prop = None;
            } else {
                let light = self.scene.pick_light(
                    self.camera.position,
                    self.camera.forward(),
                    human::LIGHT_PICK_RANGE,
                );
                let prop = self.scene.pick_prop(
                    self.camera.position,
                    self.camera.forward(),
                    human::INTERACT_RANGE,
                );
                match (light, prop) {
                    (Some((li, ld)), Some((pi, pd))) => {
                        if ld <= pd {
                            self.hovered_light = Some(li);
                            self.hovered_prop = None;
                        } else {
                            self.hovered_light = None;
                            self.hovered_prop = Some(pi);
                        }
                    }
                    (Some((li, _)), None) => {
                        self.hovered_light = Some(li);
                        self.hovered_prop = None;
                    }
                    (None, Some((pi, _))) => {
                        self.hovered_light = None;
                        self.hovered_prop = Some(pi);
                    }
                    (None, None) => {
                        self.hovered_light = None;
                        self.hovered_prop = None;
                    }
                }
            }
            let strafe_kb = if self.keys.contains(&KeyCode::KeyD) {
                1.0
            } else {
                0.0
            } - if self.keys.contains(&KeyCode::KeyA) {
                1.0
            } else {
                0.0
            };
            let strafe = (strafe_kb + gp.move_axes.x).clamp(-1.0, 1.0);
            if grounded && !self.was_grounded && self.prev_vy < -3.0 {
                let impact = ((-self.prev_vy - 3.0) / 5.5).clamp(0.15, 0.75);
                self.gamepad
                    .rumble(impact, impact * 0.4, 90, self.settings.gamepad_vibration);
            }
            self.was_grounded = grounded;
            self.prev_vy = self.resolved_vel.y;
            let holding_something = self.held_light.is_some() || self.held_prop.is_some();
            self.weapon.update(
                &self.renderer,
                &self.scene,
                &self.audio,
                &self.camera,
                dt,
                sim_dt,
                self.player_speed,
                grounded,
                self.resolved_vel.y,
                sprinting,
                strafe,
                holding_something,
            );
            self.camera.fov = self.settings.fov * self.weapon.fov_scale();
        } else if self.state == ScreenState::Menu
            || (self.state == ScreenState::Sun && self.return_state == ScreenState::Menu)
        {
            self.hovered_light = None;
            self.hovered_prop = None;
            self.camera.position = Vec3::new(7.8 + (self.time * 0.08).sin() * 0.30, 3.8, 10.2);
            let (yaw, pitch) =
                coord::yaw_pitch_to_target(self.camera.position, Vec3::new(-0.8, 1.1, -1.8));
            self.camera.yaw = yaw;
            self.camera.pitch = pitch;
            self.camera.fov = camera::MENU_HFOV_DEG;
        }
        self.audio.listener = self.camera.position;
        self.audio.right = self.camera.right();
        self.look = Vec2::ZERO;
    }

    fn render_camera(&self) -> Camera {
        let (rp, ry, roll) = if self.state == ScreenState::Playing {
            self.weapon.camera_offsets(self.time)
        } else {
            (0.0, 0.0, 0.0)
        };
        Camera {
            position: self.camera.position,
            yaw: self.camera.yaw + ry,
            pitch: (self.camera.pitch + rp).clamp(-camera::MAX_PITCH_RAD, camera::MAX_PITCH_RAD),
            roll,
            fov: self.camera.fov,
        }
    }

    fn scene_draws(&self, render_cam: &Camera) -> Vec<Draw> {
        let mut draws = self.scene.draws(self.hovered_light, self.held_light);
        if self.state == ScreenState::Playing {
            self.weapon
                .append_draws(&self.scene, render_cam, &mut draws);
        }
        draws
    }

    fn scene_lights(&self) -> Vec<PointLight> {
        let mut lights = self.scene.render_lights();
        if self.state == ScreenState::Playing {
            self.weapon.append_lights(&mut lights);
        }
        lights
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
                time: self.time,
                light_hovered: self.hovered_light.is_some(),
                light_held: self.held_light.is_some(),
                prop_hovered: self.hovered_prop.is_some(),
                prop_held: self.held_prop.is_some(),
                light_count: self.scene.lights.len(),
                light_max: MAX_POINT_LIGHTS,
                light_preset: self.light_preset,
                prop_count: self.scene.props.len(),
                show_controls: self.show_controls,
                gamepad_name: self.gamepad_state.name.as_deref(),
                gamepad_active: self.gamepad_active,
                weapon: self.weapon.hud(self.player_speed, self.grounded),
            };
            ui::render(
                ctx,
                &snap,
                &mut self.settings,
                &mut self.settings_tab,
                &mut self.sun,
                &mut self.menu_nav,
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
            self.settings.moon_phase = defaults.moon_phase;
            self.settings.cloud_coverage = defaults.cloud_coverage;
            self.settings.show_trajectory = defaults.show_trajectory;
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
        let render_cam = self.render_camera();
        let draws = self.scene_draws(&render_cam);
        let lights = self.scene_lights();
        self.renderer.encode(
            &mut encoder,
            &view,
            &render_cam,
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
        let lights = self.scene_lights();
        let render_cam = self.render_camera();
        let mut encoder = self
            .renderer
            .device
            .create_command_encoder(&Default::default());
        self.renderer.encode(
            &mut encoder,
            &view,
            &render_cam,
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
                    WindowEvent::MouseInput { state, button, .. } => {
                        if state == ElementState::Pressed {
                            game.gamepad_active = false;
                        }
                        if !consumed {
                            game.mouse_button(button, state == ElementState::Pressed);
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        if game.state == ScreenState::Playing
                            && (game.held_light.is_some() || game.held_prop.is_some())
                        {
                            let y = match delta {
                                MouseScrollDelta::LineDelta(_, y) => y,
                                MouseScrollDelta::PixelDelta(p) => p.y as f32 / 30.0,
                            };
                            let max = if game.held_light.is_some() {
                                human::LIGHT_HOLD_MAX
                            } else {
                                human::HOLD_DIST_MAX
                            };
                            let min = if game.held_light.is_some() {
                                human::LIGHT_HOLD_MIN
                            } else {
                                human::HOLD_DIST_MIN
                            };
                            game.hold_dist = (game.hold_dist + y * 0.45).clamp(min, max);
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
                    if delta.0.abs() > 0.5 || delta.1.abs() > 0.5 {
                        game.gamepad_active = false;
                    }
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
