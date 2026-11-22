//! Linux arm64 syscall dispatch.

use crate::errno::*;
use crate::fd::*;
use crate::mm::*;
use crate::task::*;
use crate::vfs::*;
use indroid_a64::Cpu;
use parking_lot::Mutex;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum Flow {
    Continue,
    /// The process/address space changed (execve): re-create the Ctx.
    Reload,
    /// This thread is done.
    Exit,
}

const AT_FDCWD: i64 = -100;
const O_RDONLY_FLAG: u32 = 0;
const AT_SYMLINK_NOFOLLOW: u32 = 0x100;
const AT_REMOVEDIR: u32 = 0x200;
const AT_EMPTY_PATH: u32 = 0x1000;

/// `INDROID_STRACE=1` traces everything; any other value traces only
/// processes whose comm contains it (e.g. `INDROID_STRACE=systemui`).
pub(crate) fn strace_match(ctx: &Ctx) -> bool {
    static FILTER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    match FILTER.get_or_init(|| {
        std::env::var("INDROID_STRACE")
            .ok()
            .filter(|v| !v.is_empty() && v != "1")
    }) {
        None => true,
        Some(f) => {
            ctx.process.comm.lock().contains(f.as_str())
                || ctx.process.exe.lock().contains(f.as_str())
        }
    }
}

pub fn dispatch(ctx: &Ctx, cpu: &mut Cpu) -> Flow {
    let nr = cpu.x[8];
    let a = [cpu.x[0], cpu.x[1], cpu.x[2], cpu.x[3], cpu.x[4], cpu.x[5]];
    let trace = ctx.kernel.trace_syscalls.load(Ordering::Relaxed) && strace_match(ctx);
    // Syscalls that change control flow.
    match nr {
        93 => {
            // exit (this thread only)
            let p = &ctx.process;
            if p.threads.lock().len() == 1 {
                *p.exit.lock() = Some(ExitStatus::Exited(a[0] as i32 & 0xff));
            }
            return Flow::Exit;
        }
        94 => {
            ctx.kernel
                .exit_group(&ctx.process, ExitStatus::Exited(a[0] as i32 & 0xff));
            return Flow::Exit;
        }
        139 => {
            if trace {
                ctx.kernel
                    .log(&format!("[{}] rt_sigreturn", ctx.thread.tid));
            }
            if !ctx.sigreturn(cpu) {
                ctx.kernel
                    .exit_group(&ctx.process, ExitStatus::Signaled(SIGSEGV, true));
                return Flow::Exit;
            }
            if trace {
                ctx.kernel.log(&format!(
                    "[{}] rt_sigreturn -> pc={} x30={:#x} sp={:#x}",
                    ctx.thread.tid,
                    crate::task::locate(&ctx.mm, cpu.pc),
                    cpu.x[30],
                    cpu.sp
                ));
            }
            return Flow::Continue;
        }
        221 | 281 => {
            let r = sys_execve(ctx, cpu, nr, a);
            if trace {
                ctx.kernel
                    .log(&format!("[{}] execve -> {r}", ctx.thread.tid));
            }
            if r == 0 {
                return Flow::Reload;
            }
            cpu.x[0] = r as u64;
            return Flow::Continue;
        }
        _ => {}
    }
    // Path argument, decoded before the call (for tracing).
    let path_arg = if trace {
        match nr {
            56 | 48 | 439 | 79 | 291 | 78 | 34 | 35 | 33 | 53 | 54 => ctx.mm.read_cstr(a[1], 4096),
            49 | 43 | 45 | 8 | 9 | 5 | 6 => ctx.mm.read_cstr(a[0], 4096),
            36 => ctx.mm.read_cstr(a[2], 4096),
            200 | 203 => {
                let mut b = [0u8; 110];
                ctx.mm.read(a[1], &mut b[..(a[2] as usize).min(110)]);
                Some(
                    b[2..]
                        .iter()
                        .map(|&c| if c == 0 { b'@' } else { c })
                        .take_while(|_| true)
                        .collect::<Vec<u8>>(),
                )
            }
            _ => None,
        }
    } else {
        None
    };
    let r = syscall(ctx, cpu, nr, a);
    if let Some(p) = &path_arg {
        let s = String::from_utf8_lossy(p);
        ctx.kernel.log(&format!(
            "[{}] {}(\"{}\") = {}",
            ctx.thread.tid,
            name(nr),
            s.trim_end_matches('@'),
            if (-4096..0).contains(&r) {
                format!("{r} ({})", errname(r))
            } else {
                format!("{r:#x}")
            }
        ));
    } else if trace {
        ctx.kernel.log(&format!(
            "[{}] {}({:#x}, {:#x}, {:#x}, {:#x}) = {}",
            ctx.thread.tid,
            name(nr),
            a[0],
            a[1],
            a[2],
            a[3],
            if (-4096..0).contains(&r) {
                format!("{r} ({})", errname(r))
            } else {
                format!("{r:#x}")
            }
        ));
    }
    if r == i64::MIN {
        // Clone created the child; register already set.
        return Flow::Continue;
    }
    cpu.x[0] = r as u64;
    if ctx.process.dying.load(Ordering::Relaxed) {
        return Flow::Exit;
    }
    Flow::Continue
}

fn errname(e: i64) -> &'static str {
    match e {
        ENOENT => "ENOENT",
        EBADF => "EBADF",
        EINVAL => "EINVAL",
        EACCES => "EACCES",
        EPERM => "EPERM",
        ENOSYS => "ENOSYS",
        EAGAIN => "EAGAIN",
        EFAULT => "EFAULT",
        ENOTTY => "ENOTTY",
        EEXIST => "EEXIST",
        ENOTDIR => "ENOTDIR",
        EINTR => "EINTR",
        ENOMEM => "ENOMEM",
        ETIMEDOUT => "ETIMEDOUT",
        EROFS => "EROFS",
        ECONNREFUSED => "ECONNREFUSED",
        _ => "",
    }
}

pub fn name(nr: u64) -> String {
    let s = match nr {
        17 => "getcwd",
        19 => "eventfd2",
        20 => "epoll_create1",
        21 => "epoll_ctl",
        22 => "epoll_pwait",
        23 => "dup",
        24 => "dup3",
        25 => "fcntl",
        29 => "ioctl",
        34 => "mkdirat",
        35 => "unlinkat",
        36 => "symlinkat",
        38 => "renameat",
        43 => "statfs",
        44 => "fstatfs",
        46 => "ftruncate",
        48 => "faccessat",
        49 => "chdir",
        53 => "fchmodat",
        54 => "fchownat",
        56 => "openat",
        57 => "close",
        59 => "pipe2",
        61 => "getdents64",
        62 => "lseek",
        63 => "read",
        64 => "write",
        65 => "readv",
        66 => "writev",
        67 => "pread64",
        68 => "pwrite64",
        73 => "ppoll",
        78 => "readlinkat",
        79 => "newfstatat",
        80 => "fstat",
        96 => "set_tid_address",
        98 => "futex",
        99 => "set_robust_list",
        101 => "nanosleep",
        113 => "clock_gettime",
        115 => "clock_nanosleep",
        124 => "sched_yield",
        129 => "kill",
        131 => "tgkill",
        132 => "sigaltstack",
        134 => "rt_sigaction",
        135 => "rt_sigprocmask",
        160 => "uname",
        167 => "prctl",
        172 => "getpid",
        178 => "gettid",
        198 => "socket",
        200 => "bind",
        203 => "connect",
        211 => "sendmsg",
        212 => "recvmsg",
        214 => "brk",
        215 => "munmap",
        216 => "mremap",
        220 => "clone",
        222 => "mmap",
        226 => "mprotect",
        233 => "madvise",
        260 => "wait4",
        261 => "prlimit64",
        278 => "getrandom",
        279 => "memfd_create",
        280 => "bpf",
        291 => "statx",
        _ => return format!("sys_{nr}"),
    };
    s.to_string()
}

fn cstr(ctx: &Ctx, addr: u64) -> Result<String> {
    let b = ctx.mm.read_cstr(addr, 4096).ok_or(EFAULT)?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}

/// Directory used for *at() calls.
fn dir_of(ctx: &Ctx, dirfd: i64, path: &str) -> Result<String> {
    if path.starts_with('/') || dirfd == AT_FDCWD || dirfd as i32 == AT_FDCWD as i32 {
        return Ok(ctx.process.cwd.lock().clone());
    }
    let f = ctx.fds().lock().get(dirfd as i32)?;
    Ok(f.path.clone())
}

fn resolve(ctx: &Ctx, dirfd: i64, path: &str, follow: bool) -> Result<(String, Loc)> {
    let cwd = dir_of(ctx, dirfd, path)?;
    ctx.kernel.vfs.resolve(ctx, &cwd, path, follow)
}

/// Opens a resolved location as a node.
fn open_loc(ctx: &Ctx, gpath: &str, loc: &Loc, flags: u32, mode: u32) -> Result<FileRef> {
    let write = flags & O_ACCMODE != 0;
    match loc {
        Loc::Device(p) => ctx.kernel.vfs.device(p).map(|(n, _)| n).ok_or(ENOENT),
        Loc::Synth(n) => Ok(n.clone()),
        Loc::Missing { host, rw } => {
            if (gpath.ends_with("/cgroup.procs") || gpath.ends_with("/tasks"))
                && (gpath.starts_with("/sys/fs/cgroup")
                    || gpath.starts_with("/dev/")
                    || gpath.starts_with("/acct"))
            {
                return Ok(crate::fd::NullDev::new(0));
            }
            if flags & O_CREAT == 0 {
                return Err(ENOENT);
            }
            if !rw {
                return Err(EROFS);
            }
            let umask = ctx.process.umask.load(Ordering::Relaxed);
            let f = Arc::new(HostFile::open(
                host,
                true,
                true,
                flags & O_TRUNC != 0,
                mode & !umask,
            )?);
            notify_path(gpath, 0x100);
            Ok(f)
        }
        Loc::Host { host, rw } => {
            if flags & O_CREAT != 0 && flags & O_EXCL != 0 {
                return Err(EEXIST);
            }
            // cgroup membership files: accept writes, list nothing (we have
            // no cgroups; libprocessgroup polls these to kill process groups).
            if gpath.starts_with("/sys/fs/cgroup")
                || gpath.starts_with("/dev/cg2_bpf")
                || gpath.starts_with("/acct")
                || gpath.starts_with("/dev/cpuset")
                || gpath.starts_with("/dev/stune")
                || gpath.starts_with("/dev/blkio")
                || gpath.starts_with("/dev/memcg")
            {
                if gpath.ends_with("/cgroup.procs") || gpath.ends_with("/tasks") {
                    return Ok(crate::fd::NullDev::new(0));
                }
            }
            let md = std::fs::metadata(host).map_err(|e| host_err(&e))?;
            if md.is_dir() {
                if write {
                    return Err(EISDIR);
                }
                return Ok(Arc::new(HostDir::new(
                    host,
                    ctx.kernel.vfs.extra_entries(gpath),
                )));
            }
            if flags & O_DIRECTORY != 0 {
                return Err(ENOTDIR);
            }
            if write && !rw {
                return Err(EROFS);
            }
            ensure_readable(host);
            Ok(Arc::new(HostFile::open(
                host,
                write,
                false,
                flags & O_TRUNC != 0 && write,
                0o644,
            )?))
        }
    }
}

pub fn open_path(
    ctx: &Ctx,
    dirfd: i64,
    path: &str,
    flags: u32,
    mode: u32,
) -> Result<(String, FileRef)> {
    let follow = flags & O_NOFOLLOW == 0;
    let (gpath, loc) = resolve(ctx, dirfd, path, follow)?;
    if !follow {
        if let Loc::Host { host, .. } = &loc {
            if std::fs::symlink_metadata(host)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(ELOOP);
            }
        }
    }
    let node = open_loc(ctx, &gpath, &loc, flags, mode)?;
    Ok((gpath, node))
}

fn stat_loc(ctx: &Ctx, gpath: &str, loc: &Loc) -> Result<Stat> {
    match loc {
        Loc::Host { host, .. } => {
            let md = std::fs::symlink_metadata(host).map_err(|e| host_err(&e))?;
            let mut st = stat_from_host(&md);
            if md.file_type().is_symlink() {
                st.mode = S_IFLNK | 0o777;
            }
            Ok(st)
        }
        Loc::Device(p) => {
            let (n, mode) = ctx.kernel.vfs.device(p).ok_or(ENOENT)?;
            let mut st = n.stat();
            if st.mode & S_IFMT == 0 {
                st.mode |= mode & S_IFMT;
            }
            Ok(st)
        }
        Loc::Synth(n) => Ok(n.stat()),
        Loc::Missing { .. } => {
            let _ = gpath;
            Err(ENOENT)
        }
    }
}

fn write_stat(ctx: &Ctx, addr: u64, st: &Stat) -> i64 {
    // struct stat (arm64 generic): 128 bytes.
    let mut b = [0u8; 128];
    let p64 = |b: &mut [u8], o: usize, v: u64| b[o..o + 8].copy_from_slice(&v.to_le_bytes());
    let p32 = |b: &mut [u8], o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    p64(&mut b, 0, st.dev);
    p64(&mut b, 8, st.ino);
    p32(&mut b, 16, st.mode);
    p32(&mut b, 20, st.nlink.max(1));
    p32(&mut b, 24, st.uid);
    p32(&mut b, 28, st.gid);
    p64(&mut b, 32, st.rdev);
    p64(&mut b, 48, st.size as u64);
    p32(&mut b, 56, st.blksize.max(512) as u32);
    p64(&mut b, 64, st.blocks as u64);
    p64(&mut b, 72, st.atime.0 as u64);
    p64(&mut b, 80, st.atime.1 as u64);
    p64(&mut b, 88, st.mtime.0 as u64);
    p64(&mut b, 96, st.mtime.1 as u64);
    p64(&mut b, 104, st.ctime.0 as u64);
    p64(&mut b, 112, st.ctime.1 as u64);
    if ctx.mm.write(addr, &b) {
        0
    } else {
        EFAULT
    }
}

