//! Celestial mechanics (Sun & Moon trajectories) and procedural Sun/Moon textures.
use glam::{Quat, Vec3};
use std::f32::consts::{FRAC_PI_2, TAU};

pub const CELESTIAL_TEX_SIZE: u32 = 512;
pub const CLOUD_NOISE_SIZE: u32 = 64;
pub const WEATHER_TEX_SIZE: u32 = 256;

/// Computes the unit direction toward the Sun for the given `time_of_day` (0..24 h),
/// orbital `azimuth_deg`, and ecliptic `tilt_deg`.
pub fn sun_direction(time_of_day: f32, azimuth_deg: f32, tilt_deg: f32) -> Vec3 {
    let day_angle = (time_of_day.rem_euclid(24.0) / 24.0) * TAU - FRAC_PI_2;
    let tilt = tilt_deg.clamp(10.0, 89.0).to_radians();
    let local = Vec3::new(
        day_angle.cos(),
        day_angle.sin() * tilt.sin(),
        day_angle.sin() * tilt.cos(),
    );
    (Quat::from_rotation_y(azimuth_deg.to_radians()) * local).normalize()
}

/// Computes the unit direction toward the Moon along an inclined lunar orbital trajectory
/// determined by `time_of_day`, `azimuth_deg`, `tilt_deg`, and `moon_phase` (0=New .. 0.5=Full .. 1=New).
pub fn moon_direction(time_of_day: f32, azimuth_deg: f32, tilt_deg: f32, moon_phase: f32) -> Vec3 {
    // Synodic lag: a full moon (phase 0.5) lags ~12 hours behind the Sun;
    // waxing/waning phases shift rise/transit/set times realistically.
    let phase = moon_phase.rem_euclid(1.0);
    let moon_time = (time_of_day - phase * 24.0).rem_euclid(24.0);
    let orbit_angle = (moon_time / 24.0) * TAU - FRAC_PI_2;

    // Lunar orbit is inclined relative to the ecliptic plane (distinct arc & azimuth).
    let lunar_tilt = (tilt_deg + 8.5 + (phase * TAU).sin() * 5.0)
        .clamp(14.0, 88.0)
        .to_radians();
    let lunar_azimuth = (azimuth_deg - 14.0).to_radians();

    let local = Vec3::new(
        orbit_angle.cos(),
        orbit_angle.sin() * lunar_tilt.sin(),
        orbit_angle.sin() * lunar_tilt.cos(),
    );
    (Quat::from_rotation_y(lunar_azimuth) * local).normalize()
}

/// Returns the active primary directional shadow caster `(dir, is_sun)` and smooth
/// crossfade weight `sun_weight` in `[0.0, 1.0]`.
pub fn dominant_celestial_light(sun: Vec3, moon: Vec3) -> (Vec3, bool, f32) {
    let sun_weight = smoothstep(-0.06, 0.04, sun.y);
    if sun.y >= -0.02 {
        (
            Vec3::new(sun.x, sun.y.max(0.06), sun.z).normalize(),
            true,
            sun_weight,
        )
    } else if moon.y > -0.02 {
        (
            Vec3::new(moon.x, moon.y.max(0.08), moon.z).normalize(),
            false,
            sun_weight,
        )
    } else {
        (
            Vec3::new(moon.x, (-moon.y).max(0.18), moon.z).normalize(),
            false,
            sun_weight,
        )
    }
}

/// Samples `steps` points along the 24-hour Sun trajectory.
pub fn sun_trajectory(azimuth_deg: f32, tilt_deg: f32, steps: usize) -> Vec<Vec3> {
    (0..steps)
        .map(|i| {
            let h = (i as f32 / steps.max(1) as f32) * 24.0;
            sun_direction(h, azimuth_deg, tilt_deg)
        })
        .collect()
}

