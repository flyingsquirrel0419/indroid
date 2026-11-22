//! Initial virtio-gpu DRM device boundary.
//!
//! Only discovery ioctls are implemented; resource and submission operations
//! deliberately return ENOTTY until their memory and fence lifetimes exist.
use crate::errno::*;
use crate::mm::SharedMem;
use crate::task::Ctx;
use crate::vfs::{self, Node, Stat};
use parking_lot::Mutex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Renderer discovery, independent of the host renderer implementation.
pub struct GpuFence {
    signaled: AtomicBool,
    queue: Arc<crate::sync::WaitQueue>,
    /// CLOCK_MONOTONIC nanoseconds at signal time (sync_fence_info).
    time: AtomicU64,
}
impl Default for GpuFence {
    fn default() -> Self {
        Self {
            signaled: AtomicBool::new(false),
            queue: crate::sync::WaitQueue::new(),
            time: AtomicU64::new(0),
        }
    }
}
impl GpuFence {
    pub fn signal(&self) {
        self.time.store(
            crate::time::monotonic().as_nanos() as u64,
            Ordering::Relaxed,
        );
        self.signaled.store(true, Ordering::Release);
        self.queue.notify();
    }
    fn wait(&self, nowait: bool) -> vfs::Result<()> {
        loop {
            let epoch = self.queue.epoch();
            if self.signaled.load(Ordering::Acquire) {
                return Ok(());
            }
            if nowait {
                return Err(EBUSY);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            self.queue.wait_change(epoch, None);
        }
    }
}
struct DmaBuf {
    id: u64,
    resource: Arc<dyn GpuResource>,
    size: u64,
    mem: u32,
}
impl Node for DmaBuf {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            ino: self.id,
            mode: vfs::S_IFREG | 0o600,
            nlink: 1,
            size: self.size as i64,
            ..Default::default()
        }
    }
    fn shared_mem(&self) -> Option<Arc<SharedMem>> {
        self.resource.clone().map().ok()
    }
    fn mmap_requires_shared_mem(&self) -> bool {
        true
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
struct SyncFile {
    id: u64,
    fence: Arc<GpuFence>,
}
impl Node for SyncFile {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            ino: self.id,
            mode: vfs::S_IFREG | 0o600,
            nlink: 1,
            ..Default::default()
        }
    }
    fn poll(&self) -> u32 {
        if self.fence.signaled.load(Ordering::Acquire) {
            vfs::POLLIN
        } else {
            0
        }
    }
    fn wait_queue(&self) -> Option<Arc<crate::sync::WaitQueue>> {
        Some(self.fence.queue.clone())
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> vfs::Result<i64> {
        match cmd {
            // SYNC_IOC_FILE_INFO
            0xc038_3e04 => {
                let signaled = self.fence.signaled.load(Ordering::Acquire);
                if std::env::var_os("INDROID_GPU_DEBUG").is_some() {
                    eprintln!("sync_file {}: info signaled={signaled}", self.id);
                }
                let mut info = [0u8; 56];
                info[..10].copy_from_slice(b"indroid-gpu");
                info[32..36].copy_from_slice(&(signaled as i32).to_le_bytes());
                let wanted = ctx.mm.read_u32(arg + 40).ok_or(EFAULT)?;
                let ptr = ctx.mm.read_u64(arg + 48).ok_or(EFAULT)?;
                info[40..44].copy_from_slice(&1u32.to_le_bytes());
                info[48..56].copy_from_slice(&ptr.to_le_bytes());
                if wanted > 0 {
                    let mut fence = [0u8; 80];
                    fence[..10].copy_from_slice(b"virtio_gpu");
                    fence[32..42].copy_from_slice(b"virtio_gpu");
                    fence[64..68].copy_from_slice(&(signaled as i32).to_le_bytes());
                    let time = self.fence.time.load(Ordering::Relaxed);
                    fence[72..80].copy_from_slice(&time.to_le_bytes());
                    if !ctx.mm.write(ptr, &fence) {
                        return Err(EFAULT);
                    }
                }
                if !ctx.mm.write(arg, &info) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            // SYNC_IOC_MERGE
            0xc030_3e03 => {
                let fd2 = ctx.mm.read_u32(arg + 32).ok_or(EFAULT)? as i32;
                let other = ctx.fds().lock().get(fd2)?;
                let other = other
                    .node
                    .as_any()
                    .downcast_ref::<SyncFile>()
                    .ok_or(EINVAL)?
                    .fence
                    .clone();
                let merged = Arc::new(GpuFence::default());
                let parts = [self.fence.clone(), other];
                if parts.iter().all(|f| f.signaled.load(Ordering::Acquire)) {
                    merged.signal();
                } else {
                    let m = merged.clone();
                    std::thread::Builder::new()
                        .name("sync-merge".into())
                        .spawn(move || {
                            for f in &parts {
                                loop {
                                    let epoch = f.queue.epoch();
                                    if f.signaled.load(Ordering::Acquire) {
                                        break;
                                    }
                                    f.queue.wait_change(epoch, None);
                                }
                            }
                            m.signal();
                        })
                        .map_err(|_| ENOMEM)?;
                }
                let node = Arc::new(SyncFile {
                    id: vfs::new_id(),
                    fence: merged,
                });
                let file =
                    crate::fd::OpenFile::new(node, crate::fd::O_RDWR, "anon_inode:[sync_file]");
                let table = ctx.fds();
                let fd = table.lock().insert(file, true)?;
                if !ctx.mm.write_u32(arg + 36, fd as u32) {
                    let _ = table.lock().close(fd);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            _ => Err(ENOTTY),
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `/dev/goldfish_sync`: the emulator's host-signalled timeline. Host
/// composition is synchronous here (rcCompose returns after composing), so
/// work queued by HWC gets an already signalled sync_file.
pub struct GoldfishSync {
    id: u64,
}
impl GoldfishSync {
    pub fn open() -> vfs::FileRef {
        Arc::new(Self { id: vfs::new_id() })
    }
}
impl Node for GoldfishSync {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            ino: self.id,
            mode: vfs::S_IFCHR | 0o666,
            nlink: 1,
            ..Default::default()
        }
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> vfs::Result<i64> {
        match cmd {
            // GOLDFISH_SYNC_IOC_QUEUE_WORK
            0xc018_4000 => {
                if std::env::var_os("INDROID_GPU_DEBUG").is_some() {
                    eprintln!("goldfish_sync: queue_work");
                }
                let fence = Arc::new(GpuFence::default());
                fence.signal();
                let node = Arc::new(SyncFile {
                    id: vfs::new_id(),
                    fence,
                });
                let file =
                    crate::fd::OpenFile::new(node, crate::fd::O_RDWR, "anon_inode:[sync_file]");
                let table = ctx.fds();
                let fd = table.lock().insert(file, true)?;
                if !ctx.mm.write_u32(arg + 16, fd as u32) {
                    let _ = table.lock().close(fd);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            // GOLDFISH_SYNC_IOC_SIGNAL
            0xc018_4001 => Ok(0),
            _ => Err(ENOTTY),
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Copy)]
pub struct ResourceArgs {
    pub values: [u32; 10],
    pub size: u32,
}
#[derive(Clone, Copy)]
pub struct TransferArgs {
    pub region: [u32; 6],
    pub level: u32,
    pub offset: u32,
    pub stride: u32,
    pub layer_stride: u32,
}
pub trait GpuResource: Send + Sync {
    fn id(&self) -> u32;
    fn transfer(&self, _ctx: u32, _to_host: bool, _args: TransferArgs) -> vfs::Result<()> {
        Err(ENOSYS)
    }

    fn map(self: Arc<Self>) -> vfs::Result<Arc<SharedMem>>;
}

pub trait GpuBackend: Send + Sync {
    /// Return the supported version and bytes for a capability set.
    fn capset(&self, id: u32, version: u32) -> vfs::Result<Vec<u8>>;
    fn supports_context(&self) -> bool {
        false
    }
    fn context_create(&self, _id: u32, _capset: u32) -> vfs::Result<()> {
        Err(ENOSYS)
    }
    fn context_destroy(&self, _id: u32) {}
    fn create_3d(
        &self,
        _ctx: u32,
        _id: u32,
        _args: ResourceArgs,
    ) -> vfs::Result<Arc<dyn GpuResource>> {
        Err(ENOSYS)
    }
    fn create_blob(
        &self,
        _ctx: u32,
        _id: u32,
        _mem: u32,
        _flags: u32,
        _blob_id: u64,
        _size: u64,
    ) -> vfs::Result<Arc<dyn GpuResource>> {
        Err(ENOSYS)
    }
    fn submit(&self, _ctx: u32, _cmd: &mut [u8]) -> vfs::Result<()> {
        Err(ENOSYS)
    }
    fn fence(&self, _ctx: u32, _ring: Option<u8>, _signal: Arc<GpuFence>) -> vfs::Result<()> {
        Err(ENOSYS)
    }
}

pub type BackendSlot = Arc<RwLock<Option<Arc<dyn GpuBackend>>>>;

pub struct VirtGpu {
    id: u64,
    minor: u32,
    backend: BackendSlot,
    state: Mutex<State>,
}
struct State {
    context: Option<(Arc<dyn GpuBackend>, u32)>,
    resources: HashMap<u32, (Arc<dyn GpuResource>, u64, u32)>,
    next_handle: u32,
    last_fence: Option<Arc<GpuFence>>,
    legacy_response: Option<u32>,
    legacy_asg: Option<LegacyAsg>,
}
// Android 13 goldfish used a response-page/hostmem-id handshake before
// gfxstream switched ASG creation to an explicitly supplied ring blob.
struct LegacyAsg {
    ring: Arc<dyn GpuResource>,
    ring_size: u32,
    buffer_size: u32,
    hostmem_id: u64,
}
impl State {
    fn ensure_context(
        &mut self,
        backend: &Arc<dyn GpuBackend>,
        pid: i32,
    ) -> vfs::Result<(Arc<dyn GpuBackend>, u32)> {
        if let Some(context) = &self.context {
            return Ok(context.clone());
        }
        let id = new_context_id(pid);
        backend.context_create(id, 0)?;
        self.context = Some((backend.clone(), id));
        Ok((backend.clone(), id))
    }
}
static NEXT_CONTEXT: AtomicU32 = AtomicU32::new(1);

/// Renderer process identity per context: (context -> puid, puid -> live
/// contexts). Guest Vulkan numbers commands per process, and the renderer
/// checks them per puid, so all contexts of one guest process share a puid.
static CONTEXT_PUIDS: std::sync::LazyLock<
    Mutex<(
        std::collections::HashMap<u32, u64>,
        std::collections::HashMap<u64, usize>,
    )>,
> = std::sync::LazyLock::new(Default::default);

fn new_context_id(pid: i32) -> u32 {
    let id = NEXT_CONTEXT.fetch_add(1, Ordering::Relaxed);
    // Above the context-id range so it never collides with a bare context id.
    let puid = (1u64 << 32) | pid as u32 as u64;
    let mut map = CONTEXT_PUIDS.lock();
    map.0.insert(id, puid);
    *map.1.entry(puid).or_default() += 1;
    id
}

/// The renderer's puid for a context. With `release`, the context is being
/// destroyed: returns the puid to clean up once its last context is gone,
/// else 0.
pub fn context_puid(ctx: u32, release: bool) -> u64 {
    let mut map = CONTEXT_PUIDS.lock();
    let Some(&puid) = map.0.get(&ctx) else {
        return ctx as u64;
    };
    if !release {
        return puid;
    }
    map.0.remove(&ctx);
    let left = map.1.get_mut(&puid).map(|n| {
        *n -= 1;
        *n
    });
    if left == Some(0) {
        map.1.remove(&puid);
        puid
    } else {
        0
    }
}
static NEXT_RESOURCE: AtomicU32 = AtomicU32::new(1);

impl VirtGpu {
    pub fn open(backend: BackendSlot, minor: u32) -> vfs::FileRef {
        Arc::new(Self {
            id: vfs::new_id(),
            minor,
            backend,
            state: Mutex::new(State {
                context: None,
                resources: HashMap::new(),
                next_handle: 1,
                last_fence: None,
                legacy_response: None,
                legacy_asg: None,
            }),
        })
    }
}
impl Node for VirtGpu {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            ino: self.id,
            mode: vfs::S_IFCHR | 0o666,
            nlink: 1,
            rdev: (226 << 8) | self.minor as u64,
            ..Default::default()
        }
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> vfs::Result<i64> {
        if std::env::var_os("INDROID_GPU_TRACE").is_some() {
            eprintln!("[virtgpu] ioctl {cmd:#010x} arg={arg:#x}");
        }
        let backend = self.backend.read().clone().ok_or(ENODEV)?;
        match cmd {
            // No PRIME, syncobj, modesetting or dumb buffer support yet.
            0xc010_640c => {
                let cap = ctx.mm.read_u64(arg).ok_or(EFAULT)?;
                let value = match cap {
                    5 if backend.supports_context() => 3,
                    1..=5 | 7..=9 | 0x10..=0x15 => 0,
                    6 => 1, // DRM_CAP_TIMESTAMP_MONOTONIC
                    _ => return Err(EINVAL),
                };
                if !ctx.mm.write_u64(arg + 8, value) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            // drm_version: the three size_t/pointer pairs are 8-byte aligned.
            0xc040_6400 => {
                for (off, value) in [(0, 0), (4, 1), (8, 0)] {
                    if !ctx.mm.write_u32(arg + off, value) {
                        return Err(EFAULT);
                    }
                }
                for (off, value) in [
                    (16, &b"virtio_gpu"[..]),
                    (32, &b"0"[..]),
                    (48, &b"virtio GPU"[..]),
                ] {
                    let size = ctx.mm.read_u64(arg + off).ok_or(EFAULT)?;
                    let ptr = ctx.mm.read_u64(arg + off + 8).ok_or(EFAULT)?;
                    let n = (size as usize).min(value.len());
                    if n != 0 && !ctx.mm.write(ptr, &value[..n]) {
                        return Err(EFAULT);
                    }
                    if !ctx.mm.write_u64(arg + off, value.len() as u64) {
                        return Err(EFAULT);
                    }
                }
                Ok(0)
            }
            // GETPARAM: no resource/context features are advertised until implemented.
            0xc010_6443 => {
                let param = ctx.mm.read_u64(arg).ok_or(EFAULT)?;
                let ptr = ctx.mm.read_u64(arg + 8).ok_or(EFAULT)?;
                let value = match param {
                    1 | 2 | 3 | 4 | 6 if backend.supports_context() => 1,
                    7 if backend.supports_context() => 1 << 3,
                    1..=7 => 0,
                    _ => return Err(EINVAL),
                };
                if !ctx.mm.write_u64(ptr, value) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc00c_642d => {
                let handle = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let flags = ctx.mm.read_u32(arg + 4).ok_or(EFAULT)?;
                if flags & !3 != 0 {
                    return Err(EINVAL);
                }
                let state = self.state.lock();
                let (resource, size, mem) = state.resources.get(&handle).ok_or(ENOENT)?;
                let node = Arc::new(DmaBuf {
                    id: vfs::new_id(),
                    resource: resource.clone(),
                    size: *size,
                    mem: *mem,
                });
                drop(state);
                let file = crate::fd::OpenFile::new(node, crate::fd::O_RDWR, "anon_inode:dmabuf");
                let table = ctx.fds();
                let fd = table.lock().insert(file, flags & 1 != 0)?;
                if !ctx.mm.write_u32(arg + 8, fd as u32) {
                    let _ = table.lock().close(fd);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc00c_642e => {
                let fd = ctx.mm.read_u32(arg + 8).ok_or(EFAULT)? as i32;
                let table = ctx.fds();
                let file = table.lock().get(fd)?;
                let buffer = file.node.as_any().downcast_ref::<DmaBuf>().ok_or(EINVAL)?;
                let mut state = self.state.lock();
                let handle = state.next_handle;
                state.next_handle = handle.checked_add(1).ok_or(ENOMEM)?;
                state
                    .resources
                    .insert(handle, (buffer.resource.clone(), buffer.size, buffer.mem));
                if !ctx.mm.write_u32(arg, handle) {
                    state.resources.remove(&handle);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc038_6444 => {
                let mut values = [0u32; 10];
                for (i, value) in values.iter_mut().enumerate() {
                    *value = ctx.mm.read_u32(arg + i as u64 * 4).ok_or(EFAULT)?;
                }
                let size = ctx.mm.read_u32(arg + 48).ok_or(EFAULT)?;
                if size == 0 || size > (1 << 30) {
                    return Err(EINVAL);
                }
                let mut state = self.state.lock();
                let (backend, context) = state.ensure_context(&backend, ctx.process.pid)?;
                let id = NEXT_RESOURCE.fetch_add(1, Ordering::Relaxed);
                let resource = backend.create_3d(context, id, ResourceArgs { values, size })?;
                let handle = state.next_handle;
                state.next_handle = handle.checked_add(1).ok_or(ENOMEM)?;
                state.resources.insert(handle, (resource, size as u64, 0));
                if values == [0, 64, 1 << 17, 4096, 1, 1, 0, 0, 0, 0] && size == 4096 {
                    state.legacy_response = Some(id);
                }
                if !ctx.mm.write_u32(arg + 40, handle) || !ctx.mm.write_u32(arg + 44, id) {
                    state.resources.remove(&handle);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc02c_6446 | 0xc02c_6447 => {
                let handle = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let mut region = [0u32; 6];
                for (i, value) in region.iter_mut().enumerate() {
                    *value = ctx.mm.read_u32(arg + 4 + i as u64 * 4).ok_or(EFAULT)?;
                }
                let args = TransferArgs {
                    region,
                    level: ctx.mm.read_u32(arg + 28).ok_or(EFAULT)?,
                    offset: ctx.mm.read_u32(arg + 32).ok_or(EFAULT)?,
                    stride: ctx.mm.read_u32(arg + 36).ok_or(EFAULT)?,
                    layer_stride: ctx.mm.read_u32(arg + 40).ok_or(EFAULT)?,
                };
                let state = self.state.lock();
                let (_, context) = state.context.as_ref().ok_or(EINVAL)?;
                let (resource, _, _) = state.resources.get(&handle).ok_or(ENOENT)?;
                resource.transfer(*context, cmd == 0xc02c_6447, args)?;
                Ok(0)
            }
            // CONTEXT_INIT (the Linux ABI permits setting parameters only once).
            0xc010_644b => {
                let count = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let ptr = ctx.mm.read_u64(arg + 8).ok_or(EFAULT)?;
                if count > 64 {
                    return Err(EINVAL);
                }
                let mut capset = 0u32;
                for i in 0..count as u64 {
                    let key = ctx.mm.read_u64(ptr + i * 16).ok_or(EFAULT)?;
                    let value = ctx.mm.read_u64(ptr + i * 16 + 8).ok_or(EFAULT)?;
                    match key {
                        1 if value <= u32::MAX as u64 => capset = value as u32,
                        2 if value > 0 && value <= 64 => (),
                        3 => (),
                        _ => return Err(EINVAL),
                    }
                }
                let mut state = self.state.lock();
                if state.context.is_some() {
                    return Err(EBUSY);
                }
                let id = new_context_id(ctx.process.pid);
                backend.context_create(id, capset)?;
                state.context = Some((backend, id));
                Ok(0)
            }
            // CREATE_BLOB: command payloads are submitted before allocation.
            0xc030_644a => {
                let mem = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let flags = ctx.mm.read_u32(arg + 4).ok_or(EFAULT)?;
                let size = ctx.mm.read_u64(arg + 16).ok_or(EFAULT)?;
                let cmd_size = ctx.mm.read_u32(arg + 28).ok_or(EFAULT)? as usize;
                let cmd_ptr = ctx.mm.read_u64(arg + 32).ok_or(EFAULT)?;
                let blob_id = ctx.mm.read_u64(arg + 40).ok_or(EFAULT)?;
                if !(1..=3).contains(&mem)
                    || flags & !7 != 0
                    || size == 0
                    || size > (1 << 32)
                    || cmd_size > (1 << 20)
                {
                    return Err(EINVAL);
                }
                let mut state = self.state.lock();
                let (backend, context) = state.ensure_context(&backend, ctx.process.pid)?;
                if cmd_size != 0 {
                    let mut cmd = vec![0; cmd_size];
                    if !ctx.mm.read(cmd_ptr, &mut cmd) {
                        return Err(EFAULT);
                    }
                    backend.submit(context, &mut cmd)?;
                }
                let legacy_ring = state
                    .legacy_asg
                    .as_ref()
                    .filter(|asg| {
                        blob_id == asg.hostmem_id
                            && size == u64::from(asg.ring_size) + u64::from(asg.buffer_size)
                    })
                    .map(|asg| asg.ring.clone());
                let (resource, id) = if let Some(ring) = legacy_ring {
                    let id = ring.id();
                    (ring, id)
                } else {
                    let id = NEXT_RESOURCE.fetch_add(1, Ordering::Relaxed);
                    (
                        backend.create_blob(context, id, mem, flags, blob_id, size)?,
                        id,
                    )
                };
                let handle = state.next_handle;
                state.next_handle = handle.checked_add(1).ok_or(ENOMEM)?;
                state.resources.insert(handle, (resource, size, mem));
                if !ctx.mm.write_u32(arg + 8, handle) || !ctx.mm.write_u32(arg + 12, id) {
                    state.resources.remove(&handle);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc010_6441 => {
                let handle = ctx.mm.read_u32(arg + 8).ok_or(EFAULT)?;
                let state = self.state.lock();
                if !state.resources.contains_key(&handle) {
                    return Err(ENOENT);
                }
                if !ctx.mm.write_u64(arg, (handle as u64) << 32) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc010_6445 => {
                let handle = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let state = self.state.lock();
                let (resource, size, mem) = state.resources.get(&handle).ok_or(ENOENT)?;
                if !ctx.mm.write_u32(arg + 4, resource.id())
                    || !ctx
                        .mm
                        .write_u32(arg + 8, (*size).min(u32::MAX as u64) as u32)
                    || !ctx.mm.write_u32(arg + 12, *mem)
                {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0x4008_6409 => {
                let handle = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                self.state.lock().resources.remove(&handle).ok_or(ENOENT)?;
                Ok(0)
            }
            0xc040_6442 | 0xc028_6442 | 0xc020_6442 => {
                let flags = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let len = ctx.mm.read_u32(arg + 4).ok_or(EFAULT)? as usize;
                let ptr = ctx.mm.read_u64(arg + 8).ok_or(EFAULT)?;
                if flags & !7 != 0 || len > (1 << 20) {
                    eprintln!("virtgpu: execbuffer flags {flags:#x} len {len}");
                    return Err(EINVAL);
                }
                if flags & 1 != 0 {
                    let fd = ctx.mm.read_u32(arg + 28).ok_or(EFAULT)? as i32;
                    let table = ctx.fds();
                    let file = table.lock().get(fd)?;
                    let sync = file
                        .node
                        .as_any()
                        .downcast_ref::<SyncFile>()
                        .ok_or(EINVAL)?;
                    sync.fence.wait(false)?;
                }
                let ring = if flags & 4 != 0 {
                    if cmd == 0xc020_6442 {
                        return Err(EINVAL);
                    }
                    let idx = ctx.mm.read_u32(arg + 32).ok_or(EFAULT)?;
                    if idx > 255 {
                        return Err(EINVAL);
                    }
                    Some(idx as u8)
                } else {
                    None
                };
                let mut bytes = vec![0; len];
                if !ctx.mm.read(ptr, &mut bytes) {
                    return Err(EFAULT);
                }
                // Like Linux, submitting without CONTEXT_INIT uses an implicit
                // default context (capset 0; goldfish "virtio-gpu-pipe" users).
                let (backend, context) = {
                    let mut state = self.state.lock();
                    if state.context.is_none() {
                        let id = new_context_id(ctx.process.pid);
                        backend.context_create(id, 0).inspect_err(|e| {
                            eprintln!("virtgpu: implicit context_create failed {e}")
                        })?;
                        state.context = Some((backend.clone(), id));
                    }
                    state.context.clone().unwrap()
                };
                let opcode = bytes
                    .get(..4)
                    .map(|v| u32::from_le_bytes(v.try_into().unwrap()));
                let mut handled = false;
                if opcode == Some(0x1001)
                    && bytes.len() == 8
                    && u32::from_le_bytes(bytes[4..8].try_into().unwrap()) == 10
                {
                    let mut state = self.state.lock();
                    if state.legacy_response.is_some() && state.legacy_asg.is_none() {
                        let caps = backend.capset(3, 0)?;
                        if caps.len() < 12 {
                            return Err(EINVAL);
                        }
                        let ring_size = u32::from_le_bytes(caps[4..8].try_into().unwrap());
                        let buffer_size = u32::from_le_bytes(caps[8..12].try_into().unwrap());
                        let size = u64::from(ring_size) + u64::from(buffer_size);
                        if size == 0 || size > 64 * 1024 * 1024 {
                            return Err(EINVAL);
                        }
                        let id = NEXT_RESOURCE.fetch_add(1, Ordering::Relaxed);
                        let ring_resource = backend.create_blob(context, id, 2, 1, 0, size)?;
                        let mut create = [0u8; 8];
                        create[..4].copy_from_slice(&0x1001u32.to_le_bytes());
                        create[4..].copy_from_slice(&id.to_le_bytes());
                        backend.submit(context, &mut create)?;
                        state.legacy_asg = Some(LegacyAsg {
                            ring: ring_resource,
                            ring_size,
                            buffer_size,
                            hostmem_id: (1u64 << 63) | id as u64,
                        });
                        handled = true;
                    }
                } else if opcode == Some(0x1003) && bytes.len() >= 48 {
                    let state = self.state.lock();
                    if let Some(asg) = &state.legacy_asg {
                        let response_id = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
                        if Some(response_id) != state.legacy_response {
                            return Err(EINVAL);
                        }
                        let op = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
                        let mut response = bytes[8..48].to_vec();
                        let (size, metadata) = match op {
                            0 => (asg.ring_size as u64, 0),
                            1 => (asg.buffer_size as u64, 0),
                            2 => (1, asg.hostmem_id),
                            4 => (u64::from_le_bytes(response[8..16].try_into().unwrap()), 0),
                            _ => return Err(EINVAL),
                        };
                        response[8..16].copy_from_slice(&size.to_le_bytes());
                        response[16..24].copy_from_slice(&metadata.to_le_bytes());
                        let resource = state
                            .resources
                            .values()
                            .find(|(r, _, _)| r.id() == response_id)
                            .ok_or(EINVAL)?
                            .0
                            .clone();
                        let page = resource.map()?.page(0).ok_or(EINVAL)?;
                        page.bytes_mut()[..response.len()].copy_from_slice(&response);
                        handled = true;
                    }
                } else if opcode == Some(0x1002) && bytes.len() > 8 {
                    let state = self.state.lock();
                    if let Some(asg) = &state.legacy_asg {
                        let mut ping = [0u8; 8];
                        ping[..4].copy_from_slice(&0x1002u32.to_le_bytes());
                        ping[4..].copy_from_slice(&asg.ring.id().to_le_bytes());
                        backend.submit(context, &mut ping)?;
                        handled = true;
                    }
                }
                if !handled {
                    backend.submit(context, &mut bytes).inspect_err(|e| {
                        eprintln!(
                            "virtgpu: submit({} bytes, flags {flags:#x}) failed {e}",
                            bytes.len()
                        )
                    })?;
                }
                let fence = Arc::new(GpuFence::default());
                backend.fence(context, ring, fence.clone())?;
                self.state.lock().last_fence = Some(fence.clone());
                if flags & 2 != 0 {
                    let node = Arc::new(SyncFile {
                        id: vfs::new_id(),
                        fence,
                    });
                    let file =
                        crate::fd::OpenFile::new(node, crate::fd::O_RDWR, "anon_inode:[sync_file]");
                    let table = ctx.fds();
                    let fd = table.lock().insert(file, true)?;
                    if !ctx.mm.write_u32(arg + 28, fd as u32) {
                        let _ = table.lock().close(fd);
                        return Err(EFAULT);
                    }
                }
                Ok(0)
            }
            0xc008_6448 => {
                let handle = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let flags = ctx.mm.read_u32(arg + 4).ok_or(EFAULT)?;
                if flags & !1 != 0 {
                    return Err(EINVAL);
                }
                let state = self.state.lock();
                if !state.resources.contains_key(&handle) {
                    return Err(ENOENT);
                }
                let fence = state.last_fence.clone();
                drop(state);
                if let Some(fence) = fence {
                    fence.wait(flags & 1 != 0)?;
                }
                Ok(0)
            }
            0xc018_6449 => {
                let id = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                let version = ctx.mm.read_u32(arg + 4).ok_or(EFAULT)?;
                let ptr = ctx.mm.read_u64(arg + 8).ok_or(EFAULT)?;
                let size = ctx.mm.read_u32(arg + 16).ok_or(EFAULT)? as usize;
                let caps = backend.capset(id, version)?;
                if !ctx.mm.write(ptr, &caps[..size.min(caps.len())]) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            _ => Err(ENOTTY),
        }
    }
    fn shared_mem_at(&self, offset: u64, len: u64) -> Option<(Arc<SharedMem>, u64)> {
        let handle = (offset >> 32) as u32;
        let inner = offset & 0xffff_ffff;
        let state = self.state.lock();
        let (resource, size, _) = state.resources.get(&handle)?;
        if inner.checked_add(len)? > *size {
            return None;
        }
        Some((resource.clone().map().ok()?, inner))
    }
    fn mmap_requires_shared_mem(&self) -> bool {
        true
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Drop for VirtGpu {
    fn drop(&mut self) {
        let state = self.state.get_mut();
        state.resources.clear();
        if let Some((backend, id)) = state.context.take() {
            backend.context_destroy(id);
        }
    }
}

// Goldfish mapper uses this legacy allocator for CPU-side gralloc storage even
// when the host has not advertised GL direct-memory/DMA support. Offsets are
// shared between opens so a buffer fd imported by another process sees the
// same guest pages; this does not advertise host DMA access to those pages.
static ADDRESS_BLOCKS: std::sync::LazyLock<
    Mutex<std::collections::BTreeMap<u64, (u64, Arc<SharedMem>)>>,
> = std::sync::LazyLock::new(|| Mutex::new(std::collections::BTreeMap::new()));
const ADDRESS_PHYS_BASE: u64 = 0x4000_0000_0000;

/// Host address of a goldfish_address_space block's "physical" address, for
/// the renderer's DMA reads; null if no live block contains it.
pub fn dma_host_addr(paddr: u64) -> *mut u8 {
    let Some(offset) = paddr.checked_sub(ADDRESS_PHYS_BASE) else {
        return std::ptr::null_mut();
    };
    let blocks = ADDRESS_BLOCKS.lock();
    let Some((&base, (size, mem))) = blocks.range(..=offset).next_back() else {
        return std::ptr::null_mut();
    };
    let rel = offset - base;
    if rel >= *size {
        return std::ptr::null_mut();
    }
    // Blocks are contiguous HostBlock allocations, so page 0 is the base.
    match mem.page(0) {
        Some(p) => unsafe { p.ptr().add(rel as usize) },
        None => std::ptr::null_mut(),
    }
}

static NEXT_ADDRESS_OFFSET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(4096);

/// Contiguous, page-aligned, zeroed host allocation backing an address-space
/// block.
struct HostBlock {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}
unsafe impl Send for HostBlock {}
unsafe impl Sync for HostBlock {}
impl HostBlock {
    fn new(len: usize) -> Option<HostBlock> {
        let layout = std::alloc::Layout::from_size_align(len, 4096).ok()?;
        // SAFETY: len is non-zero (checked by the caller).
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        (!ptr.is_null()).then_some(HostBlock { ptr, layout })
    }
}
impl Drop for HostBlock {
    fn drop(&mut self) {
        // SAFETY: allocated in new() with this layout.
        unsafe { std::alloc::dealloc(self.ptr, self.layout) }
    }
}

pub struct GoldfishAddressSpace {
    id: u64,
    subdevice: Mutex<Option<u64>>,
    owned: Mutex<Vec<u64>>,
}
impl GoldfishAddressSpace {
    pub fn open() -> vfs::FileRef {
        Arc::new(Self {
            id: vfs::new_id(),
            subdevice: Mutex::new(None),
            owned: Mutex::new(Vec::new()),
        })
    }
}
impl Node for GoldfishAddressSpace {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            ino: self.id,
            mode: vfs::S_IFCHR | 0o666,
            nlink: 1,
            ..Default::default()
        }
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> vfs::Result<i64> {
        match cmd {
            0xc018_470a => {
                if *self.subdevice.lock() != Some(5) {
                    return Err(EINVAL);
                }
                let size = ctx.mm.read_u64(arg).ok_or(EFAULT)?;
                if size == 0 || size > (1 << 30) {
                    return Err(EINVAL);
                }
                let len = size.checked_add(4095).ok_or(EINVAL)? & !4095;
                let offset = NEXT_ADDRESS_OFFSET.fetch_add(len, Ordering::Relaxed);
                // The renderer reads these blocks by "physical" address
                // (rcUpdateColorBufferDMA, see dma_host_addr), so back them
                // with contiguous host memory.
                let block = Arc::new(HostBlock::new(len as usize).ok_or(ENOMEM)?);
                // SAFETY: HostBlock owns ptr..ptr+len (page-aligned, zeroed)
                // until the last page clone drops the owner.
                let mem = unsafe { SharedMem::from_external(block.ptr, len, block.clone()) }
                    .ok_or(EINVAL)?;
                ADDRESS_BLOCKS.lock().insert(offset, (len, mem));
                self.owned.lock().push(offset);
                if !ctx.mm.write_u64(arg, len)
                    || !ctx.mm.write_u64(arg + 8, offset)
                    || !ctx.mm.write_u64(arg + 16, ADDRESS_PHYS_BASE + offset)
                {
                    ADDRESS_BLOCKS.lock().remove(&offset);
                    self.owned.lock().retain(|x| *x != offset);
                    return Err(EFAULT);
                }
                Ok(0)
            }
            0xc008_470b => {
                let offset = ctx.mm.read_u64(arg).ok_or(EFAULT)?;
                let mut owned = self.owned.lock();
                if !owned.contains(&offset) {
                    return Err(EINVAL);
                }
                owned.retain(|x| *x != offset);
                ADDRESS_BLOCKS.lock().remove(&offset).ok_or(EINVAL)?;
                Ok(0)
            }
            0xc028_470c => {
                let mut subdevice = self.subdevice.lock();
                let metadata = ctx.mm.read_u64(arg + 16).ok_or(EFAULT)?;
                if subdevice.is_none() {
                    if metadata != 5 {
                        return Err(ENODEV);
                    }
                    *subdevice = Some(metadata);
                } else if *subdevice == Some(5) {
                    let offset = ctx.mm.read_u64(arg).ok_or(EFAULT)?;
                    if !(metadata == 1 || metadata == 2)
                        || !ADDRESS_BLOCKS.lock().contains_key(&offset)
                    {
                        return Err(EINVAL);
                    }
                } else {
                    return Err(EINVAL);
                }
                if !ctx.mm.write_u64(arg + 16, 0) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            _ => Err(ENOTTY),
        }
    }
    fn shared_mem_at(&self, offset: u64, len: u64) -> Option<(Arc<SharedMem>, u64)> {
        let blocks = ADDRESS_BLOCKS.lock();
        let (&base, (size, mem)) = blocks.range(..=offset).next_back()?;
        let relative = offset.checked_sub(base)?;
        if relative.checked_add(len)? > *size {
            return None;
        }
        Some((mem.clone(), relative))
    }
    fn mmap_requires_shared_mem(&self) -> bool {
        true
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
impl Drop for GoldfishAddressSpace {
    fn drop(&mut self) {
        let blocks = self.owned.get_mut();
        let mut global = ADDRESS_BLOCKS.lock();
        for offset in blocks.drain(..) {
            global.remove(&offset);
        }
    }
}