fn write_statx(ctx: &Ctx, addr: u64, st: &Stat) -> i64 {
    let mut b = [0u8; 256];
    let p64 = |b: &mut [u8], o: usize, v: u64| b[o..o + 8].copy_from_slice(&v.to_le_bytes());
    let p32 = |b: &mut [u8], o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    p32(&mut b, 0, 0x7ff); // stx_mask: basic stats
    p32(&mut b, 4, st.blksize.max(512) as u32);
    p32(&mut b, 16, st.nlink.max(1));
    p32(&mut b, 20, st.uid);
    p32(&mut b, 24, st.gid);
    b[28..30].copy_from_slice(&(st.mode as u16).to_le_bytes());
    p64(&mut b, 32, st.ino);
    p64(&mut b, 40, st.size as u64);
    p64(&mut b, 48, st.blocks as u64);
    let ts = |b: &mut [u8], o: usize, t: (i64, i64)| {
        b[o..o + 8].copy_from_slice(&t.0.to_le_bytes());
        b[o + 8..o + 12].copy_from_slice(&(t.1 as u32).to_le_bytes());
    };
    ts(&mut b, 64, st.atime);
    ts(&mut b, 80, st.ctime); // btime
    ts(&mut b, 96, st.ctime);
    ts(&mut b, 112, st.mtime);
    p32(&mut b, 128, (st.rdev >> 8) as u32);
    p32(&mut b, 132, (st.rdev & 0xff) as u32);
    p32(&mut b, 136, (st.dev >> 8) as u32);
    p32(&mut b, 140, (st.dev & 0xff) as u32);
    if ctx.mm.write(addr, &b) {
        0
    } else {
        EFAULT
    }
}

fn read_timespec(ctx: &Ctx, addr: u64) -> Result<Duration> {
    let s = ctx.mm.read_u64(addr).ok_or(EFAULT)? as i64;
    let ns = ctx.mm.read_u64(addr + 8).ok_or(EFAULT)? as i64;
    if s < 0 || !(0..1_000_000_000).contains(&ns) {
        return Err(EINVAL);
    }
    Ok(Duration::new(s as u64, ns as u32))
}

fn write_timespec(ctx: &Ctx, addr: u64, d: Duration) -> bool {
    ctx.mm.write(
        addr,
        &[
            d.as_secs().to_le_bytes(),
            (d.subsec_nanos() as u64).to_le_bytes(),
        ]
        .concat(),
    )
}

/// Sleeps until `deadline`, returning EINTR if a signal arrives.
fn sleep_until(ctx: &Ctx, deadline: Instant) -> i64 {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return 0;
        }
        if ctx.signal_pending() {
            return EINTR;
        }
        let seen = ctx.thread.wake.epoch();
        ctx.thread
            .wake
            .wait_change(seen, Some(deadline.min(now + Duration::from_millis(50))));
    }
}

fn file(ctx: &Ctx, fd: u64) -> Result<Arc<OpenFile>> {
    ctx.fds().lock().get(fd as i32)
}

fn do_read(f: &OpenFile, buf: &mut [u8], at: Option<u64>) -> Result<usize> {
    if !f.readable() {
        return Err(EBADF);
    }
    if f.node.is_stream() {
        return f.node.read_at(0, buf, f.nonblock());
    }
    match at {
        Some(off) => f.node.read_at(off, buf, f.nonblock()),
        None => {
            let mut pos = f.pos.lock();
            let n = f.node.read_at(*pos, buf, f.nonblock())?;
            *pos += n as u64;
            Ok(n)
        }
    }
}

fn do_write(f: &OpenFile, buf: &[u8], at: Option<u64>) -> Result<usize> {
    if !f.writable() {
        return Err(EBADF);
    }
    if f.node.is_stream() {
        return f.node.write_at(0, buf, f.nonblock());
    }
    match at {
        Some(off) => f.node.write_at(off, buf, f.nonblock()),
        None => {
            let mut pos = f.pos.lock();
            if f.flags.load(Ordering::Relaxed) & O_APPEND != 0 {
                *pos = f.node.stat().size as u64;
            }
            let n = f.node.write_at(*pos, buf, f.nonblock())?;
            *pos += n as u64;
            Ok(n)
        }
    }
}

fn read_iov(ctx: &Ctx, iov: u64, cnt: u64) -> Result<Vec<(u64, usize)>> {
    if cnt > 1024 {
        return Err(EINVAL);
    }
    let mut v = Vec::with_capacity(cnt as usize);
    for i in 0..cnt {
        let base = ctx.mm.read_u64(iov + i * 16).ok_or(EFAULT)?;
        let len = ctx.mm.read_u64(iov + i * 16 + 8).ok_or(EFAULT)?;
        v.push((base, len as usize));
    }
    Ok(v)
}

fn ret<T: Into<i64>>(r: Result<T>) -> i64 {
    match r {
        Ok(v) => v.into(),
        Err(e) => e,
    }
}

