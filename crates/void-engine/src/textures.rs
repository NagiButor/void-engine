//! DevTextures & weapon/decal material atlas: albedo/roughness (or cutout alpha for decals) and tangent-space normals.
use anyhow::{Context, Result};
use image::{imageops::FilterType, DynamicImage, RgbImage, RgbaImage};
use std::path::Path;

pub const SIZE: u32 = 1024;
pub const LAYERS: u32 = 11;
pub const DEV_GRAY: u32 = 0;
pub const DEV_GRAY_INV: u32 = 1;
pub const DEV_ORANGE: u32 = 2;
pub const DEV_ORANGE_INV: u32 = 3;
pub const WHITE: u32 = 4;
pub const BARREL: u32 = 5;
pub const ARMS: u32 = 6;
pub const PISTOL: u32 = 7;
pub const DECAL_CONCRETE: u32 = 8;
pub const DECAL_METAL: u32 = 9;
pub const DECAL_WOOD: u32 = 10;

const DEV_SETS: [&str; 4] = [
    "Grid_4x4-Gray",
    "Grid_4x4-GrayInverted",
    "Grid_4x4-Orange",
    "Grid_4x4-OrangeInverted",
];

pub struct Atlas {
    pub albedo: Vec<u8>,
    pub normal: Vec<u8>,
}

fn load_rgb_resized(path: &Path) -> Result<RgbImage> {
    let img: DynamicImage =
        image::open(path).with_context(|| format!("Loading {}", path.display()))?;
    let resized = if img.width() == SIZE && img.height() == SIZE {
        img
    } else {
        img.resize_exact(SIZE, SIZE, FilterType::Triangle)
    };
    Ok(resized.to_rgb8())
}

