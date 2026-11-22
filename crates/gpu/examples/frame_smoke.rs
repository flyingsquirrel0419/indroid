//! Exercise renderer color-buffer upload and the optional frame capture hook.
use indroid_gpu::{Box3d, Renderer, ResourceCreate};
fn main() -> Result<(), String> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("missing gfxstream library path")?;
    let renderer = Renderer::init(std::path::Path::new(&path), |_| {})?;
    let api = renderer.api();
    let mut data = vec![255u8; 64 * 64 * 4];
    let mut iov = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: data.len(),
    };
    let mut resource = ResourceCreate {
        handle: 1,
        target: 2,
        format: 67,
        bind: 2,
        width: 64,
        height: 64,
        depth: 1,
        array_size: 1,
        ..Default::default()
    };
    let mut area = Box3d {
        x: 0,
        y: 0,
        z: 0,
        w: 64,
        h: 64,
        d: 1,
    };
    unsafe {
        let r = (api.resource_create)(&mut resource, &mut iov, 1);
        if r != 0 {
            return Err(format!("resource_create={r}"));
        }
        let r = (api.transfer_write_iov)(1, 0, 0, 0, 0, &mut area, 0, std::ptr::null_mut(), 0);
        if r != 0 {
            return Err(format!("transfer_write={r}"));
        }
        (api.flush)(1);
        (api.resource_unref)(1);
    }
    Ok(())
}
