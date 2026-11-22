//! Run with VK_ICD_FILENAMES set to the lavapipe ICD and a library path.
fn main() -> Result<(), String> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("missing gfxstream library path")?;
    let renderer = indroid_gpu::Renderer::init(std::path::Path::new(&path), |_| {})?;
    let (version, caps) = renderer.capset(3)?;
    println!(
        "gfxstream Vulkan capset version {version}, {} bytes",
        caps.len()
    );
    Ok(())
}
