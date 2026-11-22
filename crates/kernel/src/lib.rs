//! indroid neo kernel: the Linux user-space ABI implemented in Rust, so that
//! an unmodified AOSP aarch64 userspace can run on [`indroid_a64`] inside an
//! iOS app. See docs/NEO.md.
#![allow(
    clippy::type_complexity,
    clippy::manual_range_patterns,
    clippy::collapsible_if,
    clippy::needless_range_loop,
    clippy::manual_is_multiple_of,
    clippy::new_without_default,
    clippy::for_kv_map,
    clippy::while_let_loop,
    clippy::if_same_then_else,
    clippy::let_and_return,
    clippy::only_used_in_recursion,
    clippy::manual_range_contains,
    clippy::unnecessary_cast
)]

pub mod ashmem;
pub mod binder;
pub mod bpf;
pub mod elf;
pub mod errno;
pub mod fd;
pub mod futex;
pub mod goldfish;
pub mod input;
pub mod mm;
pub mod net;
pub mod procfs;
pub mod profile;
pub mod pty;
pub mod selinux;
pub mod sync;
pub mod syscall;
pub mod task;
pub mod time;
pub mod vfs;
pub mod virtgpu;

use errno::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use task::*;

/// HWCAP bits we advertise (must match what indroid-a64 implements).
pub const HWCAP: u64 = (1 << 0) // FP
    | (1 << 1) // ASIMD
    // AES/PMULL/SHA1/SHA2 (bits 3-6) are advertised once the crypto
    // extensions land in indroid-a64.
    | (1 << 7) // CRC32
    | (1 << 8) // ATOMICS
    | (1 << 9) // FPHP
    | (1 << 10) // ASIMDHP
    | (1 << 12) // ASIMDRDM
    | (1 << 15) // LRCPC
    | (1 << 20); // ASIMDDP
pub const HWCAP2: u64 = 0;

/// Console sink of the most recently created kernel, for diagnostics that
/// have no kernel reference (logcat mirroring).
static CONSOLE: parking_lot::Mutex<Option<Arc<dyn Fn(&[u8]) + Send + Sync>>> =
    parking_lot::Mutex::new(None);

pub fn console_line(s: &str) {
    match CONSOLE.lock().clone() {
        Some(c) => c(s.as_bytes()),
        None => eprint!("{s}"),
    }
}

pub struct Config {
    pub rom: std::path::PathBuf,
    pub state: std::path::PathBuf,
    /// Directories layered over the ROM (see vfs::Vfs::overlays).
    pub overlays: Vec<std::path::PathBuf>,
    /// Receives console output (guest stdout/stderr of the first process,
    /// /dev/kmsg, kernel diagnostics).
    pub console: Arc<dyn Fn(&[u8]) + Send + Sync>,
}

pub struct Kernel {
    gpu_backend: virtgpu::BackendSlot,
    pub vfs: vfs::Vfs,
    pub bpf: bpf::BpfState,
    pub procs: Mutex<HashMap<i32, Arc<Process>>>,
    pub threads: Mutex<HashMap<i32, Arc<Thread>>>,
    next_pid: AtomicI32,
    pub futexes: futex::Futexes,
    pub console: Arc<fd::Console>,
    pub trace_syscalls: AtomicBool,
    pub trace_faults: bool,
    pub hostname: Mutex<String>,
    /// Signalled when any process exits (init's last child, the embedder).
    pub exits: Arc<sync::WaitQueue>,
    pub live_threads: AtomicU32,
    pub insns: AtomicU64,
    pub touchscreen: Arc<input::InputDevice>,
    pub keys: Arc<input::InputDevice>,
    /// Sampling profile of guest code pages (INDROID_PROFILE=<out file>).
    pub profile: Option<profile::Profile>,
}