fn syscall(ctx: &Ctx, cpu: &mut Cpu, nr: u64, a: [u64; 6]) -> i64 {
    let mm = &ctx.mm;
    match nr {
        // ---- files ----
        56 => {
            // openat
            let r = (|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let flags = a[2] as u32;
                let (gpath, node) = open_path(ctx, a[0] as i64, &path, flags, a[3] as u32)?;
                let f = OpenFile::new(
                    node,
                    flags & !(O_CREAT | O_EXCL | O_TRUNC | O_CLOEXEC),
                    gpath,
                );
                let fds = ctx.fds();
                let n = fds.lock().insert(f, flags & O_CLOEXEC != 0)?;
                Ok(n as i64)
            })();
            ret(r)
        }
        57 => ret(ctx.fds().lock().close(a[0] as i32).map(|_| 0i64)),
        436 => {
            // close_range
            let fds = ctx.fds();
            let mut t = fds.lock();
            let keys: Vec<i32> = t
                .fds
                .range(a[0] as i32..=(a[1].min(i32::MAX as u64) as i32))
                .map(|(k, _)| *k)
                .collect();
            for k in keys {
                if a[2] & 4 != 0 {
                    if let Some(f) = t.fds.get_mut(&k) {
                        f.cloexec = true;
                    }
                } else {
                    t.fds.remove(&k);
                }
            }
            0
        }
        63 | 67 => {
            let r = (|| -> Result<i64> {
                let f = file(ctx, a[0])?;
                let len = (a[2] as usize).min(1 << 24);
                let mut buf = vec![0u8; len];
                let n = do_read(&f, &mut buf, (nr == 67).then_some(a[3]))?;
                if !mm.write(a[1], &buf[..n]) {
                    return Err(EFAULT);
                }
                Ok(n as i64)
            })();
            ret(r)
        }
        64 | 68 => {
            let r = (|| -> Result<i64> {
                let f = file(ctx, a[0])?;
                let len = (a[2] as usize).min(1 << 24);
                let mut buf = vec![0u8; len];
                if !mm.read(a[1], &mut buf) {
                    return Err(EFAULT);
                }
                Ok(do_write(&f, &buf, (nr == 68).then_some(a[3]))? as i64)
            })();
            ret(r)
        }
        65 | 69 => {
            let r = (|| -> Result<i64> {
                let f = file(ctx, a[0])?;
                let iov = read_iov(ctx, a[1], a[2])?;
                let mut total = 0usize;
                let mut off = (nr == 69).then_some(a[3]);
                for (base, len) in iov {
                    if len == 0 {
                        continue;
                    }
                    let mut buf = vec![0u8; len.min(1 << 24)];
                    let n = match do_read(&f, &mut buf, off) {
                        Ok(n) => n,
                        Err(e) if total == 0 => return Err(e),
                        Err(_) => break,
                    };
                    if !mm.write(base, &buf[..n]) {
                        return Err(EFAULT);
                    }
                    total += n;
                    if let Some(o) = off.as_mut() {
                        *o += n as u64;
                    }
                    if n < len {
                        break;
                    }
                }
                Ok(total as i64)
            })();
            ret(r)
        }
        66 | 70 => {
            let r = (|| -> Result<i64> {
                let f = file(ctx, a[0])?;
                let iov = read_iov(ctx, a[1], a[2])?;
                let mut data = Vec::new();
                for (base, len) in iov {
                    let mut buf = vec![0u8; len.min(1 << 24)];
                    if !mm.read(base, &mut buf) {
                        return Err(EFAULT);
                    }
                    data.extend_from_slice(&buf);
                }
                Ok(do_write(&f, &data, (nr == 70).then_some(a[3]))? as i64)
            })();
            ret(r)
        }
        62 => {
            // lseek
            let r = (|| -> Result<i64> {
                let f = file(ctx, a[0])?;
                if f.node.is_stream() {
                    return Err(ESPIPE);
                }
                let mut pos = f.pos.lock();
                let new = match a[2] {
                    0 => a[1] as i64,
                    1 => *pos as i64 + a[1] as i64,
                    2 => f.node.stat().size + a[1] as i64,
                    3 | 4 => {
                        // SEEK_DATA / SEEK_HOLE: no holes.
                        let size = f.node.stat().size;
                        if a[1] as i64 >= size {
                            return Err(ENXIO);
                        }
                        if a[2] == 3 {
                            a[1] as i64
                        } else {
                            size
                        }
                    }
                    _ => return Err(EINVAL),
                };
                if new < 0 {
                    return Err(EINVAL);
                }
                *pos = new as u64;
                // Directory rewind.
                if new == 0 {
                    *f.dir_cursor.lock() = None;
                }
                Ok(new)
            })();
            ret(r)
        }
        61 => {
            // getdents64
            let r = (|| -> Result<i64> {
                let f = file(ctx, a[0])?;
                let mut cur = f.dir_cursor.lock();
                if cur.is_none() {
                    *cur = Some((f.node.readdir()?, 0));
                }
                let (entries, idx) = cur.as_mut().unwrap();
                let mut out = Vec::new();
                while *idx < entries.len() {
                    let e = &entries[*idx];
                    let reclen = (19 + e.name.len() + 1 + 7) & !7;
                    if out.len() + reclen > a[2] as usize {
                        break;
                    }
                    let start = out.len();
                    out.extend_from_slice(&e.ino.to_le_bytes());
                    out.extend_from_slice(&((*idx + 1) as u64).to_le_bytes());
                    out.extend_from_slice(&(reclen as u16).to_le_bytes());
                    out.push(e.kind);
                    out.extend_from_slice(&e.name);
                    out.resize(start + reclen, 0);
                    *idx += 1;
                }
                if out.is_empty() && *idx < entries.len() {
                    return Err(EINVAL);
                }
                if !mm.write(a[1], &out) {
                    return Err(EFAULT);
                }
                Ok(out.len() as i64)
            })();
            ret(r)
        }
        79 => {
            // newfstatat
            let r = (|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let flags = a[3] as u32;
                if path.is_empty() && flags & AT_EMPTY_PATH != 0 {
                    let f = file(ctx, a[0])?;
                    return Ok(write_stat(ctx, a[2], &f.node.stat()));
                }
                let (g, loc) = resolve(ctx, a[0] as i64, &path, flags & AT_SYMLINK_NOFOLLOW == 0)?;
                let st = stat_loc(ctx, &g, &loc)?;
                Ok(write_stat(ctx, a[2], &st))
            })();
            ret(r)
        }
        80 => ret(file(ctx, a[0]).map(|f| write_stat(ctx, a[1], &f.node.stat()))),
        291 => {
            // statx(dirfd, path, flags, mask, buf)
            let r = (|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let flags = a[2] as u32;
                let st = if path.is_empty() && flags & AT_EMPTY_PATH != 0 {
                    file(ctx, a[0])?.node.stat()
                } else {
                    let (g, loc) =
                        resolve(ctx, a[0] as i64, &path, flags & AT_SYMLINK_NOFOLLOW == 0)?;
                    stat_loc(ctx, &g, &loc)?
                };
                Ok(write_statx(ctx, a[4], &st))
            })();
            ret(r)
        }
        48 | 439 => {
            // faccessat(2)
            let r = (|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let (g, loc) = resolve(ctx, a[0] as i64, &path, true)?;
                let st = stat_loc(ctx, &g, &loc)?;
                let mode = a[2] as u32;
                if mode & 2 != 0 {
                    if let Loc::Host { rw: false, .. } = loc {
                        if st.mode & S_IFMT != S_IFCHR {
                            return Err(EROFS);
                        }
                    }
                }
                if mode & 1 != 0 && st.mode & 0o111 == 0 && st.mode & S_IFMT != S_IFDIR {
                    return Err(EACCES);
                }
                Ok(0)
            })();
            ret(r)
        }
        78 => {
            // readlinkat
            let r = (|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let target = readlink(ctx, a[0] as i64, &path)?;
                let n = target.len().min(a[3] as usize);
                if !mm.write(a[2], &target.as_bytes()[..n]) {
                    return Err(EFAULT);
                }
                Ok(n as i64)
            })();
            ret(r)
        }
        17 => {
            let cwd = ctx.process.cwd.lock().clone();
            let b = [cwd.as_bytes(), b"\0"].concat();
            if b.len() > a[1] as usize {
                ERANGE
            } else if mm.write(a[0], &b) {
                b.len() as i64
            } else {
                EFAULT
            }
        }
        49 | 50 => {
            let r = (|| -> Result<i64> {
                let g = if nr == 49 {
                    let path = cstr(ctx, a[0])?;
                    let (g, loc) = resolve(ctx, AT_FDCWD, &path, true)?;
                    let st = stat_loc(ctx, &g, &loc)?;
                    if st.mode & S_IFMT != S_IFDIR {
                        return Err(ENOTDIR);
                    }
                    g
                } else {
                    file(ctx, a[0])?.path.clone()
                };
                *ctx.process.cwd.lock() = g;
                Ok(0)
            })();
            ret(r)
        }
        51 => 0, // chroot: ignore
        34 => {
            // mkdirat
            ret((|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let (g, loc) = resolve(ctx, a[0] as i64, &path, false)?;
                match loc {
                    Loc::Missing { host, rw: true } => {
                        std::fs::create_dir(&host).map_err(|e| host_err(&e))?;
                        notify_path(&g, 0x4000_0100);
                        Ok(0)
                    }
                    Loc::Missing { rw: false, .. } => Err(EROFS),
                    _ => Err(EEXIST),
                }
            })())
        }
        33 => {
            // mknodat: FIFOs/devices: create a plain file placeholder.
            ret((|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let (_, loc) = resolve(ctx, a[0] as i64, &path, false)?;
                match loc {
                    Loc::Missing { host, rw: true } => {
                        std::fs::File::create(&host).map_err(|e| host_err(&e))?;
                        Ok(0)
                    }
                    Loc::Missing { rw: false, .. } => Err(EROFS),
                    _ => Err(EEXIST),
                }
            })())
        }
        35 => {
            // unlinkat
            ret((|| -> Result<i64> {
                let path = cstr(ctx, a[1])?;
                let (g, loc) = resolve(ctx, a[0] as i64, &path, false)?;
                ctx.kernel.vfs.unix_sockets.lock().remove(&g);
                match loc {
                    Loc::Host { host, rw: true } => {
                        if a[2] as u32 & AT_REMOVEDIR != 0 {
                            std::fs::remove_dir(&host).map_err(|e| host_err(&e))?;
                        } else {
                            let md = std::fs::symlink_metadata(&host).map_err(|e| host_err(&e))?;
                            if md.is_dir() {
                                return Err(EISDIR);
                            }
                            std::fs::remove_file(&host).map_err(|e| host_err(&e))?;
                        }
                        Ok(0)
                    }
                    Loc::Host { rw: false, .. } => Err(EROFS),
                    Loc::Missing { .. } => Err(ENOENT),
                    _ => Err(EPERM),
                }
            })())
        }
        36 => {
            // symlinkat(target, dirfd, linkpath)
            ret((|| -> Result<i64> {
                let target = cstr(ctx, a[0])?;
                let path = cstr(ctx, a[2])?;
                let (gpath, loc) = resolve(ctx, a[1] as i64, &path, false)?;
                // Zygote's app-data isolation links /data/user/0 -> /data/data
                // over a tmpfs we don't mount; our fixed bind already does that.
                if gpath == "/data/user/0" && target == "/data/data" {
                    return Ok(0);
                }
                match loc {
                    Loc::Missing { host, rw: true } => {
                        std::os::unix::fs::symlink(&target, &host).map_err(|e| host_err(&e))?;
                        Ok(0)
                    }
                    Loc::Missing { rw: false, .. } => Err(EROFS),
                    _ => Err(EEXIST),
                }
            })())
        }
        37 => {
            // linkat: copy (hard links are not preserved).
            ret((|| -> Result<i64> {
                let old = cstr(ctx, a[1])?;
                let new = cstr(ctx, a[3])?;
                let (_, l1) = resolve(ctx, a[0] as i64, &old, a[4] as u32 & 0x400 != 0)?;
                let (_, l2) = resolve(ctx, a[2] as i64, &new, false)?;
                match (l1, l2) {
                    (Loc::Host { host: h1, .. }, Loc::Missing { host: h2, rw: true }) => {
                        std::fs::hard_link(&h1, &h2)
                            .or_else(|_| std::fs::copy(&h1, &h2).map(|_| ()))
                            .map_err(|e| host_err(&e))?;
                        Ok(0)
                    }
                    (_, Loc::Missing { rw: false, .. }) => Err(EROFS),
                    (Loc::Missing { .. }, _) => Err(ENOENT),
                    _ => Err(EEXIST),
                }
            })())
        }
        38 | 276 => {
            // renameat(2)
            ret((|| -> Result<i64> {
                let (d2, newp) = (a[2] as i64, cstr(ctx, a[3])?);
                let old = cstr(ctx, a[1])?;
                let (g1, l1) = resolve(ctx, a[0] as i64, &old, false)?;
                let (g2, l2) = resolve(ctx, d2, &newp, false)?;
                let h1 = match l1 {
                    Loc::Host { host, rw: true } => host,
                    Loc::Host { rw: false, .. } => return Err(EROFS),
                    _ => return Err(ENOENT),
                };
                let h2 = match l2 {
                    Loc::Host { host, rw: true } | Loc::Missing { host, rw: true } => host,
                    _ => return Err(EROFS),
                };
                if nr == 276 && a[4] & 1 != 0 && h2.exists() {
                    return Err(EEXIST);
                }
                std::fs::rename(&h1, &h2).map_err(|e| host_err(&e))?;
                let mut socks = ctx.kernel.vfs.unix_sockets.lock();
                if let Some(s) = socks.remove(&g1) {
                    socks.insert(g2, s);
                }
                Ok(0)
            })())
        }
        52 | 53 | 54 | 55 => 0, // fchmod, fchmodat, fchownat, fchown: accept
        88 => 0,                // utimensat
        46 => ret(file(ctx, a[0]).and_then(|f| f.node.truncate(a[1]).map(|_| 0i64))),
        45 => ret((|| -> Result<i64> {
            let path = cstr(ctx, a[0])?;
            let (_, node) = open_path(ctx, AT_FDCWD, &path, O_WRONLY, 0)?;
            node.truncate(a[1])?;
            Ok(0)
        })()),
        47 => ret(file(ctx, a[0]).and_then(|f| {
            let want = a[2] + a[3];
            if f.node.stat().size < want as i64 && a[1] & 1 == 0 {
                f.node.truncate(want)?;
            }
            Ok(0i64)
        })),
        81 | 82 | 83 | 267 | 223 | 213 => 0, // sync, fsync, fdatasync, syncfs, fadvise64, readahead
        32 => 0,                             // flock
        43 | 44 => {
            // statfs/fstatfs
            let gpath = if nr == 43 {
                cstr(ctx, a[0])
                    .ok()
                    .and_then(|p| resolve(ctx, AT_FDCWD, &p, true).ok())
                    .map(|(g, _)| g)
                    .unwrap_or_default()
            } else {
                file(ctx, a[0]).map(|f| f.path.clone()).unwrap_or_default()
            };
            let magic: u64 = if gpath.starts_with("/sys/fs/selinux") {
                0xf97cff8c
            } else if gpath.starts_with("/proc") {
                0x9fa0
            } else if gpath.starts_with("/sys") {
                0x62656572
            } else if gpath.starts_with("/dev")
                || gpath.starts_with("/mnt")
                || gpath.starts_with("/apex")
                || gpath.starts_with("/linkerconfig")
            {
                0x01021994
            } else {
                0xEF53
            };
            let mut b = [0u8; 120];
            b[0..8].copy_from_slice(&magic.to_le_bytes());
            b[8..16].copy_from_slice(&4096u64.to_le_bytes());
            b[16..24].copy_from_slice(&(1u64 << 20).to_le_bytes()); // blocks
            b[24..32].copy_from_slice(&(1u64 << 19).to_le_bytes()); // bfree
            b[32..40].copy_from_slice(&(1u64 << 19).to_le_bytes()); // bavail
            b[40..48].copy_from_slice(&(1u64 << 20).to_le_bytes());
            b[48..56].copy_from_slice(&(1u64 << 19).to_le_bytes());
            b[64..72].copy_from_slice(&255u64.to_le_bytes()); // namelen
            b[72..80].copy_from_slice(&4096u64.to_le_bytes()); // frsize
            if mm.write(a[1], &b) {
                0
            } else {
                EFAULT
            }
        }
        23 => ret((|| -> Result<i64> {
            let fds = ctx.fds();
            let mut t = fds.lock();
            let f = t.get(a[0] as i32)?;
            Ok(t.insert(f, false)? as i64)
        })()),
        24 => ret((|| -> Result<i64> {
            if a[0] == a[1] {
                return Err(EINVAL);
            }
            let fds = ctx.fds();
            let mut t = fds.lock();
            let f = t.get(a[0] as i32)?;
            t.insert_at(a[1] as i32, f, a[2] as u32 & O_CLOEXEC != 0);
            Ok(a[1] as i64)
        })()),
        25 => sys_fcntl(ctx, a),
        59 => ret((|| -> Result<i64> {
            let (r, w) = pipe();
            let flags = a[1] as u32;
            let fds = ctx.fds();
            let mut t = fds.lock();
            let rf = t.insert(
                OpenFile::new(r, flags & O_NONBLOCK, "pipe:"),
                flags & O_CLOEXEC != 0,
            )?;
            let wf = t.insert(
                OpenFile::new(w, O_WRONLY | (flags & O_NONBLOCK), "pipe:"),
                flags & O_CLOEXEC != 0,
            )?;
            drop(t);
            if !mm.write(
                a[0],
                &[(rf as u32).to_le_bytes(), (wf as u32).to_le_bytes()].concat(),
            ) {
                return Err(EFAULT);
            }
            Ok(0)
        })()),
        19 => {
            let flags = a[1] as u32;
            let f = OpenFile::new(
                EventFd::new(a[0], flags & 1 != 0),
                O_RDWR | (flags & O_NONBLOCK),
                "anon_inode:[eventfd]",
            );
            ret(ctx
                .process
                .fds()
                .lock()
                .insert(f, flags & O_CLOEXEC != 0)
                .map(|n| n as i64))
        }
        29 => ret((|| -> Result<i64> {
            let f = file(ctx, a[0])?;
            match a[1] as u32 {
                0x5421 => {
                    // FIONBIO
                    let on = mm.read_u32(a[2]).ok_or(EFAULT)? != 0;
                    if on {
                        f.flags.fetch_or(O_NONBLOCK, Ordering::Relaxed);
                    } else {
                        f.flags.fetch_and(!O_NONBLOCK, Ordering::Relaxed);
                    }
                    Ok(0)
                }
                0x5451 | 0x5450 => Ok(0), // FIOCLEX / FIONCLEX
                cmd => f.node.ioctl(ctx, cmd, a[2]),
            }
        })()),
        20 => {
            let f = OpenFile::new(Epoll::new(), O_RDWR, "anon_inode:[eventpoll]");
            ret(ctx
                .process
                .fds()
                .lock()
                .insert(f, a[0] as u32 & O_CLOEXEC != 0)
                .map(|n| n as i64))
        }
        21 => sys_epoll_ctl(ctx, a),
        22 | 441 => sys_epoll_wait(ctx, nr, a),
        73 | 72 => sys_ppoll(ctx, nr, a),
        279 => {
            // memfd_create
            ret((|| -> Result<i64> {
                let name = cstr(ctx, a[0])?;
                let node = Arc::new(crate::syscall::MemFd::new(&name));
                let f = OpenFile::new(node, O_RDWR, format!("/memfd:{name}"));
                Ok(ctx.fds().lock().insert(f, a[1] & 1 != 0)? as i64)
            })())
        }
        71 => {
            // sendfile(out, in, offset*, count)
            ret((|| -> Result<i64> {
                let out = file(ctx, a[0])?;
                let inp = file(ctx, a[1])?;
                let mut buf = vec![0u8; (a[3] as usize).min(1 << 20)];
                let n = if a[2] != 0 {
                    let off = mm.read_u64(a[2]).ok_or(EFAULT)?;
                    let n = do_read(&inp, &mut buf, Some(off))?;
                    mm.write_u64(a[2], off + n as u64);
                    n
                } else {
                    do_read(&inp, &mut buf, None)?
                };
                Ok(do_write(&out, &buf[..n], None)? as i64)
            })())
        }
        285 => ENOSYS, // copy_file_range: callers fall back
        // ---- memory ----
        214 => {
            let mut brk = mm.brk.lock();
            let (start, cur) = *brk;
            let want = a[0];
            if want == 0 || want < start {
                return cur as i64;
            }
            let new = page_up(want);
            if new > cur {
                let _map_guard = mm.map_lock.lock();
                if mm.find_free(cur, new - cur) != Some(cur) {
                    return cur as i64;
                }
                mm.map(
                    cur,
                    new - cur,
                    PROT_READ | PROT_WRITE,
                    Backing::Anon,
                    Arc::from("[heap]"),
                );
            } else if new < cur {
                mm.unmap(new, cur - new);
            }
            brk.1 = new;
            want as i64
        }
        222 => sys_mmap(ctx, a),
        215 => {
            if a[0] & PAGE_MASK != 0 {
                return EINVAL;
            }
            mm.unmap(a[0], a[1]);
            0
        }
        226 => {
            if a[0] & PAGE_MASK != 0 {
                return EINVAL;
            }
            if mm.protect(a[0], a[1], a[2] as u32 & 7) {
                0
            } else {
                ENOMEM
            }
        }
        216 => {
            let fixed = (a[3] & 2 != 0).then_some(a[4]);
            match mm.remap(a[0], a[1], a[2], a[3] & 1 != 0, fixed) {
                Ok(v) => v as i64,
                Err(e) => e,
            }
        }
        233 => {
            // madvise
            if a[2] == 4 || a[2] == 8 {
                // MADV_DONTNEED / MADV_FREE on private anon memory.
                if let Some(v) = mm.vma_at(a[0]) {
                    if matches!(v.backing, Backing::Anon) {
                        mm.discard(a[0], a[1]);
                    }
                }
            }
            0
        }
        227 | 228 | 229 | 230 | 231 => 0, // msync, mlock*, munlockall
        232 => {
            // mincore: everything resident
            let n = (page_up(a[1]) / PAGE) as usize;
            if mm.write(a[2], &vec![1u8; n]) {
                0
            } else {
                EFAULT
            }
        }
        283 => 0, // membarrier
        // ---- processes / threads ----
        220 => sys_clone(ctx, cpu, a[0], a[1], a[2], a[3], a[4]),
        435 => {
            // clone3(struct clone_args*, size)
            let mut b = [0u8; 88];
            if !mm.read(a[0], &mut b[..(a[1] as usize).min(88)]) {
                return EFAULT;
            }
            let g = |i: usize| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
            let flags = g(0) | (g(4) & 0xff);
            let stack = if g(5) != 0 { g(5) + g(6) } else { 0 };
            sys_clone(ctx, cpu, flags, stack, g(3), g(7), g(2))
        }
        260 => sys_wait4(ctx, a[0] as i32, a[1], a[2] as u32),
        95 => sys_waitid(ctx, a),
        96 => {
            ctx.thread.clear_child_tid.store(a[0], Ordering::Relaxed);
            ctx.thread.tid as i64
        }
        99 => {
            ctx.thread.robust_list.store(a[0], Ordering::Relaxed);
            0
        }
        100 => ENOSYS,
        172 => ctx.process.pid as i64,
        173 => ctx.process.ppid.load(Ordering::Relaxed) as i64,
        178 => ctx.thread.tid as i64,
        174 | 175 => ctx.process.uid.load(Ordering::Relaxed) as i64,
        176 | 177 => ctx.process.gid.load(Ordering::Relaxed) as i64,
        146 | 151 => {
            ctx.process.uid.store(a[0] as u32, Ordering::Relaxed);
            0
        }
        144 | 152 => {
            ctx.process.gid.store(a[0] as u32, Ordering::Relaxed);
            0
        }
        145 | 147 => {
            let v = if nr == 147 { a[1] } else { a[1] };
            if v as i32 != -1 {
                ctx.process.uid.store(v as u32, Ordering::Relaxed);
            }
            0
        }
        143 | 149 => {
            if a[1] as i32 != -1 {
                ctx.process.gid.store(a[1] as u32, Ordering::Relaxed);
            }
            0
        }
        148 | 150 => {
            let v = if nr == 148 {
                ctx.process.uid.load(Ordering::Relaxed)
            } else {
                ctx.process.gid.load(Ordering::Relaxed)
            };
            for p in [a[0], a[1], a[2]] {
                mm.write_u32(p, v);
            }
            0
        }
        158 => {
            let g = ctx.process.groups.lock().clone();
            if a[0] == 0 {
                return g.len() as i64;
            }
            if (a[0] as usize) < g.len() {
                return EINVAL;
            }
            let b: Vec<u8> = g.iter().flat_map(|x| x.to_le_bytes()).collect();
            mm.write(a[1], &b);
            g.len() as i64
        }
        159 => {
            let mut b = vec![0u8; (a[0] as usize).min(65536) * 4];
            mm.read(a[1], &mut b);
            *ctx.process.groups.lock() = b
                .chunks(4)
                .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            0
        }
        154 => {
            // setpgid
            let pid = if a[0] == 0 {
                ctx.process.pid
            } else {
                a[0] as i32
            };
            let pg = if a[1] == 0 { pid } else { a[1] as i32 };
            match ctx.kernel.procs.lock().get(&pid) {
                Some(p) => {
                    p.pgid.store(pg, Ordering::Relaxed);
                    0
                }
                None => ESRCH,
            }
        }
        155 => {
            let pid = if a[0] == 0 {
                ctx.process.pid
            } else {
                a[0] as i32
            };
            ctx.kernel
                .procs
                .lock()
                .get(&pid)
                .map(|p| p.pgid.load(Ordering::Relaxed) as i64)
                .unwrap_or(ESRCH)
        }
        156 => {
            let pid = if a[0] == 0 {
                ctx.process.pid
            } else {
                a[0] as i32
            };
            ctx.kernel
                .procs
                .lock()
                .get(&pid)
                .map(|p| p.sid.load(Ordering::Relaxed) as i64)
                .unwrap_or(ESRCH)
        }
        157 => {
            ctx.process.sid.store(ctx.process.pid, Ordering::Relaxed);
            ctx.process.pgid.store(ctx.process.pid, Ordering::Relaxed);
            ctx.process.pid as i64
        }
        166 => ctx
            .process
            .umask
            .swap(a[0] as u32 & 0o777, Ordering::Relaxed) as i64,
        167 => sys_prctl(ctx, a),
        90 => {
            // capget: full capabilities
            if a[1] != 0 {
                let d = [u32::MAX, u32::MAX, u32::MAX, 0x3ff, 0x3ff, 0x3ff];
                let b: Vec<u8> = d.iter().flat_map(|x| x.to_le_bytes()).collect();
                mm.write(a[1], &b);
            }
            0
        }
        91 => 0,       // capset
        92 => 0,       // personality
        97 | 268 => 0, // unshare, setns
        30 => 0,       // ioprio_set
        31 => 4 << 13, // ioprio_get: best-effort class
        277 => 0,      // seccomp: accept filters, never enforce
        280 => crate::bpf::syscall(ctx, a[0], a[1], a[2]),
        270 | 271 => sys_process_vm(ctx, nr == 271, a),
        117 => EPERM, // ptrace
        116 => 0,     // syslog
        142 => {
            // reboot
            ctx.kernel.log("[kernel] reboot requested");
            ctx.kernel.exit_group(&ctx.process, ExitStatus::Exited(0));
            0
        }
        105 | 273 | 106 => 0, // init_module, finit_module, delete_module
        // ---- scheduling ----
        124 => {
            std::thread::yield_now();
            0
        }
        118..=121 | 140 => 0,
        141 => 20, // getpriority -> nice 0
        122 => 0,
        123 => {
            // sched_getaffinity: CPUs 0..7
            let n = (a[1] as usize).min(128);
            let mut b = vec![0u8; n];
            if n > 0 {
                b[0] = 0xff;
            }
            if mm.write(a[2], &b) {
                n as i64
            } else {
                EFAULT
            }
        }
        125 => {
            if a[0] == 1 || a[0] == 2 {
                99
            } else {
                0
            }
        } // sched_get_priority_max
        126 => {
            if a[0] == 1 || a[0] == 2 {
                1
            } else {
                0
            }
        } // sched_get_priority_min
        168 => {
            if a[0] != 0 {
                mm.write_u32(a[0], 0);
            }
            if a[1] != 0 {
                mm.write_u32(a[1], 0);
            }
            0
        }
        // ---- signals ----
        134 => sys_rt_sigaction(ctx, a),
        135 => sys_rt_sigprocmask(ctx, a),
        136 => {
            let set = ctx.thread.pending.lock().set | ctx.process.pending.lock().set;
            if mm.write_u64(a[0], set) {
                0
            } else {
                EFAULT
            }
        }
        133 => {
            // rt_sigsuspend
            let Some(mask) = mm.read_u64(a[0]) else {
                return EFAULT;
            };
            let old = ctx.thread.sigmask.swap(mask, Ordering::SeqCst);
            loop {
                if ctx.signal_pending() {
                    break;
                }
                let seen = ctx.thread.wake.epoch();
                ctx.thread
                    .wake
                    .wait_change(seen, Some(Instant::now() + Duration::from_millis(50)));
            }
            // Deliver with the temporary mask, then restore the old one after
            // the handler returns: approximate by restoring now but keeping
            // the signal deliverable.
            *ctx.thread.saved_mask.lock() = Some(old);
            ctx.thread.sigmask.store(old & mask, Ordering::SeqCst);
            EINTR
        }
        137 => sys_sigtimedwait(ctx, a),
        132 => sys_sigaltstack(ctx, a),
        129 => sys_kill(ctx, a[0] as i32, a[1] as u32),
        130 | 131 => {
            let (tid, sig) = if nr == 131 {
                (a[1] as i32, a[2] as u32)
            } else {
                (a[0] as i32, a[1] as u32)
            };
            let t = ctx.kernel.threads.lock().get(&tid).cloned();
            match t {
                Some(t) => {
                    if sig != 0 {
                        ctx.kernel
                            .send_signal_thread(&t, SigInfo::simple(sig, -6, ctx.process.pid, 0));
                    }
                    0
                }
                None => ESRCH,
            }
        }
        138 | 240 => {
            // rt_sigqueueinfo / rt_tgsigqueueinfo
            let (pid, sig, uinfo) = if nr == 138 {
                (a[0] as i32, a[1] as u32, a[2])
            } else {
                (a[1] as i32, a[2] as u32, a[3])
            };
            let mut info = SigInfo::simple(sig, -1, ctx.process.pid, 0);
            mm.read(uinfo, &mut info.bytes);
            info.signo = sig;
            // Linux reports the queued signal number in si_signo (bionic's
            // abort() leaves it zero in the siginfo it passes).
            info.bytes[0..4].copy_from_slice(&(sig as i32).to_le_bytes());
            if nr == 240 {
                if let Some(t) = ctx.kernel.threads.lock().get(&pid).cloned() {
                    ctx.kernel.send_signal_thread(&t, info);
                    return 0;
                }
                return ESRCH;
            }
            match ctx.kernel.procs.lock().get(&pid).cloned() {
                Some(p) => {
                    ctx.kernel.send_signal_process(&p, info);
                    0
                }
                None => ESRCH,
            }
        }
        74 => {
            // signalfd4(fd, mask*, sizemask, flags)
            let Some(mask) = mm.read_u64(a[1]) else {
                return EFAULT;
            };
            let flags = a[3] as u32;
            if a[0] as i32 != -1 {
                return ret(file(ctx, a[0]).and_then(|f| {
                    let s = f.node.as_any().downcast_ref::<SignalFd>().ok_or(EINVAL)?;
                    *s.mask.lock() = mask;
                    Ok(a[0] as i64)
                }));
            }
            let node = Arc::new(SignalFd {
                mask: Mutex::new(mask),
                process: Arc::downgrade(&ctx.process),
                id: new_id(),
            });
            let f = OpenFile::new(node, O_RDWR | (flags & O_NONBLOCK), "anon_inode:[signalfd]");
            ret(ctx
                .process
                .fds()
                .lock()
                .insert(f, flags & O_CLOEXEC != 0)
                .map(|n| n as i64))
        }
        40 | 39 | 41 => 0, // mount, umount2, pivot_root: the tree is fixed
        219 => ENOSYS,     // keyctl
        // xattrs (SELinux labels): setting succeeds and is forgotten; reads
        // report no attribute.
        5..=7 | 14..=16 => 0,
        8..=10 => sys_getxattr(ctx, nr, a),
        11..=13 => 0,
        // ---- time ----
        113 => {
            let d = match a[0] {
                0 | 8 | 11 => crate::time::realtime(), // REALTIME(_ALARM), TAI
                2 => ctx.process.start_time.elapsed(), // PROCESS_CPUTIME (approx)
                3 => Duration::from_nanos(0) + ctx.process.start_time.elapsed(),
                _ => crate::time::monotonic(),
            };
            if write_timespec(ctx, a[1], d) {
                0
            } else {
                EFAULT
            }
        }
        114 => {
            if a[1] != 0 && !write_timespec(ctx, a[1], Duration::from_nanos(1)) {
                return EFAULT;
            }
            0
        }
        169 => {
            let d = crate::time::realtime();
            if a[0] != 0 {
                mm.write(
                    a[0],
                    &[
                        d.as_secs().to_le_bytes(),
                        (d.subsec_micros() as u64).to_le_bytes(),
                    ]
                    .concat(),
                );
            }
            if a[1] != 0 {
                mm.write(a[1], &[0u8; 8]);
            }
            0
        }
        170 | 112 => 0, // settimeofday, clock_settime
        101 => match read_timespec(ctx, a[0]) {
            Ok(d) => {
                let r = sleep_until(ctx, Instant::now() + d);
                if r == EINTR && a[1] != 0 {
                    write_timespec(ctx, a[1], Duration::ZERO);
                }
                r
            }
            Err(e) => e,
        },
        115 => match read_timespec(ctx, a[2]) {
            Ok(d) => {
                let deadline = if a[1] & 1 != 0 {
                    crate::time::deadline(a[0] as u32, d)
                } else {
                    Instant::now() + d
                };
                sleep_until(ctx, deadline)
            }
            Err(e) => e,
        },
        153 => {
            // times
            let t = ctx.process.start_time.elapsed().as_millis() as u64 / 10;
            if a[0] != 0 {
                mm.write(
                    a[0],
                    &[
                        t.to_le_bytes(),
                        0u64.to_le_bytes(),
                        0u64.to_le_bytes(),
                        0u64.to_le_bytes(),
                    ]
                    .concat(),
                );
            }
            (crate::time::monotonic().as_millis() / 10) as i64
        }
        165 => {
            // getrusage
            let mut b = [0u8; 144];
            let t = ctx.process.start_time.elapsed();
            b[0..8].copy_from_slice(&t.as_secs().to_le_bytes());
            b[8..16].copy_from_slice(&(t.subsec_micros() as u64).to_le_bytes());
            b[32..40].copy_from_slice(&(64u64 << 10).to_le_bytes()); // maxrss KiB
            if mm.write(a[1], &b) {
                0
            } else {
                EFAULT
            }
        }
        102 | 103 => {
            if a.len() > 2 && a[2] != 0 {
                mm.write(a[2], &[0u8; 32]);
            }
            0
        }
        85 => {
            let flags = a[1] as u32;
            let f = OpenFile::new(
                crate::syscall::TimerFd::new(a[0] as u32),
                O_RDWR | (flags & O_NONBLOCK),
                "anon_inode:[timerfd]",
            );
            ret(ctx
                .process
                .fds()
                .lock()
                .insert(f, flags & O_CLOEXEC != 0)
                .map(|n| n as i64))
        }
        86 => ret((|| -> Result<i64> {
            let f = file(ctx, a[0])?;
            let t = f.node.as_any().downcast_ref::<TimerFd>().ok_or(EINVAL)?;
            let interval = read_timespec(ctx, a[2])?;
            let value = read_timespec(ctx, a[2] + 16)?;
            if a[3] != 0 {
                let (i, v) = t.get();
                write_timespec(ctx, a[3], i);
                write_timespec(ctx, a[3] + 16, v);
            }
            t.set(interval, value, a[1] & 1 != 0);
            Ok(0)
        })()),
        87 => ret((|| -> Result<i64> {
            let f = file(ctx, a[0])?;
            let t = f.node.as_any().downcast_ref::<TimerFd>().ok_or(EINVAL)?;
            let (i, v) = t.get();
            write_timespec(ctx, a[1], i);
            write_timespec(ctx, a[1] + 16, v);
            Ok(0)
        })()),
        107 => {
            // timer_create(clock, sigevent*, timer_t*)
            let (signo, notify, tid) = if a[1] != 0 {
                let mut b = [0u8; 24];
                if !mm.read(a[1], &mut b) {
                    return EFAULT;
                }
                let signo = u32::from_le_bytes(b[8..12].try_into().unwrap());
                let notify = i32::from_le_bytes(b[12..16].try_into().unwrap());
                let tid = i32::from_le_bytes(b[16..20].try_into().unwrap());
                (signo, notify, tid)
            } else {
                (SIGALRM, 0, 0)
            };
            let t = PosixTimer::new(ctx, signo, notify, tid);
            let mut timers = TIMERS.lock();
            let id = timers.len() as u32;
            timers.push(Some(t));
            if mm.write_u32(a[2], id) {
                0
            } else {
                EFAULT
            }
        }
        110 | 108 => {
            // timer_settime(id, flags, new*, old*) / timer_gettime(id, cur*)
            let Some(t) = TIMERS.lock().get(a[0] as usize).cloned().flatten() else {
                return EINVAL;
            };
            let out = if nr == 108 { a[1] } else { a[3] };
            if out != 0 {
                let (i, v) = t.get();
                write_timespec(ctx, out, i);
                write_timespec(ctx, out + 16, v);
            }
            if nr == 110 {
                let (Ok(i), Ok(v)) = (read_timespec(ctx, a[2]), read_timespec(ctx, a[2] + 16))
                else {
                    return EFAULT;
                };
                t.set(i, v, a[1] & 1 != 0);
            }
            0
        }
        109 => 0,
        111 => {
            let mut timers = TIMERS.lock();
            match timers.get_mut(a[0] as usize) {
                Some(slot @ Some(_)) => {
                    if let Some(t) = slot.take() {
                        t.set(Duration::ZERO, Duration::ZERO, false);
                    }
                    0
                }
                _ => EINVAL,
            }
        }
        98 => crate::futex::sys_futex(ctx, a[0], a[1] as u32, a[2] as u32, a[3], a[4], a[5] as u32),
        // ---- misc ----
        160 => {
            let mut buf = vec![0u8; 65 * 6];
            let host = ctx.kernel.hostname.lock().clone();
            for (i, s) in [
                "Linux",
                host.as_str(),
                "5.15.110-indroid",
                "#1 SMP PREEMPT",
                "aarch64",
                "localdomain",
            ]
            .iter()
            .enumerate()
            {
                let n = s.len().min(64);
                buf[i * 65..i * 65 + n].copy_from_slice(&s.as_bytes()[..n]);
            }
            if mm.write(a[0], &buf) {
                0
            } else {
                EFAULT
            }
        }
        161 => {
            let mut b = vec![0u8; (a[1] as usize).min(64)];
            mm.read(a[0], &mut b);
            *ctx.kernel.hostname.lock() = String::from_utf8_lossy(&b).into_owned();
            0
        }
        162 => 0,
        163 | 164 | 261 => sys_prlimit(ctx, nr, a),
        179 => {
            // sysinfo
            let mut b = [0u8; 112];
            b[0..8].copy_from_slice(&crate::time::monotonic().as_secs().to_le_bytes());
            b[32..40].copy_from_slice(&(4u64 << 30).to_le_bytes()); // totalram
            b[40..48].copy_from_slice(&(2u64 << 30).to_le_bytes()); // freeram
            b[80..82].copy_from_slice(&64u16.to_le_bytes()); // procs
            b[104..108].copy_from_slice(&1u32.to_le_bytes()); // mem_unit
            if mm.write(a[0], &b) {
                0
            } else {
                EFAULT
            }
        }
        278 => {
            let mut b = vec![0u8; (a[1] as usize).min(1 << 20)];
            fill_random(&mut b);
            if mm.write(a[0], &b) {
                b.len() as i64
            } else {
                EFAULT
            }
        }
        293 => ENOSYS, // rseq
        424 | 434 => ENOSYS,
        26 => {
            let flags = a[0] as u32;
            let f = OpenFile::new(
                crate::input::Inotify::new(),
                O_RDONLY_FLAG | (flags & O_NONBLOCK),
                "anon_inode:inotify",
            );
            ret(ctx
                .process
                .fds()
                .lock()
                .insert(f, flags & O_CLOEXEC != 0)
                .map(|n| n as i64))
        }
        27 => ret((|| -> Result<i64> {
            let f = file(ctx, a[0])?;
            let ino = f
                .node
                .as_any()
                .downcast_ref::<crate::input::Inotify>()
                .ok_or(EINVAL)?;
            let path = cstr(ctx, a[1])?;
            let (g, loc) = resolve(ctx, AT_FDCWD, &path, true)?;
            if let Loc::Missing { .. } = loc {
                return Err(ENOENT);
            }
            Ok(ino.add_watch(&g, a[2] as u32) as i64)
        })()),
        28 => ret((|| -> Result<i64> {
            let f = file(ctx, a[0])?;
            let ino = f
                .node
                .as_any()
                .downcast_ref::<crate::input::Inotify>()
                .ok_or(EINVAL)?;
            if ino.rm_watch(a[1] as i32) {
                Ok(0)
            } else {
                Err(EINVAL)
            }
        })()),
        198..=212 | 242 | 243 | 269 => crate::net::syscall(ctx, nr, a),
        _ => {
            ctx.kernel.log(&format!(
                "[kernel] pid {} unimplemented syscall {} ({})",
                ctx.process.pid,
                nr,
                name(nr)
            ));
            ENOSYS
        }
    }
}

