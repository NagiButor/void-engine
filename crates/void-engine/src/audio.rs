//! Bounded spatial voice mixer with loudness-following HDR gain and soft limiting.
use glam::Vec3;
use rodio::{OutputStream, OutputStreamHandle, Source};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub const FOOTSTEP_VARIATIONS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum FootstepSurface {
    BootsLinoleum = 0,
    DirtRoad = 1,
    MetalGrate = 2,
    GlassGrit = 3,
    Ice = 4,
    Linoleum = 5,
    LowMetal = 6,
    MetalSteps = 7,
    Mud = 8,
    ParquetFloor = 9,
    PavementTiles = 10,
    Plastic = 11,
    Rubber = 12,
    SnowSteps = 13,
    TallGrass = 14,
    Tile = 15,
    Undergrowth = 16,
    WetSnow = 17,
}

impl FootstepSurface {
    pub const ALL: [Self; 18] = [
        Self::BootsLinoleum,
        Self::DirtRoad,
        Self::MetalGrate,
        Self::GlassGrit,
        Self::Ice,
        Self::Linoleum,
        Self::LowMetal,
        Self::MetalSteps,
        Self::Mud,
        Self::ParquetFloor,
        Self::PavementTiles,
        Self::Plastic,
        Self::Rubber,
        Self::SnowSteps,
        Self::TallGrass,
        Self::Tile,
        Self::Undergrowth,
        Self::WetSnow,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BootsLinoleum => "boots_linoleum",
            Self::DirtRoad => "dirt_road",
            Self::MetalGrate => "metal_grate",
            Self::GlassGrit => "glass_grit",
            Self::Ice => "ice",
            Self::Linoleum => "linoleum",
            Self::LowMetal => "low_metal",
            Self::MetalSteps => "metal_steps",
            Self::Mud => "mud",
            Self::ParquetFloor => "parquet_floor",
            Self::PavementTiles => "pavement_tiles",
            Self::Plastic => "plastic",
            Self::Rubber => "rubber",
            Self::SnowSteps => "snow_steps",
            Self::TallGrass => "tall_grass",
            Self::Tile => "tile",
            Self::Undergrowth => "undergrowth",
            Self::WetSnow => "wet_snow",
        }
    }

    /// Per-surface loudness trim balancing differing RMS energy across the pack.
    pub const fn gain_trim(self) -> f32 {
        match self {
            Self::BootsLinoleum => 0.50,
            Self::Rubber => 0.52,
            Self::MetalSteps => 0.55,
            Self::MetalGrate | Self::ParquetFloor | Self::SnowSteps => 0.78,
            Self::GlassGrit | Self::Ice | Self::WetSnow | Self::Undergrowth => 1.15,
            _ => 1.0,
        }
    }
}

struct Voice {
    samples: Arc<Vec<f32>>,
    cursor: f32,
    gain: f32,
    pan: f32,
    pitch: f32,
    filter: f32,
    delay_samples: usize,
    lowpass: f32,
}
struct Bus {
    voices: Vec<Voice>,
    volume: f32,
    hdr: f32,
    envelope: f32,
    slow: bool,
    right: f32,
    channel: bool,
}
struct Mixer(Arc<Mutex<Bus>>);