impl Kernel {
    pub fn new(cfg: Config) -> Arc<Kernel> {
        time::init();
        *syscall::ROM_PREFIX.lock() = cfg.rom.clone();
        *CONSOLE.lock() = Some(cfg.console.clone());
        let console = fd::Console::new(cfg.console.clone());
        let mut vfs = vfs::Vfs::new(&cfg.rom, &cfg.state);
        vfs.overlays = cfg.overlays.clone();
        selinux::init(&cfg.rom);
        vfs.add_mount(
            "/sys/fs/selinux",
            vfs::MountKind::Synthetic(Arc::new(selinux::SelinuxFs::new())),
        );
        vfs.add_mount("/dev/pts", vfs::MountKind::Synthetic(Arc::new(pty::PtsFs)));
        vfs.add_mount("/proc", vfs::MountKind::Synthetic(Arc::new(procfs::ProcFs)));
        let k = Arc::new(Kernel {
            gpu_backend: Arc::new(parking_lot::RwLock::new(None)),
            vfs,
            bpf: bpf::BpfState::default(),
            procs: Mutex::new(HashMap::new()),
            threads: Mutex::new(HashMap::new()),
            next_pid: AtomicI32::new(1),
            futexes: futex::Futexes::default(),
            console,
            trace_syscalls: AtomicBool::new(std::env::var_os("INDROID_STRACE").is_some()),
            trace_faults: std::env::var_os("INDROID_TRACE_FAULTS").is_some(),
            hostname: Mutex::new("localhost".into()),
            exits: sync::WaitQueue::new(),
            live_threads: AtomicU32::new(0),
            insns: AtomicU64::new(0),
            touchscreen: input::InputDevice::new(
                "indroid-touchscreen",
                input::DeviceKind::Touchscreen {
                    width: 1080,
                    height: 2340,
                },
            ),
            keys: input::InputDevice::new("indroid-keys", input::DeviceKind::Keys),
            profile: std::env::var_os("INDROID_PROFILE").map(|p| profile::Profile::new(p.into())),
        });
        k.install_devices();
        k.first_stage(&cfg.state);
        k
    }

    /// What first-stage init leaves behind for `init second_stage`
    /// (system/core/init/first_stage_init.cpp): /dev skeleton and mount points.
    fn first_stage(&self, state: &Path) {
        for (f, v) in [
            ("sys/devices/system/cpu/possible", "0\n"),
            ("sys/devices/system/cpu/online", "0\n"),
            ("sys/devices/system/cpu/present", "0\n"),
            ("sys/devices/system/cpu/cpu0/online", "1\n"),
            ("sys/power/state", "freeze mem\n"),
            ("sys/power/wakeup_count", "0\n"),
            ("sys/power/suspend_stats/success", "0\n"),
            ("sys/kernel/wakeup_reasons/last_resume_reason", ""),
            ("sys/kernel/wakeup_reasons/last_suspend_time", "0 0\n"),
            ("sys/kernel/tracing/tracing_on", "0\n"),
            ("sys/kernel/tracing/buffer_size_kb", "1408\n"),
            ("sys/kernel/tracing/current_tracer", "nop\n"),
            ("sys/kernel/tracing/trace_clock", "[local] global boot\n"),
            ("sys/kernel/tracing/events/enable", "0\n"),
        ] {
            let p = state.join(f);
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            let _ = std::fs::write(p, v);
        }
        for d in [
            "sys/class/wakeup",
            "sys/fs/fuse/connections",
            "sys/fs/bpf",
            "sys/kernel/debug/tracing",
            "dev/dri",
            "dev/socket",
            "dev/pts",
            "dev/dm-user",
            "dev/blkio",
            "mnt/vendor",
            "mnt/product",
            "sys/fs/selinux",
            "sys/fs/cgroup",
            "sys/kernel/debug",
            "sys/class",
            "sys/devices",
            "sys/block",
        ] {
            let _ = std::fs::create_dir_all(state.join(d));
        }
        let _ = std::fs::write(state.join("sys/fs/selinux/.indroid-disabled"), b"");
    }

    /// Install or remove the host GPU provider. Existing open nodes see changes.
    pub fn set_gpu_backend(&self, backend: Option<Arc<dyn virtgpu::GpuBackend>>) {
        *self.gpu_backend.write() = backend;
    }