fn readlink(ctx: &Ctx, dirfd: i64, path: &str) -> Result<String> {
    let (_, loc) = resolve(ctx, dirfd, path, false)?;
    match loc {
        Loc::Host { host, .. } => {
            let t = std::fs::read_link(&host).map_err(|_| EINVAL)?;
            Ok(t.to_string_lossy().into_owned())
        }
        Loc::Synth(n) => match n.as_any().downcast_ref::<SymlinkNode>() {
            Some(s) => Ok(s.0.clone()),
            None => match n.as_any().downcast_ref::<crate::procfs::MagicLink>() {
                Some(m) => Ok(m.target.clone()),
                None => Err(EINVAL),
            },
        },
        Loc::Missing { .. } => Err(ENOENT),
        Loc::Device(_) => Err(EINVAL),
    }
}

fn sys_fcntl(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let fds = ctx.fds();
    let mut t = fds.lock();
    let fd = a[0] as i32;
    let Some(entry) = t.fds.get(&fd).cloned() else {
        return EBADF;
    };
    match a[1] {
        0 | 1030 => {
            // F_DUPFD / F_DUPFD_CLOEXEC
            match t.lowest_free(a[2] as i32) {
                Ok(n) => {
                    t.insert_at(n, entry.file, a[1] == 1030);
                    n as i64
                }
                Err(e) => e,
            }
        }
        1 => entry.cloexec as i64,
        2 => {
            t.fds.get_mut(&fd).unwrap().cloexec = a[2] & 1 != 0;
            0
        }
        3 => {
            (entry.file.flags.load(Ordering::Relaxed)
                | if entry.file.node.is_stream() {
                    0
                } else {
                    0x8000
                }) as i64
        }
        4 => {
            let keep = entry.file.flags.load(Ordering::Relaxed) & !(O_APPEND | O_NONBLOCK);
            entry.file.flags.store(
                keep | (a[2] as u32 & (O_APPEND | O_NONBLOCK)),
                Ordering::Relaxed,
            );
            0
        }
        5 => {
            // F_GETLK: report unlocked
            let mut b = [0u8; 2];
            b.copy_from_slice(&2u16.to_le_bytes()); // F_UNLCK
            ctx.mm.write(a[2], &b);
            0
        }
        6 | 7 | 36 | 37 | 38 => 0, // F_SETLK(W), OFD locks
        1031 => 65536,             // F_SETPIPE_SZ
        1032 => 65536,             // F_GETPIPE_SZ
        1033 | 1034 => 0,          // F_ADD_SEALS / F_GET_SEALS
        8 | 9 | 10 | 11 => 0,      // F_SETOWN etc.
        _ => EINVAL,
    }
}

