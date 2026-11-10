use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use void_engine::render::RenderSettings;

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
    pub fov: f32,
    pub eye_height: f32,
    pub camera_side: f32,
    pub sensitivity: f32,
    pub head_bob: bool,
    pub volume: f32,
    pub hdr_audio: f32,
    pub muted: bool,
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
            fov: 78.0,
            eye_height: 0.85,
            camera_side: 0.0,
            sensitivity: 1.0,
            head_bob: true,
            volume: 0.75,
            hdr_audio: 0.8,
            muted: false,
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
        s.fov = clamp(s.fov, 65.0, 110.0, 78.0);
        s.exposure = clamp(s.exposure, 0.5, 1.6, 1.05);
        s.fog = clamp(s.fog, 0.0, 2.0, 0.6);
        s.eye_height = clamp(s.eye_height, 0.70, 1.0, 0.85);
        s.camera_side = clamp(s.camera_side, -0.2, 0.2, 0.0);
        s.sensitivity = clamp(s.sensitivity, 0.2, 3.0, 1.0);
        s.volume = clamp(s.volume, 0.0, 1.0, 0.75);
        s.hdr_audio = clamp(s.hdr_audio, 0.0, 1.0, 0.8);
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
        }
    }
}
