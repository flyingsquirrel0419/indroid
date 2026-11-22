//! C API for indroid neo (real Android): install a ROM, boot it, stream the
//! console/logcat, receive display frames, inject touches.

use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, Mutex};

/// Line-oriented text callback (console output, logcat, progress).
pub type NeoTextFn = extern "C" fn(ctx: *mut c_void, text: *const u8, len: usize);

struct Sink(NeoTextFn, usize);
unsafe impl Send for Sink {}
unsafe impl Sync for Sink {}

/// Display frame callback: tightly packed RGBA8, `width * height * 4` bytes,
/// valid only during the call. Runs on a renderer thread.
pub type NeoFrameFn = extern "C" fn(ctx: *mut c_void, width: u32, height: u32, rgba: *const u8);

/// Registers the display sink. Call before `indroid_neo_boot`.
#[no_mangle]
pub extern "C" fn indroid_neo_set_frame_callback(frame: Option<NeoFrameFn>, ctx: *mut c_void) {
    indroid_gpu::set_frame_sink(frame.map(|f| (f, ctx)));
}

static KERNEL: Mutex<Option<Arc<indroid_kernel::Kernel>>> = Mutex::new(None);

unsafe fn path(p: *const c_char) -> Option<std::path::PathBuf> {
    if p.is_null() {
        return None;
    }
    Some(CStr::from_ptr(p).to_string_lossy().into_owned().into())
}

/// Installs the system image zip at `zip` into `out` (blocking; run it on a
/// background thread). Returns 0 on success.
///
/// # Safety
/// Paths must be NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn indroid_neo_install(
    zip: *const c_char,
    out: *const c_char,
    work: *const c_char,
    progress: NeoTextFn,
    ctx: *mut c_void,
) -> i32 {
    let (Some(zip), Some(out), Some(work)) = (path(zip), path(out), path(work)) else {
        return -1;
    };
    let mut cb = |m: &str| progress(ctx, m.as_ptr(), m.len());
    let r = indroid_rom::install(&zip, &out, &work, &mut cb);
    let _ = std::fs::remove_dir_all(&work);
    match r {
        Ok(()) => 0,
        Err(e) => {
            let m = format!("install failed: {e}");
            progress(ctx, m.as_ptr(), m.len());
            1
        }
    }
}

/// Boots `/system/bin/init second_stage` from the ROM at `rom` with writable
/// state (data, tmpfs) under `state`. Console and logcat lines go to `log`.
/// Returns 0 if init started.
///
/// # Safety
/// Paths must be NUL-terminated UTF-8; `log` must stay valid while running.
#[no_mangle]
pub unsafe extern "C" fn indroid_neo_boot(
    rom: *const c_char,
    state: *const c_char,
    overlays: *const c_char,
    cmdline: *const c_char,
    log: NeoTextFn,
    ctx: *mut c_void,
) -> i32 {
    let (Some(rom), Some(state)) = (path(rom), path(state)) else {
        return -1;
    };
    // ':'-separated overlay directories (first wins), e.g. the app bundle's
    // rom-overlay/emu13:rom-overlay/common.
    let overlays: Vec<std::path::PathBuf> = path(overlays)
        .map(|p| {
            p.to_string_lossy()
                .split(':')
                .filter(|s| !s.is_empty())
                .map(Into::into)
                .collect()
        })
        .unwrap_or_default();
    if let Some(c) = path(cmdline) {
        std::env::set_var("INDROID_CMDLINE", c);
    }
    // Mirror logcat into the console sink.
    std::env::set_var("INDROID_LOGCAT", "1");
    let sink = Arc::new(Sink(log, ctx as usize));
    let s2 = sink.clone();
    let k = indroid_kernel::Kernel::new(indroid_kernel::Config {
        rom,
        state,
        overlays,
        console: Arc::new(move |b: &[u8]| (s2.0)(s2.1 as *mut c_void, b.as_ptr(), b.len())),
    });
    // GPU: gfxstream backend library (INDROID_GFXSTREAM), e.g. from the app bundle.
    if let Some(lib) = std::env::var_os("INDROID_GFXSTREAM") {
        match indroid_gpu::KernelBackend::load(std::path::Path::new(&lib)) {
            Ok(backend) => k.set_gpu_backend(Some(backend)),
            Err(e) => {
                let m = format!("gpu: gfxstream unavailable ({e}); no display\n");
                (sink.0)(ctx, m.as_ptr(), m.len());
            }
        }
    }
    let r = k.spawn(
        "/system/bin/init",
        &["/system/bin/init", "second_stage"],
        &[],
    );
    *KERNEL.lock().unwrap() = Some(k);
    match r {
        Ok(_) => 0,
        Err(e) => {
            let m = format!("init failed to start: errno {}\n", -e);
            (sink.0)(ctx, m.as_ptr(), m.len());
            1
        }
    }
}

/// Touchscreen input in screen pixels (1080x2340 device space); id < 0 lifts.
#[no_mangle]
pub extern "C" fn indroid_neo_touch(slot: i32, id: i32, x: i32, y: i32) {
    if let Some(k) = KERNEL.lock().unwrap().as_ref() {
        k.touchscreen.touch(slot, id, x, y);
    }
}

/// Guest instructions executed so far (for a status display).
#[no_mangle]
pub extern "C" fn indroid_neo_instructions() -> u64 {
    KERNEL
        .lock()
        .unwrap()
        .as_ref()
        .map(|k| k.insns.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0)
}