fn sys_mmap(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let (addr, len, prot, flags, fd, off) =
        (a[0], a[1], a[2] as u32 & 7, a[3] as u32, a[4] as i32, a[5]);
    const MAP_SHARED: u32 = 1;
    const MAP_PRIVATE: u32 = 2;
    const MAP_FIXED: u32 = 0x10;
    const MAP_ANON: u32 = 0x20;
    const MAP_FIXED_NOREPLACE: u32 = 0x100000;
    if len == 0 || off & PAGE_MASK != 0 {
        return EINVAL;
    }
    let len = page_up(len);
    let mm = &ctx.mm;
    let _map_guard = mm.map_lock.lock();
    let start = if flags & MAP_FIXED != 0 {
        if addr & PAGE_MASK != 0 {
            return EINVAL;
        }
        addr
    } else if flags & MAP_FIXED_NOREPLACE != 0 {
        match mm.find_free(addr, len) {
            Some(x) if x == page_down(addr) => x,
            _ => return EEXIST,
        }
    } else {
        match mm.find_free(addr, len) {
            Some(x) => x,
            None => return ENOMEM,
        }
    };
    let (backing, name): (Backing, Arc<str>) = if flags & MAP_ANON != 0 {
        if flags & MAP_SHARED != 0 && flags & MAP_PRIVATE == 0 {
            (
                Backing::Shared {
                    mem: SharedMem::new(len),
                    offset: 0,
                },
                Arc::from("/dev/zero (deleted)"),
            )
        } else {
            (Backing::Anon, Arc::from(""))
        }
    } else {
        let f = match file(ctx, fd as u64) {
            Ok(f) => f,
            Err(e) => return e,
        };
        if let Some((shm, shm_off)) = f.node.shared_mem_at(off, len) {
            // Binder requires a driver-owned receive area even though libbinder
            // maps its fd with MAP_PRIVATE.
            if flags & MAP_SHARED != 0 || f.node.as_any().is::<crate::binder::Binder>() {
                if let Err(e) = f.node.mmapped(ctx, start, len, off) {
                    return e;
                }
                (
                    Backing::Shared {
                        mem: shm,
                        offset: shm_off,
                    },
                    Arc::from(f.path.as_str()),
                )
            } else {
                // Private copy of shared memory contents: snapshot pages.
                let mem = SharedMem::new(len);
                for i in 0..len / PAGE {
                    if let (Some(src), Some(dst)) = (shm.page(shm_off / PAGE + i), mem.page(i)) {
                        dst.bytes_mut().copy_from_slice(src.bytes());
                    }
                }
                (
                    Backing::Shared { mem, offset: 0 },
                    Arc::from(f.path.as_str()),
                )
            }
        } else if f.node.mmap_requires_shared_mem() {
            return EINVAL;
        } else {
            if flags & MAP_SHARED != 0 && (prot & PROT_WRITE != 0 || f.writable() || is_rw_host(&f))
            {
                // Shared mapping of a writable file: all mappers share one
                // memory object (seeded from the file). Coherent between
                // mappings (property areas, SQLite -shm), not with write().
                let mem = shared_file_mem(&f);
                (
                    Backing::Shared { mem, offset: off },
                    Arc::from(f.path.as_str()),
                )
            } else {
                (
                    Backing::File {
                        file: f.node.clone(),
                        offset: off,
                    },
                    Arc::from(f.path.as_str()),
                )
            }
        }
    };
    mm.map(start, len, prot, backing, name);
    start as i64
}

pub fn do_execve(
    ctx: &Ctx,
    cpu: &mut Cpu,
    path: &str,
    argv: Vec<Vec<u8>>,
    envp: Vec<Vec<u8>>,
) -> Result<()> {
    let (gpath, node) = open_path(ctx, AT_FDCWD, path, 0, 0)?;
    let st = node.stat();
    if st.mode & S_IFMT == S_IFDIR {
        return Err(EACCES);
    }
    if st.mode & 0o111 == 0 {
        return Err(EACCES);
    }
    let open_interp =
        |p: &str| -> Result<FileRef> { open_path(ctx, AT_FDCWD, p, 0, 0).map(|(_, n)| n) };
    let comm_src = argv.first().cloned().unwrap_or_default();
    let image = crate::elf::load(crate::elf::ExecArgs {
        file: node,
        // Scripts see the name they were invoked by (symlink personalities).
        path: if path.starts_with('/') { path } else { &gpath },
        argv: argv.clone(),
        envp,
        open_interp: &open_interp,
        hwcap: crate::HWCAP,
        hwcap2: crate::HWCAP2,
        uid: ctx.process.uid.load(Ordering::Relaxed),
        gid: ctx.process.gid.load(Ordering::Relaxed),
    })?;
    let p = &ctx.process;
    // Kill other threads.
    {
        let me = ctx.thread.tid;
        for t in p.threads.lock().iter() {
            if t.tid != me {
                t.exited.store(true, Ordering::SeqCst);
                t.wake.notify();
            }
        }
    }
    *p.mm.write() = image.mm;
    // Unshare fd table and signal handlers; close O_CLOEXEC fds.
    {
        let old = ctx.fds();
        let mut copy = old.lock().clone();
        copy.close_on_exec();
        *p.fds.write() = Arc::new(Mutex::new(copy));
        *ctx.thread.fds.lock() = None;
        let acts = p.sigactions();
        let mut a = *acts.lock();
        for x in a.iter_mut() {
            if x.handler != SIG_IGN {
                *x = SigAction::default();
            }
        }
        *p.sigactions.write() = Arc::new(Mutex::new(a));
    }
    {
        let next = p.exec_secctx.lock().take();
        let ctxs = match next {
            Some(c) => c,
            None => {
                let label = crate::selinux::file_label(&ROM_PREFIX.lock(), &gpath, S_IFREG);
                crate::selinux::compute_create(&p.secctx.lock(), &label, "process")
            }
        };
        *p.secctx.lock() = ctxs;
    }
    *p.exe.lock() = gpath.clone();
    let mut cmdline = Vec::new();
    for s in &argv {
        cmdline.extend_from_slice(s);
        cmdline.push(0);
    }
    *p.cmdline.lock() = cmdline;
    let base = gpath.rsplit('/').next().unwrap_or("").to_string();
    let _ = comm_src;
    *p.comm.lock() = base.chars().take(15).collect();
    *ctx.thread.altstack.lock() = (0, 0, 2);
    ctx.thread.clear_child_tid.store(0, Ordering::Relaxed);
    if let Some(wq) = p.vfork_done.lock().take() {
        wq.notify();
    }
    let tlb_fresh = Cpu::new();
    *cpu = tlb_fresh;
    cpu.pc = image.entry;
    cpu.sp = image.sp;
    Ok(())
}

fn read_strv(ctx: &Ctx, mut p: u64) -> Result<Vec<Vec<u8>>> {
    let mut v = Vec::new();
    if p == 0 {
        return Ok(v);
    }
    loop {
        let s = ctx.mm.read_u64(p).ok_or(EFAULT)?;
        if s == 0 {
            break;
        }
        v.push(ctx.mm.read_cstr(s, 1 << 17).ok_or(EFAULT)?);
        p += 8;
        if v.len() > 65536 {
            return Err(E2BIG);
        }
    }
    Ok(v)
}

fn sys_execve(ctx: &Ctx, cpu: &mut Cpu, nr: u64, a: [u64; 6]) -> i64 {
    let r = (|| -> Result<()> {
        let (path, argv_p, envp_p) = if nr == 221 {
            (cstr(ctx, a[0])?, a[1], a[2])
        } else {
            let p = cstr(ctx, a[1])?;
            let full = if p.is_empty() && a[4] & AT_EMPTY_PATH as u64 != 0 {
                file(ctx, a[0])?.path.clone()
            } else if p.starts_with('/') {
                p
            } else {
                format!("{}/{}", dir_of(ctx, a[0] as i64, &p)?, p)
            };
            (full, a[2], a[3])
        };
        let argv = read_strv(ctx, argv_p)?;
        let envp = read_strv(ctx, envp_p)?;
        if ctx.kernel.trace_syscalls.load(Ordering::Relaxed) {
            let args: Vec<String> = argv
                .iter()
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect();
            ctx.kernel
                .log(&format!("[{}] execve {path} {args:?}", ctx.thread.tid));
        }
        do_execve(ctx, cpu, &path, argv, envp)
    })();
    match r {
        Ok(()) => 0,
        Err(e) => e,
    }
}