/// Samples `steps` points along the 24-hour Moon trajectory.
pub fn moon_trajectory(
    azimuth_deg: f32,
    tilt_deg: f32,
    moon_phase: f32,
    steps: usize,
) -> Vec<Vec3> {
    (0..steps)
        .map(|i| {
            let h = (i as f32 / steps.max(1) as f32) * 24.0;
            moon_direction(h, azimuth_deg, tilt_deg, moon_phase)
        })
        .collect()
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn hash3(p: Vec3) -> f32 {
    let d = p.dot(Vec3::new(127.1, 311.7, 74.7));
    let v = (d.sin()) * 43_758.547;
    v - v.floor()
}

fn noise3(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);

    let n000 = hash3(i + Vec3::new(0.0, 0.0, 0.0));
    let n100 = hash3(i + Vec3::new(1.0, 0.0, 0.0));
    let n010 = hash3(i + Vec3::new(0.0, 1.0, 0.0));
    let n110 = hash3(i + Vec3::new(1.0, 1.0, 0.0));
    let n001 = hash3(i + Vec3::new(0.0, 0.0, 1.0));
    let n101 = hash3(i + Vec3::new(1.0, 0.0, 1.0));
    let n011 = hash3(i + Vec3::new(0.0, 1.0, 1.0));
    let n111 = hash3(i + Vec3::new(1.0, 1.0, 1.0));

    let nx00 = n000 + (n100 - n000) * u.x;
    let nx10 = n010 + (n110 - n010) * u.x;
    let nx01 = n001 + (n101 - n001) * u.x;
    let nx11 = n011 + (n111 - n011) * u.x;

    let nxy0 = nx00 + (nx10 - nx00) * u.y;
    let nxy1 = nx01 + (nx11 - nx01) * u.y;
    nxy0 + (nxy1 - nxy0) * u.z
}

fn fbm3(mut p: Vec3, octaves: usize) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    for _ in 0..octaves {
        sum += amp * noise3(p);
        p = p * 2.07 + Vec3::new(13.7, 29.3, 7.1);
        amp *= 0.5;
    }
    sum
}

fn worley3(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let mut min_d = 4.0f32;
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                let g = Vec3::new(x as f32, y as f32, z as f32);
                let cell = i + g;
                let offset = Vec3::new(
                    hash3(cell + Vec3::new(1.3, 4.7, 2.1)),
                    hash3(cell + Vec3::new(5.1, 2.3, 7.9)),
                    hash3(cell + Vec3::new(8.7, 6.2, 3.4)),
                );
                let diff = g + offset - f;
                min_d = min_d.min(diff.length_squared());
            }
        }
    }
    min_d.sqrt().clamp(0.0, 1.0)
}

