//! V.O.I.D.'s Rust runtime. Right-handed Y-up coordinate system, 1:1 SI metric scale (1 unit = 1 m).
pub mod audio;
pub mod glb;
pub mod input;
pub mod locomotion;
pub mod mesh;
pub mod physics;
pub mod render;
pub mod scene;
pub mod textures;
pub mod units;
pub mod viewmodel;

pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    pollster::block_on(future)
}

pub fn save_rgba(
    path: &std::path::Path,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(path, bytes, width, height, image::ColorType::Rgba8)?;
    Ok(())
}

pub fn asset_dir() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("VOID_ASSETS") {
        return path.into();
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1).take(5) {
            let assets = dir.join("assets");
            if assets.join("shaders").is_dir() {
                return assets;
            }
        }
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

#[cfg(test)]
mod shader_tests {
    #[test]
    fn gpu_shaders_parse_and_validate() {
        for (name, source) in [
            (
                "gbuffer",
                include_str!("../../../assets/shaders/gbuffer.wgsl"),
            ),
            (
                "deferred",
                include_str!("../../../assets/shaders/deferred.wgsl"),
            ),
            ("post", include_str!("../../../assets/shaders/post.wgsl")),
            ("bloom", include_str!("../../../assets/shaders/bloom.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }
}