const CLONE_VM: u64 = 0x100;
const CLONE_FS: u64 = 0x200;
const CLONE_FILES: u64 = 0x400;
const CLONE_SIGHAND: u64 = 0x800;
const CLONE_PIDFD: u64 = 0x1000;
const CLONE_VFORK: u64 = 0x4000;
const CLONE_PARENT: u64 = 0x8000;
const CLONE_THREAD: u64 = 0x10000;
const CLONE_SETTLS: u64 = 0x80000;
const CLONE_PARENT_SETTID: u64 = 0x100000;
const CLONE_CHILD_CLEARTID: u64 = 0x200000;
const CLONE_CHILD_SETTID: u64 = 0x1000000;

fn sys_clone(
    ctx: &Ctx,
    cpu: &mut Cpu,
    flags: u64,
    stack: u64,
    ptid: u64,
    tls: u64,
    ctid: u64,
) -> i64 {
    let k = &ctx.kernel;
    let tid = k.alloc_pid();
    let mut child_cpu = cpu.clone();
    child_cpu.x[0] = 0;
    if stack != 0 {
        child_cpu.sp = stack;
    }
    if flags & CLONE_SETTLS != 0 {
        child_cpu.tpidr_el0 = tls;
    }
    child_cpu.exclusive = None;
    let p = &ctx.process;
    let mut vfork_wait = None;
    let (process, thread) = if flags & CLONE_THREAD != 0 {
        let t = Arc::new(Thread {
            tid,
            process: parking_lot::RwLock::new(p.clone()),
            sigmask: std::sync::atomic::AtomicU64::new(ctx.thread.sigmask.load(Ordering::Relaxed)),
            pending: Mutex::new(Pending::default()),
            clear_child_tid: std::sync::atomic::AtomicU64::new(
                if flags & CLONE_CHILD_CLEARTID != 0 {
                    ctid
                } else {
                    0
                },
            ),
            robust_list: std::sync::atomic::AtomicU64::new(0),
            altstack: Mutex::new((0, 0, 2)),
            exited: std::sync::atomic::AtomicBool::new(false),
            wake: crate::sync::WaitQueue::new(),
            name: Mutex::new(ctx.thread.name.lock().clone()),
            saved_mask: Mutex::new(None),
            fds: Mutex::new(if flags & CLONE_FILES != 0 {
                ctx.thread.fds.lock().clone()
            } else {
                Some(Arc::new(Mutex::new(ctx.fds().lock().clone())))
            }),
            snap: Default::default(),
        });
        p.threads.lock().push(t.clone());
        (p.clone(), t)
    } else {
        let mm = if flags & CLONE_VM != 0 {
            ctx.mm.clone()
        } else {
            Arc::new(ctx.mm.fork())
        };
        let fds = if flags & CLONE_FILES != 0 {
            ctx.fds()
        } else {
            Arc::new(Mutex::new(ctx.fds().lock().clone()))
        };
        let acts = if flags & CLONE_SIGHAND != 0 {
            p.sigactions()
        } else {
            Arc::new(Mutex::new(*p.sigactions().lock()))
        };
        let np = Arc::new(Process {
            pid: tid,
            ppid: std::sync::atomic::AtomicI32::new(if flags & CLONE_PARENT != 0 {
                p.ppid.load(Ordering::Relaxed)
            } else {
                p.pid
            }),
            pgid: std::sync::atomic::AtomicI32::new(p.pgid.load(Ordering::Relaxed)),
            sid: std::sync::atomic::AtomicI32::new(p.sid.load(Ordering::Relaxed)),
            mm: parking_lot::RwLock::new(mm),
            fds: parking_lot::RwLock::new(fds),
            cwd: Mutex::new(p.cwd.lock().clone()),
            umask: std::sync::atomic::AtomicU32::new(p.umask.load(Ordering::Relaxed)),
            sigactions: parking_lot::RwLock::new(acts),
            pending: Mutex::new(Pending::default()),
            threads: Mutex::new(Vec::new()),
            exit: Mutex::new(None),
            dying: std::sync::atomic::AtomicBool::new(false),
            exe: Mutex::new(p.exe.lock().clone()),
            cmdline: Mutex::new(p.cmdline.lock().clone()),
            comm: Mutex::new(p.comm.lock().clone()),
            uid: std::sync::atomic::AtomicU32::new(p.uid.load(Ordering::Relaxed)),
            gid: std::sync::atomic::AtomicU32::new(p.gid.load(Ordering::Relaxed)),
            groups: Mutex::new(p.groups.lock().clone()),
            children_wq: crate::sync::WaitQueue::new(),
            signal_wq: crate::sync::WaitQueue::new(),
            vfork_done: Mutex::new(None),
            exit_signal: std::sync::atomic::AtomicU32::new((flags & 0xff) as u32),
            parent_death_signal: std::sync::atomic::AtomicU32::new(0),
            start_time: Instant::now(),
            rlimits: Mutex::new(*p.rlimits.lock()),
            name_prctl: Mutex::new(String::new()),
            secctx: Mutex::new(p.secctx.lock().clone()),
            exec_secctx: Mutex::new(None),
        });
        if flags & CLONE_VFORK != 0 {
            let wq = crate::sync::WaitQueue::new();
            *np.vfork_done.lock() = Some(wq.clone());
            vfork_wait = Some(wq);
        }
        let t = Arc::new(Thread {
            tid,
            process: parking_lot::RwLock::new(np.clone()),
            sigmask: std::sync::atomic::AtomicU64::new(ctx.thread.sigmask.load(Ordering::Relaxed)),
            pending: Mutex::new(Pending::default()),
            clear_child_tid: std::sync::atomic::AtomicU64::new(
                if flags & CLONE_CHILD_CLEARTID != 0 {
                    ctid
                } else {
                    0
                },
            ),
            robust_list: std::sync::atomic::AtomicU64::new(0),
            altstack: Mutex::new(*ctx.thread.altstack.lock()),
            exited: std::sync::atomic::AtomicBool::new(false),
            wake: crate::sync::WaitQueue::new(),
            name: Mutex::new(ctx.thread.name.lock().clone()),
            saved_mask: Mutex::new(None),
            fds: Mutex::new(None),
            snap: Default::default(),
        });
        np.threads.lock().push(t.clone());
        k.procs.lock().insert(tid, np.clone());
        (np, t)
    };
    k.threads.lock().insert(tid, thread.clone());
    if flags & CLONE_PARENT_SETTID != 0 {
        ctx.mm.write_u32(ptid, tid as u32);
    }
    if flags & CLONE_CHILD_SETTID != 0 {
        process.mm().write_u32(ctid, tid as u32);
    }
    if flags & CLONE_PIDFD != 0 {
        // Unsupported pidfd: report -1.
        ctx.mm.write_u32(ptid, u32::MAX);
    }
    let _ = (CLONE_FS, CLONE_SIGHAND);
    k.start_thread(thread, child_cpu);
    if let Some(wq) = vfork_wait {
        loop {
            let seen = wq.epoch();
            if process.vfork_done.lock().is_none() {
                break;
            }
            wq.wait_change(seen, Some(Instant::now() + Duration::from_millis(50)));
        }
    }
    tid as i64
}

fn sys_wait4(ctx: &Ctx, pid: i32, status: u64, options: u32) -> i64 {
    const WNOHANG: u32 = 1;
    let me = ctx.process.pid;
    loop {
        let seen = ctx.process.children_wq.epoch();
        let children: Vec<Arc<Process>> = ctx
            .kernel
            .procs
            .lock()
            .values()
            .filter(|p| p.ppid.load(Ordering::Relaxed) == me)
            .filter(|p| match pid {
                -1 => true,
                0 => p.pgid.load(Ordering::Relaxed) == ctx.process.pgid.load(Ordering::Relaxed),
                x if x < -1 => p.pgid.load(Ordering::Relaxed) == -x,
                x => p.pid == x,
            })
            .cloned()
            .collect();
        if children.is_empty() {
            return ECHILD;
        }
        for c in &children {
            let done = c.threads.lock().is_empty();
            if let (true, Some(st)) = (done, *c.exit.lock()) {
                ctx.kernel.procs.lock().remove(&c.pid);
                if status != 0 {
                    ctx.mm.write_u32(status, st.wait_status() as u32);
                }
                return c.pid as i64;
            }
        }
        if options & WNOHANG != 0 {
            return 0;
        }
        if ctx.signal_pending() {
            return EINTR;
        }
        ctx.process
            .children_wq
            .wait_change(seen, Some(Instant::now() + Duration::from_millis(50)));
    }
}

fn sys_waitid(ctx: &Ctx, a: [u64; 6]) -> i64 {
    // waitid(idtype, id, infop, options, rusage)
    let pid = match a[0] {
        0 => -1,
        1 => a[1] as i32,
        2 => -(a[1] as i32),
        _ => return EINVAL,
    };
    let mut st = 0u32;
    let opts = if a[3] & 1 != 0 { 1 } else { 0 };
    let saddr = 0;
    let _ = saddr;
    // Use wait4 into a scratch value.
    let r = {
        let me = ctx.process.pid;
        let _ = me;
        let tmp = sys_wait4_status(ctx, pid, opts, &mut st);
        tmp
    };
    if r > 0 && a[2] != 0 {
        let mut info = [0u8; 128];
        info[0..4].copy_from_slice(&(SIGCHLD as i32).to_le_bytes());
        let (code, val) = if st & 0x7f == 0 {
            (1, (st >> 8) & 0xff)
        } else {
            (2, st & 0x7f)
        };
        info[8..12].copy_from_slice(&(code as i32).to_le_bytes());
        info[16..20].copy_from_slice(&(r as i32).to_le_bytes());
        info[24..28].copy_from_slice(&val.to_le_bytes());
        ctx.mm.write(a[2], &info);
    }
    if r > 0 {
        0
    } else {
        r
    }
}

fn sys_wait4_status(ctx: &Ctx, pid: i32, options: u32, out: &mut u32) -> i64 {
    // Same as wait4 but returns the status in `out`.
    let me = ctx.process.pid;
    loop {
        let seen = ctx.process.children_wq.epoch();
        let children: Vec<Arc<Process>> = ctx
            .kernel
            .procs
            .lock()
            .values()
            .filter(|p| {
                p.ppid.load(Ordering::Relaxed) == me
                    && (pid == -1
                        || p.pid == pid
                        || (pid < -1 && p.pgid.load(Ordering::Relaxed) == -pid))
            })
            .cloned()
            .collect();
        if children.is_empty() {
            return ECHILD;
        }
        for c in &children {
            if let (true, Some(st)) = (c.threads.lock().is_empty(), *c.exit.lock()) {
                ctx.kernel.procs.lock().remove(&c.pid);
                *out = st.wait_status() as u32;
                return c.pid as i64;
            }
        }
        if options & 1 != 0 {
            return 0;
        }
        if ctx.signal_pending() {
            return EINTR;
        }
        ctx.process
            .children_wq
            .wait_change(seen, Some(Instant::now() + Duration::from_millis(50)));
    }
}

fn sys_prctl(ctx: &Ctx, a: [u64; 6]) -> i64 {
    match a[0] {
        1 => {
            ctx.process
                .parent_death_signal
                .store(a[1] as u32, Ordering::Relaxed);
            0
        }
        2 => {
            ctx.mm.write_u32(
                a[1],
                ctx.process.parent_death_signal.load(Ordering::Relaxed),
            );
            0
        }
        15 => {
            // PR_SET_NAME
            let n = ctx.mm.read_cstr(a[1], 16).unwrap_or_default();
            let s: String = String::from_utf8_lossy(&n).chars().take(15).collect();
            *ctx.thread.name.lock() = s.clone();
            if ctx.thread.tid == ctx.process.pid {
                *ctx.process.comm.lock() = s;
            }
            0
        }
        16 => {
            let mut b = ctx.thread.name.lock().clone().into_bytes();
            b.truncate(15);
            b.resize(16, 0);
            ctx.mm.write(a[1], &b);
            0
        }
        0x53564d41 => 0, // PR_SET_VMA (anon names)
        3 => 1,          // PR_GET_DUMPABLE
        4 => 0,          // PR_SET_DUMPABLE
        23 => {
            // PR_CAPBSET_READ: caps 0..=40 exist (CAP_LAST_CAP = 40).
            if a[1] <= 40 {
                1
            } else {
                EINVAL
            }
        }
        8 | 36 | 38 | 39 | 40 | 22 | 24 | 47 | 57 | 59 | 0x59616d61 => 0,
        _ => 0,
    }
}

fn sys_prlimit(ctx: &Ctx, nr: u64, a: [u64; 6]) -> i64 {
    let (pid, res, new, old) = match nr {
        163 => (0, a[0], 0, a[1]),
        164 => (0, a[0], a[1], 0),
        _ => (a[0] as i32, a[1], a[2], a[3]),
    };
    let p = if pid == 0 {
        ctx.process.clone()
    } else {
        match ctx.kernel.procs.lock().get(&pid).cloned() {
            Some(p) => p,
            None => return ESRCH,
        }
    };
    if res >= 16 {
        return EINVAL;
    }
    let mut rl = p.rlimits.lock();
    if old != 0
        && !ctx.mm.write(
            old,
            &[
                rl[res as usize].0.to_le_bytes(),
                rl[res as usize].1.to_le_bytes(),
            ]
            .concat(),
        )
    {
        return EFAULT;
    }
    if new != 0 {
        let (Some(c), Some(m)) = (ctx.mm.read_u64(new), ctx.mm.read_u64(new + 8)) else {
            return EFAULT;
        };
        rl[res as usize] = (c, m);
    }
    0
}

fn sys_rt_sigaction(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let sig = a[0] as u32;
    if sig == 0 || sig > 64 {
        return EINVAL;
    }
    let acts = ctx.process.sigactions();
    let mut acts = acts.lock();
    let cur = acts[(sig - 1) as usize];
    if a[2] != 0 {
        let b = [cur.handler, cur.flags, cur.restorer, cur.mask];
        let bytes: Vec<u8> = b.iter().flat_map(|x| x.to_le_bytes()).collect();
        if !ctx.mm.write(a[2], &bytes) {
            return EFAULT;
        }
    }
    if a[1] != 0 {
        if sig == SIGKILL || sig == SIGSTOP {
            return EINVAL;
        }
        let mut b = [0u8; 32];
        if !ctx.mm.read(a[1], &mut b) {
            return EFAULT;
        }
        let g = |i: usize| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
        acts[(sig - 1) as usize] = SigAction {
            handler: g(0),
            flags: g(1),
            restorer: g(2),
            mask: g(3),
        };
    }
    0
}