    fn install_devices(self: &Arc<Self>) {
        use vfs::{DeviceFactory, S_IFCHR};
        let dev = |path: &str, f: DeviceFactory| self.vfs.add_device(path, S_IFCHR | 0o666, f);
        dev(
            "/dev/null",
            Arc::new(|| fd::NullDev::new(0) as vfs::FileRef),
        );
        dev(
            "/dev/zero",
            Arc::new(|| fd::NullDev::new(1) as vfs::FileRef),
        );
        dev(
            "/dev/random",
            Arc::new(|| fd::NullDev::new(2) as vfs::FileRef),
        );
        dev(
            "/dev/urandom",
            Arc::new(|| fd::NullDev::new(2) as vfs::FileRef),
        );
        dev(
            "/dev/full",
            Arc::new(|| fd::NullDev::new(3) as vfs::FileRef),
        );
        for path in ["/dev/binder", "/dev/hwbinder", "/dev/vndbinder"] {
            let context = binder::BinderContext::new();
            dev(
                path,
                Arc::new(move || binder::Binder::new(context.clone()) as vfs::FileRef),
            );
        }
        let ts = self.touchscreen.clone();
        self.vfs.add_device(
            "/dev/input/event0",
            S_IFCHR | 0o660,
            Arc::new(move || ts.open() as vfs::FileRef),
        );
        let keys = self.keys.clone();
        self.vfs.add_device(
            "/dev/input/event1",
            S_IFCHR | 0o660,
            Arc::new(move || keys.open() as vfs::FileRef),
        );
        // Block devices named by the fstab (contents are host directories).
        for b in ["/dev/block/vdc", "/dev/block/vda", "/dev/block/vdb"] {
            self.vfs.add_device(
                b,
                vfs::S_IFBLK | 0o600,
                Arc::new(|| fd::NullDev::new(4) as vfs::FileRef),
            );
        }
        dev("/dev/ptmx", Arc::new(pty::open_master));
        dev("/dev/goldfish_pipe", Arc::new(goldfish::open));
        dev(
            "/dev/goldfish_address_space",
            Arc::new(virtgpu::GoldfishAddressSpace::open),
        );
        dev("/dev/goldfish_sync", Arc::new(virtgpu::GoldfishSync::open));
        for (path, minor) in [("/dev/dri/renderD128", 128), ("/dev/dri/card0", 0)] {
            let backend = self.gpu_backend.clone();
            dev(
                path,
                Arc::new(move || virtgpu::VirtGpu::open(backend.clone(), minor)),
            );
        }
        // tracing: writes to trace_marker are discarded.
        for p in [
            "/sys/kernel/tracing/trace_marker",
            "/sys/kernel/debug/tracing/trace_marker",
        ] {
            dev(p, Arc::new(|| fd::NullDev::new(0) as vfs::FileRef));
        }
        dev(
            "/dev/ashmem",
            Arc::new(|| ashmem::Ashmem::new() as vfs::FileRef),
        );
        let c = self.console.clone();
        dev("/dev/console", Arc::new(move || c.clone() as vfs::FileRef));
        let c = self.console.clone();
        dev("/dev/tty", Arc::new(move || c.clone() as vfs::FileRef));
        let c = self.console.clone();
        dev("/dev/kmsg", Arc::new(move || c.clone() as vfs::FileRef));
    }

    pub fn log(&self, msg: &str) {
        (self.console.sink)(format!("{msg}\n").as_bytes());
    }

    pub fn alloc_pid(&self) -> i32 {
        self.next_pid.fetch_add(1, Ordering::Relaxed)
    }

    /// Starts the first process (pid 1 if nothing else ran) executing
    /// `path` with `argv`/`envp`; stdio goes to the console.
    pub fn spawn(self: &Arc<Self>, path: &str, argv: &[&str], envp: &[&str]) -> vfs::Result<i32> {
        let pid = self.alloc_pid();
        let fds = fd::FdTable::new();
        let proc_ = Arc::new(Process {
            pid,
            ppid: AtomicI32::new(0),
            pgid: AtomicI32::new(pid),
            sid: AtomicI32::new(pid),
            mm: parking_lot::RwLock::new(Arc::new(mm::Mm::new())),
            fds: parking_lot::RwLock::new(Arc::new(Mutex::new(fds))),
            cwd: Mutex::new("/".into()),
            umask: AtomicU32::new(0o022),
            sigactions: parking_lot::RwLock::new(Arc::new(Mutex::new([SigAction::default(); 64]))),
            pending: Mutex::new(Pending::default()),
            threads: Mutex::new(Vec::new()),
            exit: Mutex::new(None),
            dying: AtomicBool::new(false),
            exe: Mutex::new(path.into()),
            cmdline: Mutex::new(Vec::new()),
            comm: Mutex::new(String::new()),
            uid: AtomicU32::new(0),
            gid: AtomicU32::new(0),
            groups: Mutex::new(Vec::new()),
            children_wq: sync::WaitQueue::new(),
            signal_wq: sync::WaitQueue::new(),
            vfork_done: Mutex::new(None),
            exit_signal: AtomicU32::new(SIGCHLD),
            parent_death_signal: AtomicU32::new(0),
            start_time: std::time::Instant::now(),
            rlimits: Mutex::new(default_rlimits()),
            name_prctl: Mutex::new(String::new()),
            secctx: Mutex::new("u:r:init:s0".into()),
            exec_secctx: Mutex::new(None),
        });
        {
            let fds = proc_.fds();
            let mut t = fds.lock();
            for n in 0..3 {
                t.insert_at(
                    n,
                    fd::OpenFile::new(self.console.clone(), fd::O_RDWR, "/dev/console"),
                    false,
                );
            }
        }
        let thread = Arc::new(Thread {
            tid: pid,
            process: parking_lot::RwLock::new(proc_.clone()),
            sigmask: AtomicU64::new(0),
            pending: Mutex::new(Pending::default()),
            clear_child_tid: AtomicU64::new(0),
            robust_list: AtomicU64::new(0),
            altstack: Mutex::new((0, 0, 2)),
            exited: AtomicBool::new(false),
            wake: sync::WaitQueue::new(),
            name: Mutex::new(String::new()),
            saved_mask: Mutex::new(None),
            fds: Mutex::new(None),
            snap: Default::default(),
        });
        proc_.threads.lock().push(thread.clone());
        self.procs.lock().insert(pid, proc_.clone());
        self.threads.lock().insert(pid, thread.clone());
        let ctx = Ctx {
            kernel: self.clone(),
            process: proc_.clone(),
            thread: thread.clone(),
            mm: proc_.mm(),
        };
        let mut cpu = indroid_a64::Cpu::new();
        let argv: Vec<Vec<u8>> = argv.iter().map(|s| s.as_bytes().to_vec()).collect();
        let envp: Vec<Vec<u8>> = envp.iter().map(|s| s.as_bytes().to_vec()).collect();
        syscall::do_execve(&ctx, &mut cpu, path, argv, envp)?;
        self.start_thread(thread, cpu);
        Ok(pid)
    }