/// Generates a 512x512 procedural solar photosphere RGBA8 texture (convection granulation,
/// active sunspot groups with umbra/penumbra, bright faculae, and chromospheric turbulence).
pub fn generate_sun_texture(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let spots = [
        (Vec3::new(0.28, 0.16, 0.946).normalize(), 0.095f32),
        (Vec3::new(0.20, 0.21, 0.957).normalize(), 0.055),
        (Vec3::new(-0.36, -0.18, 0.915).normalize(), 0.082),
        (Vec3::new(-0.27, -0.12, 0.955).normalize(), 0.048),
        (Vec3::new(0.05, -0.34, 0.939).normalize(), 0.062),
    ];

    for py in 0..size {
        let v = (py as f32 + 0.5) / size as f32 * 2.0 - 1.0;
        for px in 0..size {
            let u = (px as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let r2 = u * u + v * v;
            // Clamp disc coordinates gently beyond r=1 so bilinear/mip filtering never bleeds black.
            let r = r2.sqrt();
            let (su, sv) = if r > 0.995 {
                let s = 0.995 / r.max(0.0001);
                (u * s, v * s)
            } else {
                (u, v)
            };
            let nz = (1.0 - (su * su + sv * sv)).max(0.0).sqrt();
            let sp = Vec3::new(su, sv, nz);

            // Multi-scale convection granules + turbulent plasma swirl
            let warp = Vec3::new(
                fbm3(sp * 5.2 + Vec3::new(1.2, 3.4, 0.8), 3),
                fbm3(sp * 5.2 + Vec3::new(7.1, 2.3, 4.9), 3),
                fbm3(sp * 5.2 + Vec3::new(4.5, 8.2, 6.1), 3),
            ) * 0.35;
            let gran_coarse = 1.0 - worley3(sp * 14.0 + warp * 2.5);
            let gran_fine = 1.0 - worley3(sp * 34.0 + warp * 4.0);
            let plasma_fbm = fbm3(sp * 7.5 + warp, 4);

            let mut temp = 0.76 + gran_coarse * 0.19 + gran_fine * 0.11 + (plasma_fbm - 0.5) * 0.18;

            // Sunspot umbra/penumbra + surrounding bright faculae
            for &(spot_dir, spot_r) in &spots {
                let ang_dist = sp.distance(spot_dir);
                let jitter = (fbm3(sp * 26.0, 2) - 0.5) * spot_r * 0.45;
                let d = ang_dist + jitter;
                let umbra = 1.0 - smoothstep(spot_r * 0.18, spot_r * 0.42, d);
                let penumbra = 1.0 - smoothstep(spot_r * 0.40, spot_r * 0.92, d);
                let facula = smoothstep(spot_r * 0.75, spot_r * 1.15, d)
                    * (1.0 - smoothstep(spot_r * 1.15, spot_r * 2.2, d));

                let striation =
                    0.85 + 0.15 * ((sp - spot_dir).x.atan2((sp - spot_dir).y) * 18.0).sin();
                temp *= 1.0 - penumbra * 0.42 * striation - umbra * 0.48;
                temp += facula * 0.16;
            }

            // Chromospheric edge brightening / solar limb color temperature shift
            let t = temp.clamp(0.15, 1.25);
            let r_col = (0.98 * t.powf(0.45)).clamp(0.0, 1.0);
            let g_col = (0.76 * t.powf(0.95)).clamp(0.0, 1.0);
            let b_col = (0.46 * t.powf(1.85)).clamp(0.0, 1.0);
            let disc_alpha = 1.0 - smoothstep(0.96, 1.0, r);

            let idx = ((py * size + px) * 4) as usize;
            data[idx] = (r_col * 255.0).round() as u8;
            data[idx + 1] = (g_col * 255.0).round() as u8;
            data[idx + 2] = (b_col * 255.0).round() as u8;
            data[idx + 3] = (disc_alpha * 255.0).round() as u8;
        }
    }
    data
}

struct Crater {
    dir: Vec3,
    radius: f32,
    depth: f32,
    has_rays: bool,
}

fn generate_craters() -> Vec<Crater> {
    let mut craters = Vec::with_capacity(145);

    // Hero Tycho- and Copernicus-like bright ray craters
    craters.push(Crater {
        dir: Vec3::new(-0.18, -0.52, 0.835).normalize(),
        radius: 0.125,
        depth: 0.95,
        has_rays: true,
    });
    craters.push(Crater {
        dir: Vec3::new(-0.34, 0.16, 0.926).normalize(),
        radius: 0.145,
        depth: 0.90,
        has_rays: true,
    });
    craters.push(Crater {
        dir: Vec3::new(0.42, -0.18, 0.889).normalize(),
        radius: 0.105,
        depth: 0.85,
        has_rays: true,
    });

    // Procedural multi-scale crater field across visible hemisphere
    for i in 0..140 {
        let fi = i as f32;
        let h1 = hash3(Vec3::new(fi * 1.17, 3.1, 7.3));
        let h2 = hash3(Vec3::new(fi * 2.41, 8.7, 1.9));
        let h3 = hash3(Vec3::new(fi * 3.73, 5.2, 9.4));
        let h4 = hash3(Vec3::new(fi * 5.09, 2.8, 4.6));

        let theta = h1 * TAU;
        let z = 0.08 + 0.92 * h2.powf(0.85);
        let xy = (1.0 - z * z).max(0.0).sqrt();
        let dir = Vec3::new(theta.cos() * xy, theta.sin() * xy, z).normalize();

        let (radius, depth) = if i < 14 {
            (0.085 + h3 * 0.10, 0.72 + h4 * 0.25)
        } else if i < 52 {
            (0.036 + h3 * 0.048, 0.55 + h4 * 0.28)
        } else {
            (0.013 + h3 * 0.023, 0.35 + h4 * 0.30)
        };
        craters.push(Crater {
            dir,
            radius,
            depth,
            has_rays: false,
        });
    }
    craters
}

/// Generates a 512x512 procedural lunar surface RGBA8 texture (linear RGB albedo + elevation
/// height in the Alpha channel for normal perturbation in the sky shader).
pub fn generate_moon_texture(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let craters = generate_craters();

    for py in 0..size {
        let v = (py as f32 + 0.5) / size as f32 * 2.0 - 1.0;
        for px in 0..size {
            let u = (px as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let r2 = u * u + v * v;
            let r = r2.sqrt();
            let (su, sv) = if r > 0.996 {
                let s = 0.996 / r.max(0.0001);
                (u * s, v * s)
            } else {
                (u, v)
            };
            let nz = (1.0 - (su * su + sv * sv)).max(0.0).sqrt();
            let sp = Vec3::new(su, sv, nz);

            // Basaltic lunar maria (dark lowlands) vs anorthositic highlands
            let warp = Vec3::new(
                fbm3(sp * 2.4 + Vec3::new(2.1, 5.7, 1.3), 3),
                fbm3(sp * 2.4 + Vec3::new(6.4, 1.8, 3.9), 3),
                fbm3(sp * 2.4 + Vec3::new(3.2, 7.1, 8.4), 3),
            ) * 0.45;
            let maria_noise = fbm3(sp * 2.1 + warp, 4);
            // Bias maria slightly toward upper-left & center-right like real Moon nearside
            let maria_bias = maria_noise + (-sp.x * 0.14 + sp.y * 0.10);
            let highland = smoothstep(0.46, 0.64, maria_bias);

            let micro_detail = fbm3(sp * 16.0, 4);
            let fine_regolith = fbm3(sp * 42.0, 3);

            let mut height =
                0.42 + highland * 0.30 + (micro_detail - 0.5) * 0.18 + (fine_regolith - 0.5) * 0.08;
            let mut albedo =
                0.22 + highland * 0.36 + (micro_detail - 0.5) * 0.12 + (fine_regolith - 0.5) * 0.06;

            for c in &craters {
                let dist = sp.distance(c.dir);
                let max_range = if c.has_rays {
                    c.radius * 5.5
                } else {
                    c.radius * 1.45
                };
                if dist > max_range {
                    continue;
                }
                let q = dist / c.radius;
                if q < 1.35 {
                    let bowl = if q < 0.86 {
                        let b = q / 0.86;
                        -(1.0 - b * b) * 0.24 * c.depth
                    } else {
                        0.0
                    };
                    let rim = (-((q - 0.94) / 0.16).powi(2)).exp() * 0.14 * c.depth;
                    let peak = if c.radius > 0.08 {
                        (-(q / 0.14).powi(2)).exp() * 0.08 * c.depth
                    } else {
                        0.0
                    };
                    height += bowl + rim + peak;
                    albedo += rim * 0.45 + bowl * 0.14;
                }
                if c.has_rays && q > 0.85 {
                    let tangent_u = sp - c.dir * sp.dot(c.dir);
                    let ref_axis = if c.dir.y.abs() < 0.9 {
                        Vec3::Y
                    } else {
                        Vec3::X
                    };
                    let b1 = c.dir.cross(ref_axis).normalize();
                    let b2 = c.dir.cross(b1).normalize();
                    let ang = tangent_u.dot(b2).atan2(tangent_u.dot(b1));
                    let streak = ((ang * 11.0 + noise3(sp * 9.0) * 1.8).sin() * 0.5 + 0.5)
                        .powf(3.0)
                        * ((ang * 23.0 + 1.4).cos() * 0.5 + 0.5);
                    let fade = (1.0 - smoothstep(0.85, 5.2, q)) * smoothstep(0.8, 1.3, q);
                    albedo += streak * fade * 0.22;
                }
            }

            // Subtle warm-gray vs cool-blue mineral tint between highlands and maria
            let lum = albedo.clamp(0.08, 0.88);
            let tint = Vec3::lerp(
                Vec3::new(0.89, 0.92, 0.98), // basalt maria (slightly cooler slate)
                Vec3::new(1.00, 0.98, 0.95), // anorthosite highlands (warm dusty gray)
                highland,
            );
            let rgb = (tint * lum).clamp(Vec3::ZERO, Vec3::ONE);
            let h_byte = (height.clamp(0.0, 1.0) * 255.0).round() as u8;

            let idx = ((py * size + px) * 4) as usize;
            data[idx] = (rgb.x * 255.0).round() as u8;
            data[idx + 1] = (rgb.y * 255.0).round() as u8;
            data[idx + 2] = (rgb.z * 255.0).round() as u8;
            data[idx + 3] = h_byte;
        }
    }
    data
}

#[inline]
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

#[inline]
fn hash3_periodic(x: i32, y: i32, z: i32, period: i32, seed: u32) -> f32 {
    let wx = x.rem_euclid(period) as u32;
    let wy = y.rem_euclid(period) as u32;
    let wz = z.rem_euclid(period) as u32;
    let h = hash_u32(
        wx.wrapping_mul(73856093)
            ^ wy.wrapping_mul(19349663)
            ^ wz.wrapping_mul(83492791)
            ^ seed.wrapping_mul(2654435761),
    );
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

fn value_noise3_periodic(p: Vec3, period: i32, seed: u32) -> f32 {
    let f_p = p * period as f32;
    let i = f_p.floor();
    let f = f_p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let ix = i.x as i32;
    let iy = i.y as i32;
    let iz = i.z as i32;

    let n000 = hash3_periodic(ix, iy, iz, period, seed);
    let n100 = hash3_periodic(ix + 1, iy, iz, period, seed);
    let n010 = hash3_periodic(ix, iy + 1, iz, period, seed);
    let n110 = hash3_periodic(ix + 1, iy + 1, iz, period, seed);
    let n001 = hash3_periodic(ix, iy, iz + 1, period, seed);
    let n101 = hash3_periodic(ix + 1, iy, iz + 1, period, seed);
    let n011 = hash3_periodic(ix, iy + 1, iz + 1, period, seed);
    let n111 = hash3_periodic(ix + 1, iy + 1, iz + 1, period, seed);

    let nx00 = n000 + (n100 - n000) * u.x;
    let nx10 = n010 + (n110 - n010) * u.x;
    let nx01 = n001 + (n101 - n001) * u.x;
    let nx11 = n011 + (n111 - n011) * u.x;
    let nxy0 = nx00 + (nx10 - nx00) * u.y;
    let nxy1 = nx01 + (nx11 - nx01) * u.y;
    nxy0 + (nxy1 - nxy0) * u.z
}

fn worley3_periodic(p: Vec3, period: i32, seed: u32) -> f32 {
    let f_p = p * period as f32;
    let i = f_p.floor();
    let f = f_p - i;
    let ix = i.x as i32;
    let iy = i.y as i32;
    let iz = i.z as i32;

    let mut min_d2 = 4.0f32;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = ix + dx;
                let cy = iy + dy;
                let cz = iz + dz;
                let ox = hash3_periodic(cx, cy, cz, period, seed);
                let oy = hash3_periodic(cx, cy, cz, period, seed + 17);
                let oz = hash3_periodic(cx, cy, cz, period, seed + 53);
                let d = Vec3::new(
                    dx as f32 + ox - f.x,
                    dy as f32 + oy - f.y,
                    dz as f32 + oz - f.z,
                );
                min_d2 = min_d2.min(d.length_squared());
            }
        }
    }
    (1.0 - min_d2.sqrt()).clamp(0.0, 1.0)
}