fn sys_rt_sigprocmask(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let old = ctx.thread.sigmask.load(Ordering::Relaxed);
    if a[2] != 0 && !ctx.mm.write_u64(a[2], old) {
        return EFAULT;
    }
    if a[1] != 0 {
        let Some(set) = ctx.mm.read_u64(a[1]) else {
            return EFAULT;
        };
        let new = match a[0] {
            0 => old | set,
            1 => old & !set,
            2 => set,
            _ => return EINVAL,
        };
        let unblockable = (1u64 << (SIGKILL - 1)) | (1 << (SIGSTOP - 1));
        ctx.thread
            .sigmask
            .store(new & !unblockable, Ordering::Relaxed);
    }
    0
}

fn sys_sigtimedwait(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let Some(set) = ctx.mm.read_u64(a[0]) else {
        return EFAULT;
    };
    let deadline = if a[2] != 0 {
        match read_timespec(ctx, a[2]) {
            Ok(d) => Some(Instant::now() + d),
            Err(e) => return e,
        }
    } else {
        None
    };
    loop {
        // A dying group (exit_group, SIGKILL) must not keep a thread parked
        // here: Android's "Signal Catcher" sits in sigtimedwait forever.
        if ctx.process.dying.load(Ordering::SeqCst) {
            return EINTR;
        }
        // SIGKILL/SIGSTOP are never consumed by sigtimedwait.
        let blocked = !set | (1 << (SIGKILL - 1)) | (1 << 18);
        let got = ctx
            .thread
            .pending
            .lock()
            .take(blocked)
            .or_else(|| ctx.process.pending.lock().take(blocked));
        if let Some(info) = got {
            if a[1] != 0 {
                ctx.mm.write(a[1], &info.bytes);
            }
            return info.signo as i64;
        }
        if let Some(d) = deadline {
            if Instant::now() >= d {
                return EAGAIN;
            }
        }
        // Other (unblocked, not in set) signals interrupt.
        let mask = ctx.thread.sigmask.load(Ordering::Relaxed);
        let others = (ctx.thread.pending.lock().set | ctx.process.pending.lock().set)
            & (!mask & !set | (1 << (SIGKILL - 1)));
        if others != 0 {
            return EINTR;
        }
        let seen = ctx.thread.wake.epoch();
        let step = Instant::now() + Duration::from_millis(20);
        ctx.thread
            .wake
            .wait_change(seen, Some(deadline.map_or(step, |d| d.min(step))));
    }
}

fn sys_sigaltstack(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let cur = *ctx.thread.altstack.lock();
    if a[1] != 0 {
        let mut b = [0u8; 24];
        b[0..8].copy_from_slice(&cur.0.to_le_bytes());
        b[8..12].copy_from_slice(&cur.2.to_le_bytes());
        b[16..24].copy_from_slice(&cur.1.to_le_bytes());
        if !ctx.mm.write(a[1], &b) {
            return EFAULT;
        }
    }
    if a[0] != 0 {
        let mut b = [0u8; 24];
        if !ctx.mm.read(a[0], &mut b) {
            return EFAULT;
        }
        let sp = u64::from_le_bytes(b[0..8].try_into().unwrap());
        let flags = i32::from_le_bytes(b[8..12].try_into().unwrap());
        let size = u64::from_le_bytes(b[16..24].try_into().unwrap());
        *ctx.thread.altstack.lock() = if flags & 2 != 0 {
            (0, 0, 2)
        } else {
            (sp, size, 0)
        };
    }
    0
}

fn sys_kill(ctx: &Ctx, pid: i32, sig: u32) -> i64 {
    if sig > 64 {
        return EINVAL;
    }
    let targets: Vec<Arc<crate::task::Process>> = {
        let procs = ctx.kernel.procs.lock();
        match pid {
            p if p > 0 => procs.get(&p).cloned().into_iter().collect(),
            0 => procs
                .values()
                .filter(|p| {
                    p.pgid.load(Ordering::Relaxed) == ctx.process.pgid.load(Ordering::Relaxed)
                })
                .cloned()
                .collect(),
            -1 => procs
                .values()
                .filter(|p| p.pid != 1 && p.pid != ctx.process.pid)
                .cloned()
                .collect(),
            p => procs
                .values()
                .filter(|x| x.pgid.load(Ordering::Relaxed) == -p)
                .cloned()
                .collect(),
        }
    };
    let targets: Vec<_> = targets.into_iter().filter(|p| p.alive()).collect();
    if targets.is_empty() {
        return ESRCH;
    }
    if sig == 0 {
        return 0;
    }
    for p in targets {
        ctx.kernel.send_signal_process(
            &p,
            SigInfo::simple(
                sig,
                0,
                ctx.process.pid,
                ctx.process.uid.load(Ordering::Relaxed),
            ),
        );
    }
    0
}

fn sys_epoll_ctl(ctx: &Ctx, a: [u64; 6]) -> i64 {
    let r = (|| -> Result<i64> {
        let ep = file(ctx, a[0])?;
        let ep = ep.node.as_any().downcast_ref::<Epoll>().ok_or(EINVAL)?;
        let target = file(ctx, a[2])?;
        let fd = a[2] as i32;
        let mut items = ep.items.lock();
        match a[1] {
            1 | 3 => {
                let events = ctx.mm.read_u32(a[3]).ok_or(EFAULT)?;
                let data = ctx.mm.read_u64(a[3] + 8).ok_or(EFAULT)?;
                if a[1] == 1 && items.contains_key(&fd) {
                    return Err(EEXIST);
                }
                if a[1] == 3 && !items.contains_key(&fd) {
                    return Err(ENOENT);
                }
                items.insert(
                    fd,
                    EpollItem {
                        file: Arc::downgrade(&target),
                        events,
                        data,
                        last: 0,
                        disabled: false,
                    },
                );
            }
            2 => {
                items.remove(&fd).ok_or(ENOENT)?;
            }
            _ => return Err(EINVAL),
        }
        drop(items);
        ep.wq.notify();
        Ok(0)
    })();
    ret(r)
}

/// Sleeps on a set of wait queues until one changes or the deadline.
fn wait_any(
    ctx: &Ctx,
    queues: &[Arc<crate::sync::WaitQueue>],
    epochs: &[u64],
    deadline: Option<Instant>,
) {
    let step = Instant::now() + Duration::from_millis(10);
    let until = deadline.map_or(step, |d| d.min(step));
    // Poll the queues: with many queues, sleeping on the thread's wake queue
    // with a short timeout is simpler than multi-waiting.
    if let Some(q) = queues.first() {
        if queues.len() == 1 {
            q.wait_change(epochs[0], Some(until));
            return;
        }
    }
    let seen = ctx.thread.wake.epoch();
    for (q, e) in queues.iter().zip(epochs) {
        if q.epoch() != *e {
            return;
        }
    }
    ctx.thread.wake.wait_change(seen, Some(until));
}

fn sys_epoll_wait(ctx: &Ctx, nr: u64, a: [u64; 6]) -> i64 {
    let r = (|| -> Result<i64> {
        let epf = file(ctx, a[0])?;
        let ep = epf.node.as_any().downcast_ref::<Epoll>().ok_or(EINVAL)?;
        let max = (a[2] as i32).clamp(1, 1024) as usize;
        let deadline = if nr == 441 {
            if a[3] == 0 {
                None
            } else {
                Some(Instant::now() + read_timespec(ctx, a[3])?)
            }
        } else {
            let t = a[3] as i32;
            if t < 0 {
                None
            } else {
                Some(Instant::now() + Duration::from_millis(t as u64))
            }
        };
        // Temporary signal mask.
        let mask_addr = a[4];
        let old_mask = if mask_addr != 0 {
            let m = ctx.mm.read_u64(mask_addr).ok_or(EFAULT)?;
            Some(ctx.thread.sigmask.swap(m, Ordering::SeqCst))
        } else {
            None
        };
        let result = loop {
            let queues = ep.queues();
            let epochs: Vec<u64> = queues.iter().map(|q| q.epoch()).collect();
            let ep_epoch = ep.wq.epoch();
            let ev = ep.collect(max);
            if !ev.is_empty() {
                let mut b = Vec::with_capacity(ev.len() * 16);
                for (e, d) in &ev {
                    b.extend_from_slice(&e.to_le_bytes());
                    b.extend_from_slice(&[0u8; 4]);
                    b.extend_from_slice(&d.to_le_bytes());
                }
                if !ctx.mm.write(a[1], &b) {
                    break Err(EFAULT);
                }
                break Ok(ev.len() as i64);
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                break Ok(0);
            }
            if ctx.signal_pending() {
                break Err(EINTR);
            }
            let mut qs = queues;
            let mut es = epochs;
            qs.push(ep.wq.clone());
            es.push(ep_epoch);
            wait_any(ctx, &qs, &es, deadline);
        };
        if let Some(m) = old_mask {
            ctx.thread.sigmask.store(m, Ordering::SeqCst);
        }
        result
    })();
    ret(r)
}

fn sys_ppoll(ctx: &Ctx, nr: u64, a: [u64; 6]) -> i64 {
    let r = (|| -> Result<i64> {
        if nr == 72 {
            // pselect6: implement via polling the fd sets.
            let n = a[0] as usize;
            let words = n.div_ceil(64);
            let rd = |p: u64| -> Result<Vec<u64>> {
                if p == 0 {
                    return Ok(vec![0; words]);
                }
                (0..words)
                    .map(|i| ctx.mm.read_u64(p + i as u64 * 8).ok_or(EFAULT))
                    .collect()
            };
            let (r0, w0, e0) = (rd(a[1])?, rd(a[2])?, rd(a[3])?);
            let deadline = if a[4] != 0 {
                Some(Instant::now() + read_timespec(ctx, a[4])?)
            } else {
                None
            };
            loop {
                let mut count = 0;
                let (mut r1, mut w1, e1) =
                    (vec![0u64; words], vec![0u64; words], vec![0u64; words]);
                let mut queues = Vec::new();
                for fd in 0..n {
                    let bit = 1u64 << (fd % 64);
                    let (wr, ww) = (r0[fd / 64] & bit != 0, w0[fd / 64] & bit != 0);
                    if !wr && !ww && e0[fd / 64] & bit == 0 {
                        continue;
                    }
                    let f = file(ctx, fd as u64)?;
                    let ev = f.node.poll();
                    if let Some(q) = f.node.wait_queue() {
                        queues.push(q);
                    }
                    if wr && ev & (POLLIN | POLLHUP | POLLERR) != 0 {
                        r1[fd / 64] |= bit;
                        count += 1;
                    }
                    if ww && ev & (POLLOUT | POLLERR) != 0 {
                        w1[fd / 64] |= bit;
                        count += 1;
                    }
                }
                if count > 0 || deadline.is_some_and(|d| Instant::now() >= d) {
                    let wr = |p: u64, v: &[u64]| {
                        if p != 0 {
                            let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                            ctx.mm.write(p, &b);
                        }
                    };
                    wr(a[1], &r1);
                    wr(a[2], &w1);
                    wr(a[3], &e1);
                    return Ok(count);
                }
                if ctx.signal_pending() {
                    return Err(EINTR);
                }
                let epochs: Vec<u64> = queues.iter().map(|q| q.epoch()).collect();
                wait_any(ctx, &queues, &epochs, deadline);
            }
        }
        let n = a[1] as usize;
        if n > 4096 {
            return Err(EINVAL);
        }
        let mut b = vec![0u8; n * 8];
        if !ctx.mm.read(a[0], &mut b) {
            return Err(EFAULT);
        }
        let deadline = if a[2] != 0 {
            Some(Instant::now() + read_timespec(ctx, a[2])?)
        } else {
            None
        };
        let old_mask = if a[3] != 0 {
            let m = ctx.mm.read_u64(a[3]).ok_or(EFAULT)?;
            Some(ctx.thread.sigmask.swap(m, Ordering::SeqCst))
        } else {
            None
        };
        let res = loop {
            let mut count = 0;
            let mut queues = Vec::new();
            let fds = ctx.fds();
            for i in 0..n {
                let fd = i32::from_le_bytes(b[i * 8..i * 8 + 4].try_into().unwrap());
                let events = u16::from_le_bytes(b[i * 8 + 4..i * 8 + 6].try_into().unwrap()) as u32;
                let rev = if fd < 0 {
                    0
                } else {
                    match fds.lock().get(fd) {
                        Ok(f) => {
                            if let Some(q) = f.node.wait_queue() {
                                queues.push(q);
                            }
                            f.node.poll() & (events | POLLERR | POLLHUP)
                        }
                        Err(_) => 0x20, // POLLNVAL
                    }
                };
                b[i * 8 + 6..i * 8 + 8].copy_from_slice(&(rev as u16).to_le_bytes());
                if rev != 0 {
                    count += 1;
                }
            }
            if count > 0 || deadline.is_some_and(|d| Instant::now() >= d) {
                if !ctx.mm.write(a[0], &b) {
                    break Err(EFAULT);
                }
                break Ok(count);
            }
            if ctx.signal_pending() {
                break Err(EINTR);
            }
            let epochs: Vec<u64> = queues.iter().map(|q| q.epoch()).collect();
            wait_any(ctx, &queues, &epochs, deadline);
        };
        if let Some(m) = old_mask {
            ctx.thread.sigmask.store(m, Ordering::SeqCst);
        }
        res
    })();
    ret(r)
}

// ---------------------------------------------------------------------------
// memfd and timerfd nodes.

pub struct MemFd {
    pub mem: Arc<SharedMem>,
    pub size: Mutex<u64>,
    id: u64,
    pub name: String,
}

impl MemFd {
    pub fn new(name: &str) -> MemFd {
        MemFd {
            mem: SharedMem::new(0),
            size: Mutex::new(0),
            id: new_id(),
            name: name.to_string(),
        }
    }
}