    pub fn start_thread(self: &Arc<Self>, thread: Arc<Thread>, cpu: indroid_a64::Cpu) {
        let k = self.clone();
        self.live_threads.fetch_add(1, Ordering::SeqCst);
        std::thread::Builder::new()
            .name(format!("guest-{}", thread.tid))
            .stack_size(4 << 20)
            .spawn(move || {
                let t = thread.clone();
                let k2 = k.clone();
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    task::thread_main(k, thread, cpu)
                }));
                if r.is_err() {
                    // A kernel bug on this guest thread: fail loudly and take
                    // the guest process down instead of leaving a ghost thread.
                    let p = t.process();
                    k2.log(&format!(
                        "[kernel] BUG: host panic on tid {} pid {} ({}); killing process",
                        t.tid,
                        p.pid,
                        p.comm.lock()
                    ));
                    k2.exit_group(&p, task::ExitStatus::Signaled(task::SIGSEGV, true));
                    k2.thread_exited(&t);
                }
            })
            .expect("spawn host thread");
    }

    /// Called on the exiting thread's host thread.
    pub fn thread_exited(&self, thread: &Arc<Thread>) {
        thread.exited.store(true, Ordering::SeqCst);
        let p = thread.process();
        // CLONE_CHILD_CLEARTID: zero the tid word and futex-wake it.
        let ctid = thread.clear_child_tid.swap(0, Ordering::SeqCst);
        if ctid != 0 {
            let mm = p.mm();
            mm.write_u32(ctid, 0);
            self.futexes.wake_addr(Arc::as_ptr(&mm) as usize, ctid);
        }
        self.threads.lock().remove(&thread.tid);
        let last = {
            let mut ts = p.threads.lock();
            ts.retain(|t| !Arc::ptr_eq(t, thread));
            ts.is_empty()
        };
        if last {
            let status = p.exit.lock().unwrap_or(ExitStatus::Exited(0));
            self.process_exited(&p, status);
        }
        self.live_threads.fetch_sub(1, Ordering::SeqCst);
        self.exits.notify();
    }

    /// Marks the whole thread group as exiting; each thread stops at its next
    /// check. The process becomes a zombie when its last thread is gone.
    pub fn exit_group(&self, p: &Arc<Process>, status: ExitStatus) {
        {
            let mut e = p.exit.lock();
            if e.is_none() {
                *e = Some(status);
            }
        }
        p.dying.store(true, Ordering::SeqCst);
        for t in p.threads.lock().iter() {
            t.wake.notify();
        }
    }

    fn process_exited(&self, p: &Arc<Process>, status: ExitStatus) {
        {
            let mut e = p.exit.lock();
            if e.is_none() {
                *e = Some(status);
            }
        }
        // Close files now (pipes report EOF to readers).
        let fds = p.fds();
        fds.lock().fds.clear();
        if let Some(wq) = p.vfork_done.lock().take() {
            wq.notify();
        }
        // Reparent children to init (pid 1) or reap if nobody's there.
        let procs: Vec<Arc<Process>> = self.procs.lock().values().cloned().collect();
        for c in &procs {
            if c.ppid.load(Ordering::Relaxed) == p.pid {
                c.ppid.store(1, Ordering::Relaxed);
                let sig = c.parent_death_signal.load(Ordering::Relaxed);
                if sig != 0 {
                    self.send_signal_process(c, SigInfo::simple(sig, 0, p.pid, 0));
                }
            }
        }
        let ppid = p.ppid.load(Ordering::Relaxed);
        let parent = self.procs.lock().get(&ppid).cloned();
        match parent {
            Some(parent) => {
                let sig = p.exit_signal.load(Ordering::Relaxed);
                if sig != 0 {
                    let mut info = SigInfo::simple(
                        sig,
                        if matches!(status, ExitStatus::Exited(_)) {
                            1
                        } else {
                            2
                        },
                        p.pid,
                        0,
                    );
                    let st = match status {
                        ExitStatus::Exited(c) => c,
                        ExitStatus::Signaled(s, _) => s as i32,
                    };
                    info.bytes[24..28].copy_from_slice(&st.to_le_bytes());
                    self.send_signal_process(&parent, info);
                }
                parent.children_wq.notify();
                // Auto-reap if the parent ignores SIGCHLD (SA_NOCLDWAIT/SIG_IGN).
                let act = parent.sigactions().lock()[(SIGCHLD - 1) as usize];
                if act.handler == SIG_IGN || act.flags & 2 != 0 {
                    self.procs.lock().remove(&p.pid);
                }
            }
            None => {
                self.procs.lock().remove(&p.pid);
            }
        }
        if self.trace_syscalls.load(Ordering::Relaxed) || p.pid == 1 {
            self.log(&format!(
                "[kernel] pid {} ({}) exited: {:?}",
                p.pid,
                p.comm.lock(),
                status
            ));
        }
    }

    pub fn send_signal_process(&self, p: &Arc<Process>, info: SigInfo) {
        if !p.alive() {
            return;
        }
        if info.signo == SIGKILL {
            self.exit_group(p, ExitStatus::Signaled(SIGKILL, false));
            return;
        }
        // Prefer a thread that doesn't block it.
        let threads = p.threads.lock().clone();
        let bit = 1u64 << (info.signo - 1);
        if let Some(t) = threads
            .iter()
            .find(|t| t.sigmask.load(Ordering::Relaxed) & bit == 0)
        {
            t.pending.lock().push(info);
            t.wake.notify();
        } else {
            p.pending.lock().push(info);
        }
        p.signal_wq.notify();
    }

    pub fn send_signal_thread(&self, t: &Arc<Thread>, info: SigInfo) {
        if info.signo == SIGKILL {
            self.exit_group(&t.process(), ExitStatus::Signaled(SIGKILL, false));
            return;
        }
        t.pending.lock().push(info);
        t.wake.notify();
        t.process().signal_wq.notify();
    }

    /// Blocks until no guest threads are running (or `timeout`).
    pub fn wait_idle(&self, timeout: Option<std::time::Duration>) -> bool {
        let deadline = timeout.map(|t| std::time::Instant::now() + t);
        loop {
            let seen = self.exits.epoch();
            if self.live_threads.load(Ordering::SeqCst) == 0 {
                return true;
            }
            if !self.exits.wait_change(seen, deadline) {
                return false;
            }
        }
    }

    pub fn exit_status(&self, pid: i32) -> Option<ExitStatus> {
        self.procs.lock().get(&pid).and_then(|p| *p.exit.lock())
    }
}

pub fn default_rlimits() -> [(u64, u64); 16] {
    let inf = u64::MAX;
    let mut r = [(inf, inf); 16];
    r[3] = (8 << 20, inf); // RLIMIT_STACK
    r[4] = (0, inf); // RLIMIT_CORE
    r[7] = (32768, 32768); // RLIMIT_NOFILE
    r[8] = (64 << 10, 64 << 10); // RLIMIT_MEMLOCK
    r[13] = (40, 40); // RLIMIT_NICE
    r
}

pub fn path_exists(p: &Path) -> bool {
    p.exists()
}

#[allow(dead_code)]
fn unused() -> i64 {
    ENOSYS
}
