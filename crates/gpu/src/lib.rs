//! Runtime loading for the gfxstream 0.1.2 C ABI.
//!
//! The renderer is process-global. Callers must serialize initialization and
//! teardown, retain callbacks and backing iovecs for as long as gfxstream uses
//! them, and destroy all resources before unloading the library.
use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Param {
    pub key: u64,
    pub value: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Fence {
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ResourceCreate {
    pub handle: u32,
    pub target: u32,
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BlobCreate {
    pub blob_mem: u32,
    pub blob_flags: u32,
    pub blob_id: u64,
    pub size: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Handle {
    pub os_handle: i64,
    pub handle_type: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Box3d {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
    pub h: u32,
    pub d: u32,
}
#[repr(C)]
pub struct Command {
    pub ctx_id: u32,
    pub cmd_size: u32,
    pub cmd: *mut u8,
    pub num_in_fences: u32,
    pub fences: *mut Handle,
}

/// Owns the dlopen handle; symbol pointers remain valid while this is alive.
pub struct Library(*mut libc::c_void);
// dlopen/dlsym handles can be shared; the caller must obey renderer threading rules.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}
impl Library {
    pub fn open(path: &Path) -> Result<Arc<Self>, String> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(dl_error());
        }
        Ok(Arc::new(Self(handle)))
    }
    /// Look up an ABI symbol. The caller must supply the exact C signature and
    /// ensure the library outlives the returned pointer.
    ///
    /// # Safety
    /// Calling a symbol with an incorrect signature or after unload is undefined.
    pub unsafe fn symbol<T: Copy>(&self, name: &CStr) -> Result<T, String> {
        if std::mem::size_of::<T>() != std::mem::size_of::<*mut libc::c_void>() {
            return Err("symbol has incorrect pointer size".into());
        }
        libc::dlerror();
        let ptr = libc::dlsym(self.0, name.as_ptr());
        let error = libc::dlerror();
        if !error.is_null() {
            return Err(CStr::from_ptr(error).to_string_lossy().into_owned());
        }
        if ptr.is_null() {
            return Err("null symbol".into());
        }
        Ok(std::mem::transmute_copy(&ptr))
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.0);
        }
    }
}
fn dl_error() -> String {
    unsafe {
        let p = libc::dlerror();
        if p.is_null() {
            "dynamic loader error".into()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

/// Symbols from the pinned gfxstream ABI. The renderer itself is process-global.
/// Function pointers are intentionally unsafe: gfxstream retains some pointers,
/// and resource, context, fence and mapping lifetimes must be managed by caller.
pub struct Api {
    _library: Arc<Library>,
    pub init: unsafe extern "C" fn(*mut Param, u64) -> i32,
    pub teardown: unsafe extern "C" fn(),
    pub resource_create: unsafe extern "C" fn(*mut ResourceCreate, *mut libc::iovec, u32) -> i32,
    pub resource_unref: unsafe extern "C" fn(u32),
    pub context_create: unsafe extern "C" fn(u32, u32, *const libc::c_char, u32) -> i32,
    pub context_destroy: unsafe extern "C" fn(u32),
    pub ctx_attach_resource: unsafe extern "C" fn(i32, i32),
    pub ctx_detach_resource: unsafe extern "C" fn(i32, i32),
    pub submit_cmd: unsafe extern "C" fn(*mut Command) -> i32,
    pub create_blob: unsafe extern "C" fn(
        u32,
        u32,
        *const BlobCreate,
        *const libc::iovec,
        u32,
        *const Handle,
    ) -> i32,
    pub export_blob: unsafe extern "C" fn(u32, *mut Handle) -> i32,
    pub resource_map: unsafe extern "C" fn(u32, *mut *mut libc::c_void, *mut u64) -> i32,
    pub resource_unmap: unsafe extern "C" fn(u32) -> i32,
    pub transfer_read_iov: unsafe extern "C" fn(
        u32,
        u32,
        u32,
        u32,
        u32,
        *mut Box3d,
        u64,
        *mut libc::iovec,
        i32,
    ) -> i32,
    pub transfer_write_iov: unsafe extern "C" fn(
        u32,
        u32,
        i32,
        u32,
        u32,
        *mut Box3d,
        u64,
        *mut libc::iovec,
        u32,
    ) -> i32,
    pub get_cap_set: unsafe extern "C" fn(u32, *mut u32, *mut u32),
    pub fill_caps: unsafe extern "C" fn(u32, u32, *mut libc::c_void),
    pub create_fence: unsafe extern "C" fn(*const Fence) -> i32,
    pub flush: unsafe extern "C" fn(u32),
    pub set_frame_callback:
        Option<unsafe extern "C" fn(Option<unsafe extern "C" fn(u32, u32, *const u8)>)>,
    pub set_dma_resolver:
        Option<unsafe extern "C" fn(Option<unsafe extern "C" fn(u64) -> *mut libc::c_void>)>,
    pub set_puid_resolver:
        Option<unsafe extern "C" fn(Option<unsafe extern "C" fn(u32, libc::c_int) -> u64>)>,
}
impl Api {
    pub fn load(path: &Path) -> Result<Self, String> {
        let lib = Library::open(path)?;
        macro_rules! sym {
            ($name:literal) => {{
                let name = CStr::from_bytes_with_nul(concat!($name, "\0").as_bytes())
                    .expect("symbol literal");
                unsafe { lib.symbol(name)? }
            }};
        }
        Ok(Self {
            init: sym!("stream_renderer_init"),
            teardown: sym!("stream_renderer_teardown"),
            resource_create: sym!("stream_renderer_resource_create"),
            resource_unref: sym!("stream_renderer_resource_unref"),
            context_create: sym!("stream_renderer_context_create"),
            context_destroy: sym!("stream_renderer_context_destroy"),
            ctx_attach_resource: sym!("stream_renderer_ctx_attach_resource"),
            ctx_detach_resource: sym!("stream_renderer_ctx_detach_resource"),
            submit_cmd: sym!("stream_renderer_submit_cmd"),
            create_blob: sym!("stream_renderer_create_blob"),
            export_blob: sym!("stream_renderer_export_blob"),
            resource_map: sym!("stream_renderer_resource_map"),
            resource_unmap: sym!("stream_renderer_resource_unmap"),
            transfer_read_iov: sym!("stream_renderer_transfer_read_iov"),
            transfer_write_iov: sym!("stream_renderer_transfer_write_iov"),
            get_cap_set: sym!("stream_renderer_get_cap_set"),
            fill_caps: sym!("stream_renderer_fill_caps"),
            create_fence: sym!("stream_renderer_create_fence"),
            flush: sym!("stream_renderer_flush"),
            set_frame_callback: unsafe { lib.symbol(c"indroid_set_frame_callback").ok() },
            set_dma_resolver: unsafe { lib.symbol(c"indroid_set_dma_resolver").ok() },
            set_puid_resolver: unsafe { lib.symbol(c"indroid_set_puid_resolver").ok() },
            _library: lib,
        })
    }
}

/// Process-global renderer instance. Gfxstream invokes `on_fence` on its own
/// threads; the callback must not panic or call back into the renderer.
pub struct Renderer {
    api: Api,
    callback: Box<FenceCallback>,
    api_lock: std::sync::Mutex<()>,
}
type FenceCallback = Box<dyn Fn(Fence) + Send + Sync>;
static RENDERER_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// gfxstream asks which guest process a context belongs to (see
/// indroid_kernel::virtgpu::context_puid).
unsafe extern "C" fn puid_resolve(ctx: u32, release: libc::c_int) -> u64 {
    indroid_kernel::virtgpu::context_puid(ctx, release != 0)
}

/// gfxstream asks for the host address of guest DMA buffers (goldfish
/// address-space blocks) on its render threads.
unsafe extern "C" fn dma_resolve(paddr: u64) -> *mut libc::c_void {
    let p = indroid_kernel::virtgpu::dma_host_addr(paddr);
    if p.is_null() {
        eprintln!("gpu: unresolved DMA address {paddr:#x}");
    }
    p.cast()
}

unsafe extern "C" fn fence_trampoline(cookie: *mut libc::c_void, fence: *mut Fence) {
    if cookie.is_null() || fence.is_null() {
        return;
    }
    let callback = &*(cookie as *const FenceCallback);
    let value = *fence;
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(value)));
}

/// Receives every composed frame: tightly packed RGBA8, `width * height * 4`
/// bytes, valid only for the duration of the call.
pub type FrameFn = extern "C" fn(ctx: *mut libc::c_void, width: u32, height: u32, rgba: *const u8);

static FRAME_SINK: std::sync::Mutex<Option<(FrameFn, usize)>> = std::sync::Mutex::new(None);

/// Route composed frames to `sink` (the host display). Pass `None` to stop.
/// Call before the renderer starts.
pub fn set_frame_sink(sink: Option<(FrameFn, *mut libc::c_void)>) {
    *FRAME_SINK.lock().unwrap_or_else(|e| e.into_inner()) = sink.map(|(f, c)| (f, c as usize));
}

unsafe extern "C" fn dump_frame(width: u32, height: u32, pixels: *const u8) {
    if pixels.is_null() || width == 0 || height == 0 || width > 16384 || height > 16384 {
        return;
    }
    let sink = *FRAME_SINK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((f, ctx)) = sink {
        f(ctx as *mut libc::c_void, width, height, pixels);
    }
    let Some(path) = std::env::var_os("INDROID_FB_DUMP") else {
        return;
    };
    let result = std::panic::catch_unwind(|| {
        let len = width as usize * height as usize * 4;
        let data = unsafe { std::slice::from_raw_parts(pixels, len) };
        let file = std::fs::File::create(path)?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
        writer.write_image_data(data).map_err(std::io::Error::other)
    });
    if let Ok(Err(e)) = result {
        eprintln!("frame dump failed: {e}");
    }
}

impl Renderer {
    /// Initialize a headless Vulkan guest renderer. Only one renderer may exist
    /// in a process, as required by gfxstream's global state.
    pub fn init(
        path: &Path,
        on_fence: impl Fn(Fence) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        use std::sync::atomic::Ordering;
        let api = Api::load(path)?;
        if RENDERER_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("gfxstream renderer already active".into());
        }
        let mut callback: Box<FenceCallback> = Box::new(Box::new(on_fence));
        let mut params = [
            Param {
                key: 1,
                value: (&mut *callback as *mut FenceCallback) as u64,
            },
            Param {
                key: 2,
                value: (1 << 3) | (1 << 5),
            },
            Param {
                key: 3,
                value: fence_trampoline as *const () as u64,
            },
            Param {
                key: 4,
                value: 1080,
            },
            Param {
                key: 5,
                value: 2340,
            },
        ];
        let result = unsafe { (api.init)(params.as_mut_ptr(), params.len() as u64) };
        if result != 0 {
            RENDERER_ACTIVE.store(false, Ordering::Release);
            return Err(format!("stream_renderer_init returned {result}"));
        }
        match api.set_dma_resolver {
            Some(set_resolver) => unsafe { set_resolver(Some(dma_resolve)) },
            None => eprintln!("gfxstream backend has no indroid DMA resolver hook"),
        }
        match api.set_puid_resolver {
            Some(set_resolver) => unsafe { set_resolver(Some(puid_resolve)) },
            None => eprintln!("gfxstream backend has no indroid puid resolver hook"),
        }
        let has_sink = FRAME_SINK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        if has_sink || std::env::var_os("INDROID_FB_DUMP").is_some() {
            match api.set_frame_callback {
                Some(set_callback) => unsafe { set_callback(Some(dump_frame)) },
                None => eprintln!("gfxstream backend has no indroid frame-capture hook"),
            }
        }
        Ok(Self {
            api,
            callback,
            api_lock: std::sync::Mutex::new(()),
        })
    }

    /// Access raw ABI calls; callers are responsible for their safety contracts.
    pub fn api(&self) -> &Api {
        &self.api
    }

    /// Return a capability set, subject to a defensive allocation bound.
    pub fn capset(&self, id: u32) -> Result<(u32, Vec<u8>), String> {
        let _guard = self.api_lock.lock().unwrap_or_else(|e| e.into_inner());
        let (mut version, mut size) = (0, 0);
        unsafe { (self.api.get_cap_set)(id, &mut version, &mut size) };
        if size == 0 {
            return Err(format!("unsupported capset {id}"));
        }
        if size > 16 * 1024 * 1024 {
            return Err("gfxstream capset is too large".into());
        }
        let mut data = vec![0; size as usize];
        unsafe { (self.api.fill_caps)(id, version, data.as_mut_ptr().cast()) };
        Ok((version, data))
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        // Retain callback storage through teardown, which joins renderer work.
        let _ = &self.callback;
        if let Some(set_callback) = self.api.set_frame_callback {
            unsafe { set_callback(None) };
        }
        unsafe { (self.api.teardown)() };
        RENDERER_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Adapter for the kernel's renderer-independent virtio-gpu boundary.
pub struct KernelBackend {
    renderer: Arc<Renderer>,
    fences: Arc<
        std::sync::Mutex<
            std::collections::HashMap<u64, std::sync::Weak<indroid_kernel::virtgpu::GpuFence>>,
        >,
    >,
    next_fence: std::sync::atomic::AtomicU64,
    contexts: std::sync::Mutex<std::collections::HashMap<u32, Arc<KernelContext>>>,
}
impl KernelBackend {
    pub fn load(path: &Path) -> Result<Arc<Self>, String> {
        let fences = Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
            u64,
            std::sync::Weak<indroid_kernel::virtgpu::GpuFence>,
        >::new()));
        let callback_fences = fences.clone();
        let renderer = Renderer::init(path, move |fence| {
            let pending = callback_fences
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&fence.fence_id);
            if gpu_debug() {
                eprintln!(
                    "gpu: fence {} ctx {} ring {} signaled",
                    fence.fence_id, fence.ctx_id, fence.ring_idx
                );
            }
            if let Some(fence) = pending.and_then(|w| w.upgrade()) {
                fence.signal();
            }
        })?;
        Ok(Arc::new(Self {
            renderer: Arc::new(renderer),
            fences,
            next_fence: std::sync::atomic::AtomicU64::new(1),
            contexts: std::sync::Mutex::new(std::collections::HashMap::new()),
        }))
    }
}

