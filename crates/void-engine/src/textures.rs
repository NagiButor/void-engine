//! DevTextures material atlas: albedo/roughness (RGBA) and tangent-space normals.
use anyhow::{Context, Result};
use image::{imageops::FilterType, DynamicImage, RgbImage};
use std::path::Path;

pub const SIZE: u32 = 1024;
pub const LAYERS: u32 = 5;
pub const DEV_GRAY: u32 = 0;
pub const DEV_GRAY_INV: u32 = 1;
pub const DEV_ORANGE: u32 = 2;
pub const DEV_ORANGE_INV: u32 = 3;
pub const WHITE: u32 = 4;

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

pub fn load(assets: &Path) -> Result<Atlas> {
    let layer_bytes = (SIZE * SIZE * 4) as usize;
    let total = layer_bytes * LAYERS as usize;
    let mut atlas = Atlas {
        albedo: vec![255; total],
        normal: vec![128; total],
    };
    let dev_dir = assets.join("textures/dev");

    for (layer, name) in DEV_SETS.iter().enumerate() {
        let albedo_img = load_rgb_resized(&dev_dir.join(format!("{name}_Albedo.png")))?;
        let rough_img = load_rgb_resized(&dev_dir.join(format!("{name}_Roughness.png")))?;
        let normal_img = load_rgb_resized(&dev_dir.join(format!("{name}_Normal.png")))?;

        let base = layer * layer_bytes;
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