fn write_pbr_layer(
    atlas: &mut Atlas,
    layer: u32,
    albedo_path: &Path,
    rough_path: &Path,
    normal_path: &Path,
) -> Result<()> {
    let albedo_img = load_rgb_resized(albedo_path)?;
    let rough_img = load_rgb_resized(rough_path)?;
    let normal_img = load_rgb_resized(normal_path)?;

    let layer_bytes = (SIZE * SIZE * 4) as usize;
    let base = layer as usize * layer_bytes;
    let albedo_raw = albedo_img.as_raw();
    let rough_raw = rough_img.as_raw();
    let normal_raw = normal_img.as_raw();

    for i in 0..(SIZE * SIZE) as usize {
        let src = i * 3;
        let dst = base + i * 4;
        atlas.albedo[dst] = albedo_raw[src];
        atlas.albedo[dst + 1] = albedo_raw[src + 1];
        atlas.albedo[dst + 2] = albedo_raw[src + 2];
        atlas.albedo[dst + 3] = rough_raw[src];

        atlas.normal[dst] = normal_raw[src];
        atlas.normal[dst + 1] = normal_raw[src + 1];
        atlas.normal[dst + 2] = normal_raw[src + 2];
        atlas.normal[dst + 3] = 255;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum DecalKind {
    Concrete,
    Metal,
    Wood,
}

fn hash2(x: f32, y: f32, seed: f32) -> f32 {
    let v = (x * 127.1 + y * 311.7 + seed * 74.7).sin() * 43758.545;
    v - v.floor()
}

fn value_noise(x: f32, y: f32, seed: f32) -> f32 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2(ix, iy, seed);
    let b = hash2(ix + 1.0, iy, seed);
    let c = hash2(ix, iy + 1.0, seed);
    let d = hash2(ix + 1.0, iy + 1.0, seed);
    a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy
}

fn write_bullet_hole_decal(atlas: &mut Atlas, layer: u32, kind: DecalKind) {
    let n = SIZE as usize;
    let mut height = vec![0.0f32; n * n];
    let mut color_img = RgbaImage::new(SIZE, SIZE);

    let seed = match kind {
        DecalKind::Concrete => 11.0,
        DecalKind::Metal => 27.0,
        DecalKind::Wood => 43.0,
    };

    for py in 0..n {
        let v = (py as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
        for px in 0..n {
            let u = (px as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
            let r = (u * u + v * v).sqrt();
            let ang = v.atan2(u);

            // Multi-octave angular irregularity for jagged crater & cracks
            let jag = value_noise(ang.cos() * 4.5 + 8.0, ang.sin() * 4.5 + 8.0, seed) * 0.14
                + value_noise(u * 14.0, v * 14.0, seed + 3.0) * 0.08;
            let crack_wave = ((ang * 7.0 + value_noise(u * 6.0, v * 6.0, seed + 5.0) * 2.6).sin())
                .abs()
                .powf(10.0);
            let crack = crack_wave * (1.0 - (r / 0.88).clamp(0.0, 1.0)) * 0.45;

            let (rgb, alpha, depth) = match kind {
                DecalKind::Concrete => {
                    let core = 1.0 - ((r + jag * 0.55) / 0.30).clamp(0.0, 1.0);
                    let chip_ring = 1.0 - ((r + jag * 0.85) / 0.58).clamp(0.0, 1.0);
                    let soot = (1.0 - (r / 0.86).clamp(0.0, 1.0)).powf(2.0);
                    let mask = (chip_ring * 1.45 + crack * 1.2 + soot * 0.38).clamp(0.0, 1.0);
                    let base = if core > 0.35 {
                        [14.0, 13.0, 12.0]
                    } else if chip_ring > 0.15 {
                        let c = 92.0 + value_noise(u * 26.0, v * 26.0, seed) * 45.0;
                        [c, c * 0.96, c * 0.90]
                    } else {
                        [34.0, 32.0, 30.0]
                    };
                    let d = -(core * 0.95 + chip_ring * 0.35 + crack * 0.25);
                    (base, mask, d)
                }
                DecalKind::Metal => {
                    let r_j = r + jag * 0.25;
                    let hole = 1.0 - (r_j / 0.26).clamp(0.0, 1.0);
                    let rim = (1.0 - ((r_j - 0.31).abs() / 0.11).clamp(0.0, 1.0)).powf(1.3);
                    let scorch = (1.0 - (r / 0.72).clamp(0.0, 1.0)).powf(2.2);
                    let mask = (hole * 1.6 + rim * 1.2 + scorch * 0.45).clamp(0.0, 1.0);
                    let base = if hole > 0.45 {
                        [10.0, 11.0, 13.0]
                    } else if rim > 0.25 {
                        // Exposed bright bare steel bevel around bullet entry
                        let shine = 175.0 + rim * 65.0;
                        [shine, shine * 1.01, shine * 1.04]
                    } else {
                        [28.0, 24.0, 22.0]
                    };
                    let d = -(hole * 1.05) + rim * 0.28;
                    (base, mask, d)
                }
                DecalKind::Wood => {
                    // Elongated fibrous splintering along X
                    let ell = ((u * 0.82).powi(2) + (v * 1.18).powi(2)).sqrt() + jag * 0.65;
                    let hole = 1.0 - (ell / 0.28).clamp(0.0, 1.0);
                    let fiber = ((v * 22.0 + value_noise(u * 10.0, v * 10.0, seed) * 2.0).sin())
                        .abs()
                        .powf(3.0);
                    let splinter = (1.0 - (ell / 0.72).clamp(0.0, 1.0)) * (0.45 + 0.55 * fiber);
                    let mask = (hole * 1.6 + splinter * 1.1).clamp(0.0, 1.0);
                    let base = if hole > 0.4 {
                        [16.0, 11.0, 8.0]
                    } else {
                        let tone = 135.0 + fiber * 55.0;
                        [tone, tone * 0.68, tone * 0.36]
                    };
                    let d = -(hole * 0.90 + splinter * 0.25);
                    (base, mask, d)
                }
            };

            height[py * n + px] = depth;
            color_img.put_pixel(
                px as u32,
                py as u32,
                image::Rgba([
                    rgb[0].clamp(0.0, 255.0) as u8,
                    rgb[1].clamp(0.0, 255.0) as u8,
                    rgb[2].clamp(0.0, 255.0) as u8,
                    (alpha * 255.0).clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }

    let layer_bytes = n * n * 4;
    let base = layer as usize * layer_bytes;
    atlas.albedo[base..base + layer_bytes].copy_from_slice(color_img.as_raw());

    for py in 0..n {
        let ym = py.saturating_sub(1);
        let yp = (py + 1).min(n - 1);
        for px in 0..n {
            let xm = px.saturating_sub(1);
            let xp = (px + 1).min(n - 1);
            let dx = (height[py * n + xp] - height[py * n + xm]) * 18.0;
            let dy = (height[yp * n + px] - height[ym * n + px]) * 18.0;
            let len = (dx * dx + dy * dy + 1.0).sqrt();
            let nx = -dx / len;
            let ny = -dy / len;
            let nz = 1.0 / len;
            let dst = base + (py * n + px) * 4;
            atlas.normal[dst] = ((nx * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            atlas.normal[dst + 1] = ((ny * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            atlas.normal[dst + 2] = ((nz * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            atlas.normal[dst + 3] = 255;
        }
    }
}

pub fn load(assets: &Path) -> Result<Atlas> {
    let layer_bytes = (SIZE * SIZE * 4) as usize;
    let total = layer_bytes * LAYERS as usize;
    let mut atlas = Atlas {
        albedo: vec![255; total],
        normal: vec![128; total],
    };
    let dev_dir = assets.join("textures/dev");

    for (layer, name) in DEV_SETS.iter().enumerate() {
        write_pbr_layer(
            &mut atlas,
            layer as u32,
            &dev_dir.join(format!("{name}_Albedo.png")),
            &dev_dir.join(format!("{name}_Roughness.png")),
            &dev_dir.join(format!("{name}_Normal.png")),
        )?;
    }

    // Layer WHITE: neutral calibration material (alpha 179 ≈ 0.70 so Material::roughness maps 1:1).
    let white_base = WHITE as usize * layer_bytes;
    for i in 0..(SIZE * SIZE) as usize {
        let dst = white_base + i * 4;
        atlas.albedo[dst] = 255;
        atlas.albedo[dst + 1] = 255;
        atlas.albedo[dst + 2] = 255;
        atlas.albedo[dst + 3] = 179;

        atlas.normal[dst] = 128;
        atlas.normal[dst + 1] = 128;
        atlas.normal[dst + 2] = 255;
        atlas.normal[dst + 3] = 255;
    }

    let barrel_dir = assets.join("textures/props/old_tin_barrel");
    write_pbr_layer(
        &mut atlas,
        BARREL,
        &barrel_dir.join("Barrel_Albedo.png"),
        &barrel_dir.join("Barrel_Roughness.png"),
        &barrel_dir.join("Barrel_Normal.png"),
    )?;

    let pistol_dir = assets.join("textures/weapons/pistol");
    write_pbr_layer(
        &mut atlas,
        ARMS,
        &pistol_dir.join("arms_albedo.png"),
        &pistol_dir.join("arms_roughness.png"),
        &pistol_dir.join("arms_normal.png"),
    )?;
    write_pbr_layer(
        &mut atlas,
        PISTOL,
        &pistol_dir.join("pistol_albedo.png"),
        &pistol_dir.join("pistol_roughness.png"),
        &pistol_dir.join("pistol_normal.png"),
    )?;

    write_bullet_hole_decal(&mut atlas, DECAL_CONCRETE, DecalKind::Concrete);
    write_bullet_hole_decal(&mut atlas, DECAL_METAL, DecalKind::Metal);
    write_bullet_hole_decal(&mut atlas, DECAL_WOOD, DecalKind::Wood);

    Ok(atlas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_dev_texture_atlas() {
        let atlas = load(&crate::asset_dir()).expect("dev textures should load");
        let expected = (SIZE * SIZE * LAYERS * 4) as usize;
        assert_eq!(atlas.albedo.len(), expected);
        assert_eq!(atlas.normal.len(), expected);
    }
}
