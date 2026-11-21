use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use void_engine::{
    render::RenderSettings,
    units::{camera, human},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub shadows: u32,
    pub exposure: f32,
    pub fog: f32,
    pub ao: bool,
    pub bloom: bool,
    pub gi: bool,
    pub gi_intensity: f32,
    pub clouds: bool,
    pub cloud_coverage: f32,
    pub volumetric: bool,
    pub volumetric_intensity: f32,
    pub fov: f32,
    pub eye_height: f32,
    pub camera_side: f32,
    pub sensitivity: f32,
    pub invert_y: bool,
    pub head_bob: bool,
    pub gamepad_sensitivity: f32,
    pub gamepad_deadzone: f32,
    pub gamepad_vibration: bool,
    pub volume: f32,
    pub hdr_audio: f32,
    pub muted: bool,
    pub time_of_day: f32,
    pub sun_azimuth: f32,
    pub sun_tilt: f32,
    pub moon_phase: f32,
    pub show_trajectory: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 900,
            vsync: true,
            shadows: 2,
            exposure: 1.05,
            fog: 0.6,
            ao: true,
            bloom: true,
            gi: true,
            gi_intensity: 1.0,
            clouds: true,
            cloud_coverage: 0.52,
            volumetric: true,
            volumetric_intensity: 1.0,
            fov: camera::DEFAULT_HFOV_DEG,
            eye_height: human::EYE_HEIGHT,
            camera_side: 0.0,
            sensitivity: camera::SENSITIVITY_DEFAULT,
            invert_y: false,
            head_bob: true,
            gamepad_sensitivity: 1.0,
            gamepad_deadzone: 0.12,
            gamepad_vibration: true,
            volume: 0.75,
            hdr_audio: 0.8,
            muted: false,
            time_of_day: 13.5,
            sun_azimuth: -28.0,
            sun_tilt: 62.0,
            moon_phase: 0.42,
            show_trajectory: true,
        }
    }
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("settings.toml")))
            .unwrap_or_else(|| PathBuf::from("settings.toml"))
    }
    pub fn load() -> Self {
        let mut s: Self = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        let clamp = |v: f32, a: f32, b: f32, default: f32| {
            if v.is_finite() {
                v.clamp(a, b)
            } else {
                default
            }
        };
        s.width = s.width.clamp(640, 3840);
        s.height = s.height.clamp(360, 2160);
        s.shadows = s.shadows.min(2);
        s.fov = clamp(
            s.fov,
            camera::HFOV_MIN_DEG,
            camera::HFOV_MAX_DEG,
            camera::DEFAULT_HFOV_DEG,
        );
        s.exposure = clamp(s.exposure, 0.5, 1.6, 1.05);
        s.fog = clamp(s.fog, 0.0, 2.0, 0.6);
        s.gi_intensity = clamp(s.gi_intensity, 0.0, 2.0, 1.0);
        s.cloud_coverage = clamp(s.cloud_coverage, 0.0, 1.0, 0.52);
        s.volumetric_intensity = clamp(s.volumetric_intensity, 0.0, 2.0, 1.0);
        // Migrate legacy center-relative eye offsets (< 1.2 m) to floor-relative POV height.
        let raw_eye = if s.eye_height.is_finite() && s.eye_height < 1.2 {
            s.eye_height + human::CENTER_HEIGHT
        } else {
            s.eye_height
        };
        s.eye_height = clamp(
            raw_eye,
            human::EYE_HEIGHT_MIN,
            human::EYE_HEIGHT_MAX,
            human::EYE_HEIGHT,
        );
        s.camera_side = clamp(
            s.camera_side,
            -camera::SIDE_OFFSET_MAX_M,
            camera::SIDE_OFFSET_MAX_M,
            0.0,
        );
        s.sensitivity = clamp(
            s.sensitivity,
            camera::SENSITIVITY_MIN,
            camera::SENSITIVITY_MAX,
            camera::SENSITIVITY_DEFAULT,
        );
        s.gamepad_sensitivity = clamp(s.gamepad_sensitivity, 0.25, 3.0, 1.0);
        s.gamepad_deadzone = clamp(s.gamepad_deadzone, 0.04, 0.30, 0.12);
        s.volume = clamp(s.volume, 0.0, 1.0, 0.75);
        s.hdr_audio = clamp(s.hdr_audio, 0.0, 1.0, 0.8);
        s.time_of_day = clamp(s.time_of_day, 0.0, 24.0, 13.5);
        s.sun_azimuth = clamp(s.sun_azimuth, -180.0, 180.0, -28.0);
        s.sun_tilt = clamp(s.sun_tilt, 15.0, 85.0, 62.0);
        s.moon_phase = clamp(s.moon_phase, 0.0, 1.0, 0.42);
        s
    }
    pub fn save(&self) {
        if let Ok(text) = toml::to_string_pretty(self) {
            if let Err(e) = std::fs::write(Self::path(), text) {
                eprintln!("Saving settings: {e}");
            }
        }
    }
    pub fn rendering(&self) -> RenderSettings {
        RenderSettings {
            exposure: self.exposure,
            fog: self.fog,
            shadows: self.shadows,
            ao: self.ao,
            bloom: self.bloom,
            gi: self.gi,
            gi_intensity: self.gi_intensity,
            clouds: self.clouds,
            cloud_coverage: self.cloud_coverage,
            volumetric: self.volumetric,
            volumetric_intensity: self.volumetric_intensity,
            time_of_day: self.time_of_day,
            sun_azimuth: self.sun_azimuth,
            sun_tilt: self.sun_tilt,
            moon_phase: self.moon_phase,
            show_trajectory: self.show_trajectory,
        }
    }
}
