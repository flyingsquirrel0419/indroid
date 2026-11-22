//! Virtual file system: the guest sees one Linux tree assembled from
//!  - the read-only ROM directory (system-as-root image, see tools/fetch-rom.sh),
//!  - writable host directories for tmpfs/data mounts (/data, /dev, /mnt, ...),
//!  - synthetic nodes (/proc, device files) provided by other modules.
//!
//! Guest symlinks are stored as host symlinks whose targets are *guest* paths,
//! so path resolution is done here, component by component, never by the host.

use crate::errno::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirEntryExt, FileExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, i64>;

/// Linux `struct stat` fields we report (arm64 layout is written by syscalls).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub size: i64,
    pub blksize: i32,
    pub blocks: i64,
    pub atime: (i64, i64),
    pub mtime: (i64, i64),
    pub ctime: (i64, i64),
}

pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;

/// Readiness bits for poll/epoll (Linux values).
pub const POLLIN: u32 = 0x1;
pub const POLLPRI: u32 = 0x2;
pub const POLLOUT: u32 = 0x4;
pub const POLLERR: u32 = 0x8;
pub const POLLHUP: u32 = 0x10;
pub const POLLRDHUP: u32 = 0x2000;

pub struct DirEntry {
    pub name: Vec<u8>,
    pub ino: u64,
    /// DT_* type.
    pub kind: u8,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1 << 40);
pub fn new_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// An open-able object (the inode side; position lives in `OpenFile`).
pub trait Node: Send + Sync + 'static {
    /// Unique identity (page cache key, st_ino).
    fn id(&self) -> u64;
    fn stat(&self) -> Stat;
    /// Positioned read. Stream nodes ignore `off`.
    fn read_at(&self, _off: u64, _buf: &mut [u8], _nonblock: bool) -> Result<usize> {
        Err(EINVAL)
    }
    fn write_at(&self, _off: u64, _buf: &[u8], _nonblock: bool) -> Result<usize> {
        Err(EINVAL)
    }
    /// True for pipes/sockets/ttys (no seeking, reads ignore offsets).
    fn is_stream(&self) -> bool {
        false
    }
    fn truncate(&self, _len: u64) -> Result<()> {
        Err(EINVAL)
    }
    fn readdir(&self) -> Result<Vec<DirEntry>> {
        Err(ENOTDIR)
    }
    /// ioctl with guest memory access through `ctx`.
    fn ioctl(&self, _ctx: &crate::task::Ctx, _cmd: u32, _arg: u64) -> Result<i64> {
        Err(ENOTTY)
    }
    /// Current readiness (POLL* bits).
    fn poll(&self) -> u32 {
        POLLIN | POLLOUT
    }
    /// Registers a waiter woken when readiness may have changed.
    fn wait_queue(&self) -> Option<Arc<crate::sync::WaitQueue>> {
        None
    }
    /// Shared memory object for MAP_SHARED (ashmem, memfd, shared files).
    fn shared_mem(&self) -> Option<Arc<crate::mm::SharedMem>> {
        None
    }
    /// Resolve a file offset to a shared object and an offset within it.
    /// Devices with multiple independently mapped objects can override this.
    fn shared_mem_at(&self, offset: u64, _len: u64) -> Option<(Arc<crate::mm::SharedMem>, u64)> {
        self.shared_mem().map(|mem| (mem, offset))
    }
    /// Devices can reject offsets which do not resolve to shared memory.
    fn mmap_requires_shared_mem(&self) -> bool {
        false
    }
    /// Called after this node was mmapped at `addr` (MAP_SHARED of
    /// `shared_mem()`); binder uses it to learn its buffer address.
    fn mmapped(&self, _ctx: &crate::task::Ctx, _addr: u64, _len: u64, _offset: u64) -> Result<()> {
        Ok(())
    }
    /// Fills one page for private file mappings (zero-filled past EOF).
    fn read_page(&self, off: u64, page: &mut [u8; 4096]) {
        let mut done = 0;
        while done < 4096 {
            match self.read_at(off + done as u64, &mut page[done..], false) {
                Ok(0) | Err(_) => break,
                Ok(n) => done += n,
            }
        }
        page[done..].fill(0);
    }
    /// Host path for host-backed nodes (used by /proc/self/fd and fd paths).
    fn host_path(&self) -> Option<&Path> {
        None
    }
    /// Called when the last OpenFile referencing this node is closed.
    fn release(&self) {}
    fn as_any(&self) -> &dyn std::any::Any;
}