/// Generates a seamlessly tileable 3D `size³` RGBA8 Perlin-Worley cloud noise volume
/// `(R = Perlin-Worley base shape, G = medium Worley billow, B = fine Worley erosion, A = fine FBM)`.
pub fn generate_cloud_noise_3d(size: u32) -> Vec<u8> {
    let n = size as usize;
    let mut data = vec![0u8; n * n * n * 4];
    let inv = 1.0 / size as f32;

    for z in 0..n {
        let pz = (z as f32 + 0.5) * inv;
        for y in 0..n {
            let py = (y as f32 + 0.5) * inv;
            for x in 0..n {
                let px = (x as f32 + 0.5) * inv;
                let p = Vec3::new(px, py, pz);

                let fbm_base = value_noise3_periodic(p, 4, 1) * 0.55
                    + value_noise3_periodic(p, 8, 2) * 0.30
                    + value_noise3_periodic(p, 16, 3) * 0.15;
                let w_low = worley3_periodic(p, 4, 11);
                let perlin_worley = ((fbm_base - (1.0 - w_low) * 0.38) / 0.62).clamp(0.0, 1.0);

                let w_mid = worley3_periodic(p, 6, 23);
                let w_high = worley3_periodic(p, 12, 37);
                let fbm_fine = value_noise3_periodic(p, 12, 41) * 0.65
                    + value_noise3_periodic(p, 24, 43) * 0.35;

                let idx = ((z * n + y) * n + x) * 4;
                data[idx] = (perlin_worley * 255.0).round() as u8;
                data[idx + 1] = (w_mid * 255.0).round() as u8;
                data[idx + 2] = (w_high * 255.0).round() as u8;
                data[idx + 3] = (fbm_fine.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    data
}

fn value_noise2_periodic(px: f32, py: f32, period: i32, seed: u32) -> f32 {
    let fx_p = px * period as f32;
    let fy_p = py * period as f32;
    let ix = fx_p.floor() as i32;
    let iy = fy_p.floor() as i32;
    let fx = fx_p - ix as f32;
    let fy = fy_p - iy as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    let a = hash3_periodic(ix, iy, 0, period, seed);
    let b = hash3_periodic(ix + 1, iy, 0, period, seed);
    let c = hash3_periodic(ix, iy + 1, 0, period, seed);
    let d = hash3_periodic(ix + 1, iy + 1, 0, period, seed);
    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    ab + (cd - ab) * uy
}

/// Generates a seamlessly tileable 2D `size²` RGBA8 atmospheric weather map
/// `(R = macro cloud coverage, G = cloud vertical anvil scale, B = warped cirrus wisps)`.
pub fn generate_weather_texture(size: u32) -> Vec<u8> {
    let n = size as usize;
    let mut data = vec![0u8; n * n * 4];
    let inv = 1.0 / size as f32;

    for y in 0..n {
        let py = (y as f32 + 0.5) * inv;
        for x in 0..n {
            let px = (x as f32 + 0.5) * inv;
            let cov = value_noise2_periodic(px, py, 3, 101) * 0.50
                + value_noise2_periodic(px, py, 6, 103) * 0.28
                + value_noise2_periodic(px, py, 12, 107) * 0.14
                + value_noise2_periodic(px, py, 24, 109) * 0.08;
            let anvil = value_noise2_periodic(px, py, 4, 211) * 0.65
                + value_noise2_periodic(px, py, 9, 223) * 0.35;

            let wx = value_noise2_periodic(px, py, 4, 307) * 0.16;
            let wy = value_noise2_periodic(px + 0.37, py + 0.61, 4, 311) * 0.16;
            let cirrus = value_noise2_periodic(px + wx, py + wy, 6, 313) * 0.55
                + value_noise2_periodic(px + wx * 1.4, py + wy * 1.4, 14, 317) * 0.45;

            let idx = (y * n + x) * 4;
            data[idx] = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
            data[idx + 1] = (anvil.clamp(0.0, 1.0) * 255.0).round() as u8;
            data[idx + 2] = (cirrus.clamp(0.0, 1.0) * 255.0).round() as u8;
            data[idx + 3] = 255;
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_and_moon_follow_distinct_continuous_orbits() {
        let noon_sun = sun_direction(12.0, -28.0, 62.0);
        let midnight_sun = sun_direction(0.0, -28.0, 62.0);
        assert!(noon_sun.y > 0.7, "Sun should be high at noon");
        assert!(
            midnight_sun.y < -0.7,
            "Sun should be below horizon at midnight"
        );

        let midnight_moon = moon_direction(0.0, -28.0, 62.0, 0.5);
        let noon_moon = moon_direction(12.0, -28.0, 62.0, 0.5);
        assert!(
            midnight_moon.y > 0.6,
            "Full moon should be high at midnight"
        );
        assert!(
            noon_moon.y < -0.6,
            "Full moon should set below horizon by noon"
        );

        // Distinct inclined orbits: moon is not strictly -sun
        assert!((midnight_moon + midnight_sun).length() > 0.05);

        // Continuity across midnight wrap
        let m1 = moon_direction(23.99, -28.0, 62.0, 0.62);
        let m2 = moon_direction(0.01, -28.0, 62.0, 0.62);
        assert!((m1 - m2).length() < 0.02);
    }

    #[test]
    fn procedural_sun_and_moon_textures_have_rich_detail() {
        let sun = generate_sun_texture(64);
        let moon = generate_moon_texture(64);
        assert_eq!(sun.len(), 64 * 64 * 4);
        assert_eq!(moon.len(), 64 * 64 * 4);

        let sun_min = sun.iter().step_by(4).copied().min().unwrap();
        let sun_max = sun.iter().step_by(4).copied().max().unwrap();
        assert!(
            sun_max > sun_min + 20,
            "Sun texture should have granulation/sunspot contrast"
        );

        let moon_h_min = moon.iter().skip(3).step_by(4).copied().min().unwrap();
        let moon_h_max = moon.iter().skip(3).step_by(4).copied().max().unwrap();
        assert!(
            moon_h_max > moon_h_min + 30,
            "Moon heightfield should contain maria and craters"
        );

        let cloud_3d = generate_cloud_noise_3d(16);
        assert_eq!(cloud_3d.len(), 16 * 16 * 16 * 4);
        let weather = generate_weather_texture(32);
        assert_eq!(weather.len(), 32 * 32 * 4);
    }
}
