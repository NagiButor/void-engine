//! Deterministic, seamless material atlases: colour/roughness and tangent normals.
pub const SIZE: u32 = 512;
pub const LAYERS: u32 = 8;
pub const WHITE: u32 = 0;
pub const CONCRETE: u32 = 1;
pub const WOOD: u32 = 2;
pub const METAL: u32 = 3;
pub const PAINT: u32 = 4;
pub const HAZARD: u32 = 5;
pub const RUBBER: u32 = 6;
pub const BRICK: u32 = 7;
pub const VIEWMODEL: u32 = 8;

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ seed;
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = x.fract();
    let fy = y.fract();
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let h = |a: i32, b: i32| hash(a.rem_euclid(period), b.rem_euclid(period), seed);
    let a = h(xi, yi) * (1.0 - u) + h(xi + 1, yi) * u;
    let b = h(xi, yi + 1) * (1.0 - u) + h(xi + 1, yi + 1) * u;
    a * (1.0 - v) + b * v
}

pub struct Atlas {
    pub albedo: Vec<u8>,
    pub normal: Vec<u8>,
}

pub fn generate() -> Atlas {
    let count = (SIZE * SIZE * LAYERS * 4) as usize;
    let mut atlas = Atlas {
        albedo: vec![255; count],
        normal: vec![128; count],
    };
    for layer in 0..LAYERS {
        let mut heights = vec![0.5f32; (SIZE * SIZE) as usize];
        for y in 0..SIZE {
            for x in 0..SIZE {
                let u = x as f32 / SIZE as f32;
                let v = y as f32 / SIZE as f32;
                let fine = hash(x as i32, y as i32, 17 + layer) - 0.5;
                let broad = noise(u * 8.0, v * 8.0, 8, 21 + layer);
                let medium = noise(u * 64.0, v * 64.0, 64, 74);
                let mut roughness: f32 = 0.8;
                let mut height = medium * 0.12 + fine * 0.035;
                let rgb: [f32; 3] = match layer {
                    WHITE => {
                        height = 0.5;
                        roughness = 0.65;
                        [1.0; 3]
                    }
                    CONCRETE => {
                        let pore = if fine > 0.47 { -0.13 } else { 0.0 };
                        let c = 0.49 + (broad - 0.5) * 0.10 + fine * 0.065 + pore;
                        height = medium * 0.018 + fine * 0.003 + pore * 0.08;
                        [c * 1.02, c * 1.01, c * 0.96]
                    }
                    WOOD => {
                        let warp = noise(u * 8.0, v * 8.0, 8, 4) * 3.0;
                        let grain =
                            (u * std::f32::consts::TAU * 65.0 + warp + (v * 12.56).sin()).sin();
                        let seam = (u * 5.0).fract().min(1.0 - (u * 5.0).fract()) < 0.009;
                        let c = 0.59 + grain * 0.065 + (broad - 0.5) * 0.18 + fine * 0.035;
                        height = grain * 0.08 + if seam { -0.35 } else { 0.0 };
                        let s = if seam { 0.32 } else { 1.0 };
                        [c * s, c * 0.72 * s, c * 0.43 * s]
                    }
                    METAL => {
                        roughness = 0.38 + broad * 0.18;
                        let scratch = if hash(x as i32, 0, 8) > 0.985 {
                            0.14
                        } else {
                            0.0
                        };
                        let c = 0.66 + fine * 0.06 + scratch;
                        height = fine * 0.002 + scratch * 0.004;
                        [c * 0.96, c, c * 1.03]
                    }
                    PAINT => {
                        roughness = 0.4 + medium * 0.22;
                        let chip = broad > 0.83 && medium > 0.84;
                        let c = if chip {
                            0.32 + fine * 0.09
                        } else {
                            0.84 + fine * 0.035
                        };
                        height = if chip { -0.018 } else { medium * 0.006 };
                        if chip {
                            [c, c * 0.62, c * 0.36]
                        } else {
                            [c; 3]
                        }
                    }
                    HAZARD => {
                        roughness = 0.65;
                        let stripe = ((u + v) * 8.0).fract() > 0.5;
                        let wear = if medium > 0.8 { 0.55 } else { 1.0 };
                        if stripe {
                            [0.065 * wear; 3]
                        } else {
                            [0.90 * wear, 0.61 * wear, 0.09 * wear]
                        }
                    }
                    RUBBER => {
                        roughness = 0.92;
                        let weave = ((x + y) % 4) as f32 * 0.008;
                        height = weave + fine * 0.03;
                        [0.20 + weave + fine * 0.025; 3]
                    }
                    _ => {
                        roughness = 0.94;
                        [0.65 + fine * 0.13, 0.37 + fine * 0.07, 0.25 + fine * 0.04]
                    }
                };
                let index = ((layer * SIZE * SIZE + y * SIZE + x) * 4) as usize;
                for (c, value) in rgb.iter().enumerate() {
                    atlas.albedo[index + c] = (value.clamp(0.0, 1.0) * 255.0) as u8;
                }
                atlas.albedo[index + 3] = (roughness.clamp(0.0, 1.0) * 255.0) as u8;
                heights[(y * SIZE + x) as usize] = height;
            }
        }
        for y in 0..SIZE {
            for x in 0..SIZE {
                let h = |dx: u32, dy: u32| {
                    heights[(((y + dy) % SIZE) * SIZE + (x + dx) % SIZE) as usize]
                };
                let dx = h(1, 0) - h(SIZE - 1, 0);
                let dy = h(0, 1) - h(0, SIZE - 1);
                let n = glam::Vec3::new(-dx * 2.5, -dy * 2.5, 1.0).normalize();
                let index = ((layer * SIZE * SIZE + y * SIZE + x) * 4) as usize;
                for c in 0..3 {
                    atlas.normal[index + c] = ((n[c] * 0.5 + 0.5) * 255.0) as u8;
                }
                atlas.normal[index + 3] = 255;
            }
        }
    }
    atlas
}