pub type FileRef = Arc<dyn Node>;

// ---------------------------------------------------------------------------
// Host-backed regular files and directories.

pub struct HostFile {
    pub file: fs::File,
    pub path: PathBuf,
    id: u64,
    writable: bool,
}

impl HostFile {
    pub fn open(
        path: &Path,
        write: bool,
        create: bool,
        trunc: bool,
        mode: u32,
    ) -> Result<HostFile> {
        let mut o = fs::OpenOptions::new();
        o.read(true);
        if write {
            o.write(true);
        }
        if create {
            o.create(true);
        }
        if trunc {
            o.truncate(true);
        }
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(mode & 0o7777 | 0o600);
        let file = match o.open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !write => {
                // ROM files may lack the host read bit for our user.
                return Err(host_err(&e));
            }
            Err(e) => return Err(host_err(&e)),
        };
        let md = file.metadata().map_err(|e| host_err(&e))?;
        let id = md.dev().wrapping_mul(0x9e3779b97f4a7c15) ^ md.ino();
        Ok(HostFile {
            file,
            path: path.to_owned(),
            id,
            writable: write,
        })
    }
}

impl Node for HostFile {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        self.file
            .metadata()
            .map(|m| stat_from_host(&m))
            .unwrap_or_default()
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        self.file.read_at(buf, off).map_err(|e| host_err(&e))
    }
    fn write_at(&self, off: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        if !self.writable {
            return Err(EBADF);
        }
        self.file.write_at(buf, off).map_err(|e| host_err(&e))
    }
    fn truncate(&self, len: u64) -> Result<()> {
        self.file.set_len(len).map_err(|e| host_err(&e))
    }
    fn host_path(&self) -> Option<&Path> {
        Some(&self.path)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct HostDir {
    pub path: PathBuf,
    id: u64,
    /// Extra synthetic entries (device nodes etc.) merged into listings.
    extra: Vec<DirEntry>,
}

impl HostDir {
    pub fn new(path: &Path, extra: Vec<DirEntry>) -> HostDir {
        let id = fs::metadata(path)
            .map(|m| m.dev().wrapping_mul(0x9e3779b97f4a7c15) ^ m.ino())
            .unwrap_or_else(|_| new_id());
        HostDir {
            path: path.to_owned(),
            id,
            extra,
        }
    }
}

impl Node for HostDir {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        fs::metadata(&self.path)
            .map(|m| stat_from_host(&m))
            .unwrap_or_default()
    }
    fn readdir(&self) -> Result<Vec<DirEntry>> {
        let mut out = vec![
            DirEntry {
                name: b".".to_vec(),
                ino: self.id,
                kind: 4,
            },
            DirEntry {
                name: b"..".to_vec(),
                ino: 1,
                kind: 4,
            },
        ];
        for e in fs::read_dir(&self.path).map_err(|e| host_err(&e))? {
            let Ok(e) = e else { continue };
            let Ok(ft) = e.file_type() else { continue };
            use std::os::unix::ffi::OsStrExt;
            let name = e.file_name().as_bytes().to_vec();
            if name.starts_with(b".indroid") {
                continue;
            }
            let kind = if ft.is_dir() {
                4
            } else if ft.is_symlink() {
                10
            } else {
                8
            };
            out.push(DirEntry {
                name,
                ino: e.ino(),
                kind,
            });
        }
        for x in &self.extra {
            if !out.iter().any(|e| e.name == x.name) {
                out.push(DirEntry {
                    name: x.name.clone(),
                    ino: x.ino,
                    kind: x.kind,
                });
            }
        }
        Ok(out)
    }
    fn host_path(&self) -> Option<&Path> {
        Some(&self.path)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn stat_from_host(m: &fs::Metadata) -> Stat {
    // Host umask leaves group-write on our files; Android treats g/o-writable
    // config files as insecure, and we do not enforce permissions anyway.
    let mut mode = m.mode() & !0o022;
    // ROM files are owned by "root" in the guest; keep permission bits but
    // make sure the guest (uid 0) can read what we can read.
    if m.is_dir() {
        mode |= 0o555;
    }
    Stat {
        dev: 0xfd00,
        ino: m.dev().wrapping_mul(0x9e3779b97f4a7c15) ^ m.ino(),
        mode,
        nlink: m.nlink() as u32,
        uid: 0,
        gid: 0,
        rdev: 0,
        size: m.size() as i64,
        blksize: 4096,
        blocks: m.blocks() as i64,
        atime: (m.atime(), m.atime_nsec()),
        mtime: (m.mtime(), m.mtime_nsec()),
        ctime: (m.ctime(), m.ctime_nsec()),
    }
}

pub fn host_err(e: &std::io::Error) -> i64 {
    match e.raw_os_error() {
        // Host errno values are Linux values on Linux; map the common ones
        // explicitly so this also works on Darwin hosts.
        Some(_) | None => match e.kind() {
            std::io::ErrorKind::NotFound => ENOENT,
            std::io::ErrorKind::PermissionDenied => EACCES,
            std::io::ErrorKind::AlreadyExists => EEXIST,
            std::io::ErrorKind::InvalidInput => EINVAL,
            std::io::ErrorKind::NotADirectory => ENOTDIR,
            std::io::ErrorKind::IsADirectory => EISDIR,
            std::io::ErrorKind::DirectoryNotEmpty => ENOTEMPTY,
            std::io::ErrorKind::ReadOnlyFilesystem => EROFS,
            std::io::ErrorKind::CrossesDevices => EXDEV,
            _ => match e.raw_os_error() {
                Some(libc::ELOOP) => ELOOP,
                Some(libc::ENAMETOOLONG) => ENAMETOOLONG,
                Some(libc::ENOSPC) => ENOSPC,
                _ => EIO,
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Mounts and path resolution.

#[derive(Clone)]
pub enum MountKind {
    /// Host directory; `rw` allows modifications.
    Host { dir: PathBuf, rw: bool },
    /// Synthetic file system implemented by a provider.
    Synthetic(Arc<dyn SyntheticFs>),
}

/// A synthetic file system (procfs, sysfs parts...). Paths are relative to the
/// mount point, without leading slash ("" = the mount root).
pub trait SyntheticFs: Send + Sync {
    fn lookup(&self, ctx: &crate::task::Ctx, rel: &str) -> Result<SynthEntry>;
}

pub enum SynthEntry {
    Node(FileRef),
    Symlink(String),
}

/// A device or socket node placed at a fixed guest path (e.g. /dev/null,
/// /dev/binder). Shadows whatever the backing host directory has.
pub type DeviceFactory = Arc<dyn Fn() -> FileRef + Send + Sync>;

pub struct Vfs {
    mounts: Vec<(String, MountKind)>,
    /// Directories layered over read-only mounts (first match wins): our
    /// patches to the ROM (rc files, stub binaries) without modifying it.
    pub overlays: Vec<PathBuf>,
    pub devices: Mutex<HashMap<String, (DeviceFactory, u32)>>,
    /// Bound AF_UNIX socket paths -> listener objects.
    pub unix_sockets: Mutex<HashMap<String, FileRef>>,
}

/// Result of resolving a guest path.
#[derive(Clone)]
pub enum Loc {
    Host {
        host: PathBuf,
        rw: bool,
    },
    Device(String),
    Synth(FileRef),
    /// The path does not exist (yet); parent exists and is a host dir.
    Missing {
        host: PathBuf,
        rw: bool,
    },
}

impl Vfs {
    pub fn new(rom: &Path, state: &Path) -> Vfs {
        // tmpfs-like mounts start empty on every boot; /data, /cache and
        // /metadata persist.
        let persistent = ["/data", "/cache", "/metadata"];
        let rw = |p: &str| -> MountKind {
            let d = state.join(p.trim_start_matches('/'));
            if !persistent.contains(&p) {
                let _ = fs::remove_dir_all(&d);
            }
            let _ = fs::create_dir_all(&d);
            MountKind::Host { dir: d, rw: true }
        };
        let mounts = vec![
            (
                "/".to_string(),
                MountKind::Host {
                    dir: rom.to_owned(),
                    rw: false,
                },
            ),
            ("/data".into(), rw("/data")),
            ("/dev".into(), rw("/dev")),
            ("/mnt".into(), rw("/mnt")),
            ("/tmp".into(), rw("/tmp")),
            ("/cache".into(), rw("/cache")),
            ("/metadata".into(), rw("/metadata")),
            ("/storage".into(), rw("/storage")),
            ("/linkerconfig".into(), rw("/linkerconfig")),
            ("/data_mirror".into(), rw("/data_mirror")),
            ("/acct".into(), rw("/acct")),
            ("/config".into(), rw("/config")),
            ("/sys".into(), rw("/sys")),
            ("/debug_ramdisk".into(), rw("/debug_ramdisk")),
            (
                "/second_stage_resources".into(),
                rw("/second_stage_resources"),
            ),
        ];
        // init (Android 13+) bind-mounts /data/data onto /data/user/0; the
        // mount table is fixed, so set the bind up front. The empty host
        // directory keeps "0" visible when listing /data/user.
        let mut mounts = mounts;
        let data = state.join("data");
        let _ = fs::create_dir_all(data.join("data"));
        let _ = fs::create_dir_all(data.join("user/0"));
        mounts.push((
            "/data/user/0".into(),
            MountKind::Host {
                dir: data.join("data"),
                rw: true,
            },
        ));
        Vfs {
            mounts,
            overlays: Vec::new(),
            devices: Mutex::new(HashMap::new()),
            unix_sockets: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_mount(&mut self, at: &str, kind: MountKind) {
        self.mounts.retain(|(m, _)| m != at);
        self.mounts.push((at.to_string(), kind));
    }

    pub fn add_device(&self, path: &str, mode: u32, f: DeviceFactory) {
        self.devices.lock().insert(path.to_string(), (f, mode));
    }

    /// Longest-prefix mount for a normalized absolute guest path.
    fn mount_for<'a>(&'a self, path: &str) -> (&'a str, &'a MountKind) {
        let mut best: Option<(&str, &MountKind)> = None;
        for (m, k) in &self.mounts {
            let hit = m == "/"
                || path == m
                || (path.starts_with(m.as_str()) && path.as_bytes().get(m.len()) == Some(&b'/'));
            if hit && best.is_none_or(|(b, _)| m.len() > b.len()) {
                best = Some((m, k));
            }
        }
        best.expect("root mount")
    }

    /// Maps a normalized guest path (no symlink resolution) to a location.
    fn locate(&self, ctx: &crate::task::Ctx, path: &str) -> Result<(Loc, Option<String>)> {
        if self.devices.lock().contains_key(path) {
            return Ok((Loc::Device(path.to_string()), None));
        }
        let (m, kind) = self.mount_for(path);
        let rel = path[m.len()..].trim_start_matches('/');
        match kind {
            MountKind::Host { dir, rw } => {
                let mut host = if rel.is_empty() {
                    dir.clone()
                } else {
                    dir.join(rel)
                };
                if !*rw && path != "/" {
                    let r = path.trim_start_matches('/');
                    for o in &self.overlays {
                        let cand = o.join(r);
                        if let Ok(md) = fs::symlink_metadata(&cand) {
                            // Overlays replace files; a directory the ROM also
                            // has stays the ROM's and extra_entries merges the
                            // overlay's files into its listing.
                            if md.is_dir() && host.is_dir() {
                                break;
                            }
                            host = cand;
                            break;
                        }
                    }
                }
                match fs::symlink_metadata(&host) {
                    Ok(md) if md.file_type().is_symlink() => {
                        let t = fs::read_link(&host).map_err(|e| host_err(&e))?;
                        Ok((
                            Loc::Host { host, rw: *rw },
                            Some(t.to_string_lossy().into_owned()),
                        ))
                    }
                    Ok(_) => Ok((Loc::Host { host, rw: *rw }, None)),
                    Err(_) => Ok((Loc::Missing { host, rw: *rw }, None)),
                }
            }
            MountKind::Synthetic(fs) => match fs.lookup(ctx, rel)? {
                SynthEntry::Node(n) => Ok((Loc::Synth(n), None)),
                SynthEntry::Symlink(t) => {
                    Ok((Loc::Synth(Arc::new(SymlinkNode(t.clone()))), Some(t)))
                }
            },
        }
    }

    /// Resolves `path` relative to `cwd`, following symlinks (the last one
    /// only if `follow`). Returns the canonical guest path and its location.
    pub fn resolve(
        &self,
        ctx: &crate::task::Ctx,
        cwd: &str,
        path: &str,
        follow: bool,
    ) -> Result<(String, Loc)> {
        if path.is_empty() {
            return Err(ENOENT);
        }
        let mut pending: Vec<String> = Vec::new();
        let start = if path.starts_with('/') {
            String::new()
        } else {
            cwd.trim_end_matches('/').to_string()
        };
        push_components(&mut pending, path);
        let mut cur = start; // "" = root
        let mut links = 0;
        let mut loc = Loc::Host {
            host: PathBuf::new(),
            rw: false,
        };
        let mut first = true;
        while let Some(comp) = pending.pop() {
            let last = pending.is_empty();
            match comp.as_str() {
                "." => {
                    if last {
                        let p = if cur.is_empty() {
                            "/".to_string()
                        } else {
                            cur.clone()
                        };
                        loc = self.locate(ctx, &p)?.0;
                    }
                    continue;
                }
                ".." => {
                    if let Some(i) = cur.rfind('/') {
                        cur.truncate(i);
                    }
                    if last {
                        let p = if cur.is_empty() {
                            "/".to_string()
                        } else {
                            cur.clone()
                        };
                        loc = self.locate(ctx, &p)?.0;
                    }
                    continue;
                }
                _ => {}
            }
            let candidate = format!("{cur}/{comp}");
            let (l, link) = self.locate(ctx, &candidate)?;
            if let Some(target) = link {
                if !last || follow {
                    links += 1;
                    if links > 40 {
                        return Err(ELOOP);
                    }
                    if target.starts_with('/') {
                        cur.clear();
                    }
                    push_components(&mut pending, &target);
                    if pending.is_empty() {
                        loc = self.locate(ctx, if cur.is_empty() { "/" } else { &cur })?.0;
                    }
                    continue;
                }
            }
            if !last {
                if let Loc::Missing { .. } = l {
                    return Err(ENOENT);
                }
            }
            cur = candidate;
            loc = l;
            first = false;
        }
        let _ = first;
        if cur.is_empty() {
            cur = "/".into();
            loc = self.locate(ctx, "/")?.0;
        }
        Ok((cur, loc))
    }

    pub fn device(&self, path: &str) -> Option<(FileRef, u32)> {
        self.devices.lock().get(path).map(|(f, m)| (f(), *m))
    }

    /// Synthetic directory entries (devices) directly under `dir`.
    pub fn extra_entries(&self, dir: &str) -> Vec<DirEntry> {
        let prefix = if dir == "/" {
            "/".to_string()
        } else {
            format!("{dir}/")
        };
        let mut out = Vec::new();
        for o in &self.overlays {
            if let Ok(rd) = fs::read_dir(o.join(dir.trim_start_matches('/'))) {
                for e in rd.flatten() {
                    use std::os::unix::ffi::OsStrExt;
                    let kind = match e.file_type() {
                        Ok(t) if t.is_dir() => 4,
                        Ok(t) if t.is_symlink() => 10,
                        _ => 8,
                    };
                    out.push(DirEntry {
                        name: e.file_name().as_bytes().to_vec(),
                        ino: e.ino(),
                        kind,
                    });
                }
            }
        }
        for (p, (_, mode)) in self.devices.lock().iter() {
            if let Some(rest) = p.strip_prefix(&prefix) {
                let name = rest.split('/').next().unwrap();
                let kind = if rest.contains('/') {
                    4
                } else if mode & S_IFMT == S_IFCHR {
                    2
                } else {
                    8
                };
                if !out.iter().any(|e: &DirEntry| e.name == name.as_bytes()) {
                    out.push(DirEntry {
                        name: name.as_bytes().to_vec(),
                        ino: new_id(),
                        kind,
                    });
                }
            }
        }
        out
    }
}

fn push_components(stack: &mut Vec<String>, path: &str) {
    // stack is popped from the end: push in reverse order.
    for c in path.split('/').filter(|c| !c.is_empty()).rev() {
        stack.push(c.to_string());
    }
}

/// A symlink in a synthetic file system (lstat/readlink target).
pub struct SymlinkNode(pub String);

impl Node for SymlinkNode {
    fn id(&self) -> u64 {
        0
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFLNK | 0o777,
            size: self.0.len() as i64,
            nlink: 1,
            ..Default::default()
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Makes a host file readable by us if the ROM extraction dropped the bit.
pub fn ensure_readable(p: &Path) {
    if let Ok(m) = fs::metadata(p) {
        let mode = m.permissions().mode();
        if mode & 0o400 == 0 {
            let _ = fs::set_permissions(p, fs::Permissions::from_mode(mode | 0o400));
        }
    }
}
