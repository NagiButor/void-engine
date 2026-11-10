//! V.O.I.D.'s Rust runtime. World and first-person geometry use metres, Y up.
pub mod audio;
pub mod glb;
pub mod mesh;
pub mod physics;
pub mod render;
pub mod scene;
pub mod textures;

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
            if assets.join("models").is_dir() {
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
            ("PBR", include_str!("../../../assets/shaders/pbr.wgsl")),
            ("post", include_str!("../../../assets/shaders/post.wgsl")),
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