impl Iterator for Mixer {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let mut bus = self.0.lock().unwrap_or_else(|e| e.into_inner());
        bus.channel = !bus.channel;
        if !bus.channel {
            return Some(bus.right);
        }
        let slow = bus.slow;
        let mut left = 0.0;
        let mut right = 0.0;
        for v in &mut bus.voices {
            if v.delay_samples > 0 {
                v.delay_samples -= 1;
                continue;
            }
            let i = v.cursor as usize;
            if i + 1 >= v.samples.len() {
                continue;
            }
            let t = v.cursor.fract();
            let s = v.samples[i] * (1.0 - t) + v.samples[i + 1] * t;
            let lp = if slow { 0.12 } else { 0.85 } * v.lowpass.clamp(0.05, 1.0);
            v.filter += (s - v.filter) * lp;
            let fade = (v.cursor / 96.0).min(1.0);
            let s = v.filter * v.gain * fade;
            left += s * ((1.0 - v.pan) * 0.5).sqrt();
            right += s * ((1.0 + v.pan) * 0.5).sqrt();
            v.cursor += v.pitch * if slow { 0.72 } else { 1.0 };
        }
        bus.voices
            .retain(|v| (v.cursor as usize + 1) < v.samples.len());
        let peak = left.abs().max(right.abs());
        let coefficient = if peak > bus.envelope { 0.007 } else { 0.00003 };
        bus.envelope += (peak - bus.envelope) * coefficient;
        let gain = (0.24 / bus.envelope.max(0.055)).clamp(0.35, 3.0);
        let gain = (1.0 + (gain - 1.0) * bus.hdr) * bus.volume;
        bus.right = (right * gain).tanh() * 0.96;
        Some((left * gain).tanh() * 0.96)
    }
}
impl Source for Mixer {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        48000
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct Audio {
    _stream: Option<OutputStream>,
    _handle: Option<OutputStreamHandle>,
    bus: Arc<Mutex<Bus>>,
    clips: HashMap<String, Arc<Vec<f32>>>,
    footsteps: HashMap<FootstepSurface, [Arc<Vec<f32>>; FOOTSTEP_VARIATIONS]>,
    footstep_cursors: [AtomicUsize; FootstepSurface::ALL.len()],
    shot_cursor: AtomicUsize,
    pub listener: Vec3,
    pub right: Vec3,
}

fn load_wav_mono_48k(path: &Path) -> Arc<Vec<f32>> {
    let clip = std::fs::File::open(path)
        .ok()
        .and_then(|file| rodio::Decoder::new(std::io::BufReader::new(file)).ok())
        .map(|decoder| {
            rodio::source::UniformSourceIterator::new(decoder.convert_samples::<f32>(), 1, 48000)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            (0..4800)
                .map(|i| {
                    let t = i as f32 / 48000.0;
                    (t * 150.0 * std::f32::consts::TAU).sin() * (-t * 45.0).exp() * 0.2
                })
                .collect()
        });
    Arc::new(clip)
}

impl Audio {
    pub fn new(assets: &Path) -> Self {
        let bus = Arc::new(Mutex::new(Bus {
            voices: Vec::new(),
            volume: 0.75,
            hdr: 0.8,
            envelope: 0.1,
            slow: false,
            right: 0.0,
            channel: false,
        }));
        let (stream, handle) = match OutputStream::try_default() {
            Ok((stream, handle)) => {
                if let Err(e) = handle.play_raw(Mixer(bus.clone())) {
                    eprintln!("Audio: {e}");
                }
                (Some(stream), Some(handle))
            }
            Err(e) => {
                eprintln!("Audio device unavailable: {e}");
                (None, None)
            }
        };
        let sfx_dir = assets.join("audio/sfx");
        let mut clips = HashMap::new();
        for name in [
            "grab",
            "impact_concrete",
            "impact_metal",
            "impact_plastic",
            "impact_wood",
            "slowmo_off",
            "slowmo_on",
            "spawn",
            "throw",
            "ui_click",
            "pistol_shot_indoor_01",
            "pistol_shot_indoor_02",
            "pistol_shot_indoor_03",
            "pistol_shot_outdoor_01",
            "pistol_shot_outdoor_02",
            "pistol_shot_outdoor_03",
            "pistol_tail_outdoor",
            "pistol_dry_fire",
            "pistol_reload_easy",
            "pistol_reload_full",
            "shell_clink",
            "bullet_concrete_01",
            "bullet_concrete_02",
            "bullet_metal_01",
            "bullet_metal_02",
            "bullet_wood_01",
            "bullet_wood_02",
            "bullet_ricochet_01",
            "bullet_ricochet_02",
        ] {
            clips.insert(
                name.to_owned(),
                load_wav_mono_48k(&sfx_dir.join(format!("{name}.wav"))),
            );
        }
        let footsteps_dir = sfx_dir.join("footsteps");
        let mut footsteps = HashMap::with_capacity(FootstepSurface::ALL.len());
        for surface in FootstepSurface::ALL {
            let prefix = surface.as_str();
            let variations = std::array::from_fn(|i| {
                load_wav_mono_48k(&footsteps_dir.join(format!("{prefix}_{:02}.wav", i + 1)))
            });
            footsteps.insert(surface, variations);
        }
        Self {
            _stream: stream,
            _handle: handle,
            bus,
            clips,
            footsteps,
            footstep_cursors: std::array::from_fn(|_| AtomicUsize::new(0)),
            shot_cursor: AtomicUsize::new(0),
            listener: Vec3::ZERO,
            right: Vec3::X,
        }
    }
    pub fn settings(&self, volume: f32, hdr: f32, slow: bool) {
        let mut bus = self.bus.lock().unwrap_or_else(|e| e.into_inner());
        bus.volume = volume;
        bus.hdr = hdr;
        bus.slow = slow;
    }
    fn enqueue_clip(
        &self,
        clip: &Arc<Vec<f32>>,
        position: Option<Vec3>,
        gain: f32,
        pitch: f32,
        delay_samples: usize,
        lowpass: f32,
    ) {
        if gain <= 0.002 {
            return;
        }
        let (attenuation, pan) = position
            .map(|p| {
                let delta = p - self.listener;
                (
                    1.0 / (1.0 + delta.length_squared() * 0.045),
                    delta.normalize_or_zero().dot(self.right).clamp(-0.9, 0.9),
                )
            })
            .unwrap_or((1.0, 0.0));
        let mut bus = self.bus.lock().unwrap_or_else(|e| e.into_inner());
        if bus.voices.len() >= 32 {
            bus.voices.remove(0);
        }
        bus.voices.push(Voice {
            samples: clip.clone(),
            cursor: 0.0,
            gain: gain * attenuation,
            pan,
            pitch,
            filter: 0.0,
            delay_samples,
            lowpass,
        });
    }
    pub fn play(&self, name: &str, position: Option<Vec3>, gain: f32) {
        self.play_pitched(name, position, gain, 1.0);
    }
    pub fn play_pitched(&self, name: &str, position: Option<Vec3>, gain: f32, pitch: f32) {
        let Some(clip) = self.clips.get(name) else {
            return;
        };
        self.enqueue_clip(clip, position, gain, pitch, 0, 1.0);
    }
    pub fn play_gunshot(
        &self,
        muzzle_pos: Vec3,
        indoor_mix: f32,
        echo_taps: &[(Vec3, f32, f32)],
        pitch: f32,
    ) {
        let variant = (self.shot_cursor.fetch_add(1, Ordering::Relaxed) % 3) + 1;
        let indoor_mix = indoor_mix.clamp(0.0, 1.0);
        let indoor_name = format!("pistol_shot_indoor_{variant:02}");
        let outdoor_name = format!("pistol_shot_outdoor_{variant:02}");
        if let Some(clip) = self.clips.get(&indoor_name) {
            self.enqueue_clip(clip, Some(muzzle_pos), indoor_mix * 0.95, pitch, 0, 1.0);
        }
        if let Some(clip) = self.clips.get(&outdoor_name) {
            self.enqueue_clip(
                clip,
                Some(muzzle_pos),
                (1.0 - indoor_mix) * 0.92,
                pitch,
                0,
                1.0,
            );
            // Physical slapback echoes off surrounding arena walls when firing outdoors.
            for &(tap_pos, delay_sec, tap_gain) in echo_taps {
                let g = tap_gain * (1.0 - indoor_mix * 0.85);
                let delay = (delay_sec.max(0.02) * 48000.0) as usize;
                self.enqueue_clip(clip, Some(tap_pos), g, pitch * 0.98, delay, 0.16);
            }
        }
        if indoor_mix < 0.78 {
            if let Some(tail) = self.clips.get("pistol_tail_outdoor") {
                self.enqueue_clip(tail, None, (1.0 - indoor_mix) * 0.45, pitch, 1400, 0.75);
            }
        }
    }
    pub fn play_footstep(&self, surface: FootstepSurface, position: Option<Vec3>, gain: f32) {
        let variant = self.footstep_cursors[surface as usize].fetch_add(1, Ordering::Relaxed)
            % FOOTSTEP_VARIATIONS;
        self.play_footstep_variant(surface, variant, position, gain);
    }
    pub fn play_footstep_variant(
        &self,
        surface: FootstepSurface,
        variant: usize,
        position: Option<Vec3>,
        gain: f32,
    ) {
        let Some(clips) = self.footsteps.get(&surface) else {
            return;
        };
        self.enqueue_clip(
            &clips[variant % FOOTSTEP_VARIATIONS],
            position,
            gain * surface.gain_trim(),
            1.0,
            0,
            1.0,
        );
    }
    pub fn stats(&self) -> (usize, f32) {
        let bus = self.bus.lock().unwrap_or_else(|e| e.into_inner());
        (
            bus.voices.len(),
            20.0 * (0.24 / bus.envelope.max(0.055)).clamp(0.35, 3.0).log10(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdr_bus_is_bounded_and_spatial() {
        let bus = Arc::new(Mutex::new(Bus {
            voices: vec![Voice {
                samples: Arc::new(vec![0.5; 1000]),
                cursor: 100.0,
                gain: 8.0,
                pan: 0.9,
                pitch: 1.0,
                filter: 0.0,
                delay_samples: 0,
                lowpass: 1.0,
            }],
            volume: 1.0,
            hdr: 1.0,
            envelope: 0.1,
            slow: false,
            right: 0.0,
            channel: false,
        }));
        let mut mixer = Mixer(bus);
        for _ in 0..200 {
            let l = mixer.next().unwrap();
            let r = mixer.next().unwrap();
            assert!(l.is_finite() && r.is_finite() && l.abs() <= 0.96 && r.abs() <= 0.96);
            assert!(r > l);
        }
    }

    #[test]
    fn footstep_pack_loads_all_surfaces_and_variations() {
        let footsteps_dir = crate::asset_dir().join("audio/sfx/footsteps");
        let audio = Audio::new(&crate::asset_dir());
        assert_eq!(audio.footsteps.len(), FootstepSurface::ALL.len());
        for surface in FootstepSurface::ALL {
            let clips = audio
                .footsteps
                .get(&surface)
                .expect("surface clips should be loaded");
            for (i, clip) in clips.iter().enumerate() {
                let path = footsteps_dir.join(format!("{}_{:02}.wav", surface.as_str(), i + 1));
                assert!(path.is_file(), "missing footstep clip {}", path.display());
                assert!(
                    clip.len() > 10_000,
                    "clip {} fell back to synth ({} samples)",
                    path.display(),
                    clip.len()
                );
            }
            audio.play_footstep(surface, None, 0.25);
        }
        assert!(audio.stats().0 > 0);
    }
}