struct KernelContext {
    renderer: Arc<Renderer>,
    id: u32,
}
impl Drop for KernelContext {
    fn drop(&mut self) {
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe { (self.renderer.api.context_destroy)(self.id) }
    }
}
struct GuestBacking(std::cell::UnsafeCell<Box<[u8]>>);
// The guest and renderer synchronize access according to the virtio protocol.
unsafe impl Send for GuestBacking {}
unsafe impl Sync for GuestBacking {}
struct KernelResource {
    renderer: Arc<Renderer>,
    id: u32,
    backing: Option<GuestBacking>,
    _context: Arc<KernelContext>,
}
impl Drop for KernelResource {
    fn drop(&mut self) {
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            if self.backing.is_none() {
                (self.renderer.api.resource_unmap)(self.id);
            }
            (self.renderer.api.resource_unref)(self.id);
        }
    }
}
impl indroid_kernel::virtgpu::GpuResource for KernelResource {
    fn id(&self) -> u32 {
        self.id
    }
    fn transfer(
        &self,
        ctx: u32,
        to_host: bool,
        args: indroid_kernel::virtgpu::TransferArgs,
    ) -> indroid_kernel::vfs::Result<()> {
        let [x, y, z, w, h, d] = args.region;
        let mut region = Box3d { x, y, z, w, h, d };
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let ret = unsafe {
            if to_host {
                (self.renderer.api.transfer_write_iov)(
                    self.id,
                    ctx,
                    args.level as i32,
                    args.stride,
                    args.layer_stride,
                    &mut region,
                    args.offset as u64,
                    std::ptr::null_mut(),
                    0,
                )
            } else {
                (self.renderer.api.transfer_read_iov)(
                    self.id,
                    ctx,
                    args.level,
                    args.stride,
                    args.layer_stride,
                    &mut region,
                    args.offset as u64,
                    std::ptr::null_mut(),
                    0,
                )
            }
        };
        renderer_result(ret)
    }