impl Node for MemFd {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFREG | 0o600,
            ino: self.id,
            size: *self.size.lock() as i64,
            nlink: 1,
            blksize: 4096,
            ..Default::default()
        }
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        let size = *self.size.lock();
        if off >= size {
            return Ok(0);
        }
        let n = buf.len().min((size - off) as usize);
        let mut done = 0;
        while done < n {
            let a = off + done as u64;
            let p = self.mem.page(a / PAGE).ok_or(EIO)?;
            let o = (a % PAGE) as usize;
            let k = (PAGE as usize - o).min(n - done);
            buf[done..done + k].copy_from_slice(&p.bytes()[o..o + k]);
            done += k;
        }
        Ok(n)
    }
    fn write_at(&self, off: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        let mut done = 0;
        while done < buf.len() {
            let a = off + done as u64;
            let p = self.mem.page(a / PAGE).ok_or(EIO)?;
            let o = (a % PAGE) as usize;
            let k = (PAGE as usize - o).min(buf.len() - done);
            p.bytes_mut()[o..o + k].copy_from_slice(&buf[done..done + k]);
            done += k;
        }
        let mut s = self.size.lock();
        *s = (*s).max(off + buf.len() as u64);
        Ok(buf.len())
    }
    fn truncate(&self, len: u64) -> Result<()> {
        *self.size.lock() = len;
        self.mem.resize(len);
        Ok(())
    }
    fn shared_mem(&self) -> Option<Arc<SharedMem>> {
        Some(self.mem.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct TimerFd {
    state: Mutex<(Option<Instant>, Duration, u64)>, // next expiry, interval, expirations
    wq: Arc<crate::sync::WaitQueue>,
    id: u64,
    clock: u32,
}

impl TimerFd {
    pub fn new(clock: u32) -> Arc<TimerFd> {
        let t = Arc::new(TimerFd {
            state: Mutex::new((None, Duration::ZERO, 0)),
            wq: crate::sync::WaitQueue::new(),
            id: new_id(),
            clock,
        });
        // Ticker thread: wakes waiters when the timer expires.
        let weak = Arc::downgrade(&t);
        std::thread::Builder::new()
            .name("timerfd".into())
            .spawn(move || loop {
                let Some(t) = weak.upgrade() else { break };
                let next = t.state.lock().0;
                drop(t);
                let sleep = next
                    .map(|n| n.saturating_duration_since(Instant::now()))
                    .unwrap_or(Duration::from_millis(20));
                std::thread::sleep(sleep.min(Duration::from_millis(20)));
                let Some(t) = weak.upgrade() else { break };
                if t.update() > 0 {
                    t.wq.notify();
                }
            })
            .ok();
        t
    }
    fn update(&self) -> u64 {
        let mut s = self.state.lock();
        if let Some(n) = s.0 {
            let now = Instant::now();
            if now >= n {
                if s.1.is_zero() {
                    s.0 = None;
                    s.2 += 1;
                } else {
                    let missed = ((now - n).as_nanos() / s.1.as_nanos()) as u64 + 1;
                    s.2 += missed;
                    s.0 = Some(n + s.1 * missed as u32);
                }
            }
        }
        s.2
    }
    pub fn set(&self, interval: Duration, value: Duration, abs: bool) {
        let next = if value.is_zero() {
            None
        } else if abs {
            Some(crate::time::deadline(self.clock, value))
        } else {
            Some(Instant::now() + value)
        };
        *self.state.lock() = (next, interval, 0);
        self.wq.notify();
    }
    pub fn get(&self) -> (Duration, Duration) {
        let s = self.state.lock();
        (
            s.1,
            s.0.map(|n| n.saturating_duration_since(Instant::now()))
                .unwrap_or_default(),
        )
    }
}

impl Node for TimerFd {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: 0o600,
            ino: self.id,
            nlink: 1,
            ..Default::default()
        }
    }
    fn is_stream(&self) -> bool {
        true
    }
    fn read_at(&self, _o: u64, buf: &mut [u8], nonblock: bool) -> Result<usize> {
        if buf.len() < 8 {
            return Err(EINVAL);
        }
        loop {
            let seen = self.wq.epoch();
            let n = self.update();
            if n > 0 {
                self.state.lock().2 = 0;
                buf[..8].copy_from_slice(&n.to_le_bytes());
                return Ok(8);
            }
            if nonblock {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            self.wq
                .wait_change(seen, Some(Instant::now() + Duration::from_millis(20)));
        }
    }
    fn poll(&self) -> u32 {
        if self.update() > 0 {
            POLLIN
        } else {
            0
        }
    }
    fn wait_queue(&self) -> Option<Arc<crate::sync::WaitQueue>> {
        Some(self.wq.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn is_rw_host(f: &OpenFile) -> bool {
    f.node
        .as_any()
        .downcast_ref::<HostFile>()
        .is_some_and(|h| !h.path.starts_with(&*ROM_PREFIX.lock()))
}

/// Host path prefix of the read-only ROM (set by the kernel at startup).
pub static ROM_PREFIX: std::sync::LazyLock<Mutex<std::path::PathBuf>> =
    std::sync::LazyLock::new(|| Mutex::new(std::path::PathBuf::new()));

static SHARED_FILES: std::sync::LazyLock<
    Mutex<std::collections::HashMap<u64, std::sync::Weak<SharedMem>>>,
> = std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn shared_file_mem(f: &OpenFile) -> Arc<SharedMem> {
    let id = f.node.id();
    let mut m = SHARED_FILES.lock();
    if let Some(s) = m.get(&id).and_then(|w| w.upgrade()) {
        return s;
    }
    let size = f.node.stat().size.max(0) as u64;
    let mem = SharedMem::new(size);
    for i in 0..page_up(size) / PAGE {
        if let Some(p) = mem.page(i) {
            f.node.read_page(i * PAGE, p.bytes_mut());
        }
    }
    m.retain(|_, w| w.strong_count() > 0);
    m.insert(id, Arc::downgrade(&mem));
    mem
}

/// signalfd: reads dequeue pending signals in `mask` of the creating process
/// (any of its threads).
pub struct SignalFd {
    pub mask: Mutex<u64>,
    process: std::sync::Weak<Process>,
    id: u64,
}

impl SignalFd {
    fn take(&self) -> Option<SigInfo> {
        let p = self.process.upgrade()?;
        let mask = *self.mask.lock();
        let not_in_mask = !mask;
        if let Some(s) = p.pending.lock().take(not_in_mask) {
            return Some(s);
        }
        for t in p.threads.lock().iter() {
            if let Some(s) = t.pending.lock().take(not_in_mask) {
                return Some(s);
            }
        }
        None
    }
    fn ready(&self) -> bool {
        let Some(p) = self.process.upgrade() else {
            return false;
        };
        let mask = *self.mask.lock();
        p.pending.lock().set & mask != 0
            || p.threads
                .lock()
                .iter()
                .any(|t| t.pending.lock().set & mask != 0)
    }
}

impl Node for SignalFd {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: 0o600,
            ino: self.id,
            nlink: 1,
            ..Default::default()
        }
    }
    fn is_stream(&self) -> bool {
        true
    }
    fn read_at(&self, _o: u64, buf: &mut [u8], nonblock: bool) -> Result<usize> {
        if buf.len() < 128 {
            return Err(EINVAL);
        }
        let wq = self
            .process
            .upgrade()
            .map(|p| p.signal_wq.clone())
            .ok_or(EIO)?;
        loop {
            let seen = wq.epoch();
            let mut n = 0;
            while n + 128 <= buf.len() {
                let Some(info) = self.take() else { break };
                let out = &mut buf[n..n + 128];
                out.fill(0);
                out[0..4].copy_from_slice(&info.signo.to_le_bytes());
                out[8..12].copy_from_slice(&info.bytes[8..12]); // code
                out[12..16].copy_from_slice(&info.bytes[16..20]); // pid
                out[16..20].copy_from_slice(&info.bytes[20..24]); // uid
                out[44..48].copy_from_slice(&info.bytes[24..28]); // status
                n += 128;
            }
            if n > 0 {
                return Ok(n);
            }
            if nonblock {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            wq.wait_change(seen, Some(Instant::now() + Duration::from_millis(50)));
        }
    }
    fn poll(&self) -> u32 {
        if self.ready() {
            POLLIN
        } else {
            0
        }
    }
    fn wait_queue(&self) -> Option<Arc<crate::sync::WaitQueue>> {
        self.process.upgrade().map(|p| p.signal_wq.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn notify_path(g: &str, mask: u32) {
    if let Some(i) = g.rfind('/') {
        let dir = if i == 0 { "/" } else { &g[..i] };
        crate::input::notify(dir, &g[i + 1..], mask);
    }
}

fn sys_getxattr(ctx: &Ctx, nr: u64, a: [u64; 6]) -> i64 {
    let r = (|| -> Result<i64> {
        let (gpath, mode) = if nr == 10 {
            let f = file(ctx, a[0])?;
            (f.path.clone(), f.node.stat().mode)
        } else {
            let path = cstr(ctx, a[0])?;
            let (g, loc) = resolve(ctx, AT_FDCWD, &path, nr == 8)?;
            let st = stat_loc(ctx, &g, &loc)?;
            (g, st.mode)
        };
        let name = cstr(ctx, a[1])?;
        if name != "security.selinux" {
            return Err(ENODATA);
        }
        let mut label = if gpath.starts_with("/dev/socket/")
            || gpath.starts_with("/data")
            || gpath.starts_with("/dev")
        {
            // Runtime-created files: use file_contexts too (restorecon'd).
            crate::selinux::file_label(&ROM_PREFIX.lock(), &gpath, mode)
        } else {
            crate::selinux::file_label(&ROM_PREFIX.lock(), &gpath, mode)
        }
        .into_bytes();
        label.push(0);
        if a[3] == 0 {
            return Ok(label.len() as i64);
        }
        if (a[3] as usize) < label.len() {
            return Err(ERANGE);
        }
        if !ctx.mm.write(a[2], &label) {
            return Err(EFAULT);
        }
        Ok(label.len() as i64)
    })();
    ret(r)
}

/// POSIX timers (timer_create): a host thread delivers the signal.
pub struct PosixTimer {
    state: Mutex<(Option<Instant>, Duration)>,
    process: std::sync::Weak<Process>,
    tid: i32,
    signo: u32,
    notify: i32,
    kernel: std::sync::Weak<crate::Kernel>,
    wake: Arc<crate::sync::WaitQueue>,
}

static TIMERS: std::sync::LazyLock<Mutex<Vec<Option<Arc<PosixTimer>>>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

impl PosixTimer {
    fn new(ctx: &Ctx, signo: u32, notify: i32, tid: i32) -> Arc<PosixTimer> {
        let t = Arc::new(PosixTimer {
            state: Mutex::new((None, Duration::ZERO)),
            process: Arc::downgrade(&ctx.process),
            tid,
            signo,
            notify,
            kernel: Arc::downgrade(&ctx.kernel),
            wake: crate::sync::WaitQueue::new(),
        });
        let w = Arc::downgrade(&t);
        std::thread::Builder::new()
            .name("posix-timer".into())
            .spawn(move || loop {
                let Some(t) = w.upgrade() else { break };
                if t.process.upgrade().is_none_or(|p| !p.alive()) {
                    break;
                }
                let seen = t.wake.epoch();
                let next = t.state.lock().0;
                match next {
                    Some(n) if Instant::now() >= n => {
                        {
                            let mut s = t.state.lock();
                            s.0 = if s.1.is_zero() { None } else { Some(n + s.1) };
                        }
                        t.fire();
                    }
                    Some(n) => {
                        let wq = t.wake.clone();
                        drop(t);
                        wq.wait_change(
                            seen,
                            Some(n.min(Instant::now() + Duration::from_millis(100))),
                        );
                    }
                    None => {
                        let wq = t.wake.clone();
                        drop(t);
                        wq.wait_change(seen, Some(Instant::now() + Duration::from_millis(100)));
                    }
                }
            })
            .ok();
        t
    }
    fn fire(&self) {
        // SIGEV_NONE (1) delivers nothing; SIGEV_THREAD_ID (4) targets a thread.
        if self.notify == 1 {
            return;
        }
        let (Some(k), Some(p)) = (self.kernel.upgrade(), self.process.upgrade()) else {
            return;
        };
        let info = SigInfo::simple(self.signo, -2, 0, 0); // SI_TIMER
        if self.notify == 4 {
            if let Some(t) = k.threads.lock().get(&self.tid).cloned() {
                k.send_signal_thread(&t, info);
                return;
            }
        }
        k.send_signal_process(&p, info);
    }
    fn set(&self, interval: Duration, value: Duration, abs: bool) {
        let next = if value.is_zero() {
            None
        } else if abs {
            Some(crate::time::deadline(1, value))
        } else {
            Some(Instant::now() + value)
        };
        *self.state.lock() = (next, interval);
        self.wake.notify();
    }
    fn get(&self) -> (Duration, Duration) {
        let s = self.state.lock();
        (
            s.1,
            s.0.map(|n| n.saturating_duration_since(Instant::now()))
                .unwrap_or_default(),
        )
    }
}

/// process_vm_readv / process_vm_writev. ART's fault handler relies on
/// process_vm_readv (SafeCopy) to probe memory while handling SIGSEGV for
/// implicit null checks; without it every Java NullPointerException crashes.
fn sys_process_vm(ctx: &Ctx, write: bool, a: [u64; 6]) -> i64 {
    let (pid, liov, lcnt, riov, rcnt) = (a[0] as i32, a[1], a[2], a[3], a[4]);
    if a[5] != 0 || lcnt > 1024 || rcnt > 1024 {
        return EINVAL;
    }
    let remote = if pid == ctx.process.pid {
        ctx.mm.clone()
    } else {
        match ctx.kernel.procs.lock().get(&pid) {
            Some(p) => p.mm(),
            None => return ESRCH,
        }
    };
    let read_iov = |base: u64, n: u64| -> Option<Vec<(u64, u64)>> {
        (0..n)
            .map(|i| {
                Some((
                    ctx.mm.read_u64(base + i * 16)?,
                    ctx.mm.read_u64(base + i * 16 + 8)?,
                ))
            })
            .collect()
    };
    let (Some(local), Some(far)) = (read_iov(liov, lcnt), read_iov(riov, rcnt)) else {
        return EFAULT;
    };
    // Walk both vectors in page-sized chunks; stop at the first fault.
    let (mut li, mut lo, mut ri, mut ro) = (0usize, 0u64, 0usize, 0u64);
    let mut done: u64 = 0;
    let mut buf = vec![0u8; 4096];
    while li < local.len() && ri < far.len() {
        let (lb, ll) = local[li];
        let (rb, rl) = far[ri];
        if lo >= ll {
            li += 1;
            lo = 0;
            continue;
        }
        if ro >= rl {
            ri += 1;
            ro = 0;
            continue;
        }
        let raddr = rb + ro;
        let n = (ll - lo).min(rl - ro).min(4096 - (raddr & 4095)).min(4096) as usize;
        let ok = if write {
            ctx.mm.read(lb + lo, &mut buf[..n]) && remote.write(raddr, &buf[..n])
        } else {
            remote.read(raddr, &mut buf[..n]) && ctx.mm.write(lb + lo, &buf[..n])
        };
        if !ok {
            break;
        }
        done += n as u64;
        lo += n as u64;
        ro += n as u64;
    }
    if done == 0 && (local.iter().any(|x| x.1 > 0) && far.iter().any(|x| x.1 > 0)) {
        EFAULT
    } else {
        done as i64
    }
}
