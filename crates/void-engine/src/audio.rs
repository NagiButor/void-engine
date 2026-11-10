//! Bounded spatial voice mixer with loudness-following HDR gain and soft limiting.
use glam::Vec3;
use rodio::{OutputStream, OutputStreamHandle, Source};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

struct Voice {
    samples: Arc<Vec<f32>>,
    cursor: f32,
    gain: f32,
    pan: f32,
    pitch: f32,
    filter: f32,
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
            let i = v.cursor as usize;
            if i + 1 >= v.samples.len() {
                continue;
            }
            let t = v.cursor.fract();
            let s = v.samples[i] * (1.0 - t) + v.samples[i + 1] * t;
            v.filter += (s - v.filter) * if slow { 0.12 } else { 0.85 };
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
    pub listener: Vec3,
    pub right: Vec3,
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
        let mut clips = HashMap::new();
        for name in [
            "footstep_1",
            "footstep_2",
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
        ] {
            let clip = std::fs::File::open(assets.join("audio/sfx").join(format!("{name}.wav")))
                .ok()
                .and_then(|file| rodio::Decoder::new(std::io::BufReader::new(file)).ok())
                .map(|decoder| {
                    rodio::source::UniformSourceIterator::new(
                        decoder.convert_samples::<f32>(),
                        1,
                        48000,
                    )
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
            clips.insert(name.to_owned(), Arc::new(clip));
        }
        Self {
            _stream: stream,
            _handle: handle,
            bus,
            clips,
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
    pub fn play(&self, name: &str, position: Option<Vec3>, gain: f32) {
        let Some(clip) = self.clips.get(name) else {
            return;
        };
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
            pitch: 1.0,
            filter: 0.0,
        });
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
}