    fn map(self: Arc<Self>) -> indroid_kernel::vfs::Result<Arc<indroid_kernel::mm::SharedMem>> {
        if let Some(backing) = &self.backing {
            let ptr = unsafe { (*backing.0.get()).as_mut_ptr() };
            let size = unsafe { (&*backing.0.get()).len() } as u64;
            let owner: Arc<dyn Send + Sync> = self;
            return unsafe { indroid_kernel::mm::SharedMem::from_external(ptr, size, owner) }
                .ok_or(indroid_kernel::errno::EINVAL);
        }
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut ptr = std::ptr::null_mut();
        let mut size = 0;
        let ret = unsafe { (self.renderer.api.resource_map)(self.id, &mut ptr, &mut size) };
        if ret != 0 {
            return Err(if ret < 0 { ret as i64 } else { -(ret as i64) });
        }
        drop(_guard);
        let owner: Arc<dyn Send + Sync> = self;
        unsafe { indroid_kernel::mm::SharedMem::from_external(ptr.cast(), size, owner) }
            .ok_or(indroid_kernel::errno::EINVAL)
    }
}
impl indroid_kernel::virtgpu::GpuBackend for KernelBackend {
    fn capset(&self, id: u32, version: u32) -> indroid_kernel::vfs::Result<Vec<u8>> {
        let (max_version, data) = self
            .renderer
            .capset(id)
            .map_err(|_| indroid_kernel::errno::EINVAL)?;
        if version > max_version {
            return Err(indroid_kernel::errno::EINVAL);
        }
        Ok(data)
    }
    fn supports_context(&self) -> bool {
        true
    }
    fn context_create(&self, id: u32, capset: u32) -> indroid_kernel::vfs::Result<()> {
        let name = b"indroid\0";
        let ret = {
            let _guard = self
                .renderer
                .api_lock
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            unsafe { (self.renderer.api.context_create)(id, 6, name.as_ptr().cast(), capset) }
        };
        renderer_result(ret)?;
        self.contexts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                Arc::new(KernelContext {
                    renderer: self.renderer.clone(),
                    id,
                }),
            );
        Ok(())
    }
    fn context_destroy(&self, id: u32) {
        self.contexts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
    fn create_3d(
        &self,
        ctx: u32,
        id: u32,
        args: indroid_kernel::virtgpu::ResourceArgs,
    ) -> indroid_kernel::vfs::Result<Arc<dyn indroid_kernel::virtgpu::GpuResource>> {
        let context = self
            .contexts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&ctx)
            .cloned()
            .ok_or(indroid_kernel::errno::EINVAL)?;
        let [target, format, bind, width, height, depth, array_size, last_level, nr_samples, flags] =
            args.values;
        let mut create = ResourceCreate {
            handle: id,
            target,
            format,
            bind,
            width,
            height,
            depth,
            array_size,
            last_level,
            nr_samples,
            flags,
        };
        let len = ((args.size as usize) + 4095) & !4095;
        let backing = GuestBacking(std::cell::UnsafeCell::new(
            vec![0u8; len].into_boxed_slice(),
        ));
        let mut iov = libc::iovec {
            iov_base: unsafe { (*backing.0.get()).as_mut_ptr().cast() },
            iov_len: len,
        };
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        renderer_result(unsafe { (self.renderer.api.resource_create)(&mut create, &mut iov, 1) })?;
        unsafe {
            (self.renderer.api.ctx_attach_resource)(ctx as i32, id as i32);
        }
        Ok(Arc::new(KernelResource {
            renderer: self.renderer.clone(),
            id,
            backing: Some(backing),
            _context: context,
        }))
    }
    fn create_blob(
        &self,
        ctx: u32,
        id: u32,
        mem: u32,
        flags: u32,
        blob_id: u64,
        size: u64,
    ) -> indroid_kernel::vfs::Result<Arc<dyn indroid_kernel::virtgpu::GpuResource>> {
        let context = self
            .contexts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&ctx)
            .cloned()
            .ok_or(indroid_kernel::errno::EINVAL)?;
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let args = BlobCreate {
            blob_mem: mem,
            blob_flags: flags,
            blob_id,
            size,
        };
        let ret = unsafe {
            (self.renderer.api.create_blob)(ctx, id, &args, std::ptr::null(), 0, std::ptr::null())
        };
        renderer_result(ret)?;
        // Attaching allows gfxstream to detach an ASG on resource destruction;
        // its cleanup worker retains the ring until the ASG consumer exits.
        unsafe {
            (self.renderer.api.ctx_attach_resource)(ctx as i32, id as i32);
        }
        Ok(Arc::new(KernelResource {
            renderer: self.renderer.clone(),
            id,
            backing: None,
            _context: context,
        }))
    }
    fn fence(
        &self,
        ctx: u32,
        ring: Option<u8>,
        signal: Arc<indroid_kernel::virtgpu::GpuFence>,
    ) -> indroid_kernel::vfs::Result<()> {
        let id = self
            .next_fence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.fences
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, Arc::downgrade(&signal));
        let fence = Fence {
            flags: 1 | if ring.is_some() { 2 } else { 0 },
            fence_id: id,
            ctx_id: ctx,
            ring_idx: ring.unwrap_or(0),
        };
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let result = renderer_result(unsafe { (self.renderer.api.create_fence)(&fence) });
        if gpu_debug() {
            eprintln!("gpu: fence {id} ctx {ctx} ring {ring:?} created: {result:?}");
        }
        if result.is_err() {
            self.fences
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
        }
        result
    }
    fn submit(&self, ctx: u32, bytes: &mut [u8]) -> indroid_kernel::vfs::Result<()> {
        let _guard = self
            .renderer
            .api_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut cmd = Command {
            ctx_id: ctx,
            cmd_size: bytes.len() as u32,
            cmd: bytes.as_mut_ptr(),
            num_in_fences: 0,
            fences: std::ptr::null_mut(),
        };
        renderer_result(unsafe { (self.renderer.api.submit_cmd)(&mut cmd) })
    }
}
fn renderer_result(ret: i32) -> indroid_kernel::vfs::Result<()> {
    if ret == 0 {
        Ok(())
    } else if ret < 0 {
        Err(ret as i64)
    } else {
        Err(-(ret as i64))
    }
}

fn gpu_debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("INDROID_GPU_DEBUG").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_library_is_an_error() {
        assert!(Library::open(Path::new("/nonexistent/indroid-gfxstream.so")).is_err());
    }
}
