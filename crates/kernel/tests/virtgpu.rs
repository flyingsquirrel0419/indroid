use std::process::Command;
use std::sync::{Arc, Mutex};

struct Fake;
impl indroid_kernel::virtgpu::GpuBackend for Fake {
    fn capset(&self, id: u32, version: u32) -> indroid_kernel::vfs::Result<Vec<u8>> {
        assert_eq!((id, version), (3, 0));
        Ok(b"fake\0".to_vec())
    }
}

#[test]
fn discovery_ioctls() {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("virtgpu-discovery");
    let rom = root.join("rom");
    std::fs::create_dir_all(rom.join("bin")).unwrap();
    let status = Command::new("aarch64-linux-gnu-gcc")
        .args(["-O2", "-static", "-o"])
        .arg(rom.join("bin/test"))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/c/virtgpu.c"))
        .status();
    match status {
        Ok(s) => assert!(s.success()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("cross compiler: {e}"),
    }
    let output = Arc::new(Mutex::new(Vec::new()));
    let out = output.clone();
    let k = indroid_kernel::Kernel::new(indroid_kernel::Config {
        rom,
        state: root.join("state"),
        overlays: vec![],
        console: Arc::new(move |b| out.lock().unwrap().extend_from_slice(b)),
    });
    k.set_gpu_backend(Some(Arc::new(Fake)));
    k.spawn("/bin/test", &["/bin/test"], &[]).unwrap();
    assert!(k.wait_idle(Some(std::time::Duration::from_secs(60))));
    let output = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
    assert!(
        output.contains("driver=virtio_gpu prime=0 param=0 caps=fake"),
        "{output}"
    );
}
