//! Processes, threads, the per-thread run loop and signal delivery.

use crate::fd::FdTable;
use crate::mm::Mm;
use crate::sync::WaitQueue;
use crate::Kernel;
use indroid_a64::{Cpu, Exit};
use parking_lot::{Mutex, RwLock};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub const SIGHUP: u32 = 1;
pub const SIGINT: u32 = 2;
pub const SIGQUIT: u32 = 3;
pub const SIGILL: u32 = 4;
pub const SIGTRAP: u32 = 5;
pub const SIGABRT: u32 = 6;
pub const SIGBUS: u32 = 7;
pub const SIGFPE: u32 = 8;
pub const SIGKILL: u32 = 9;
pub const SIGUSR1: u32 = 10;
pub const SIGSEGV: u32 = 11;
pub const SIGUSR2: u32 = 12;
pub const SIGPIPE: u32 = 13;
pub const SIGALRM: u32 = 14;
pub const SIGTERM: u32 = 15;
pub const SIGCHLD: u32 = 17;
pub const SIGCONT: u32 = 18;
pub const SIGSTOP: u32 = 19;
pub const SIGTSTP: u32 = 20;
pub const SIGURG: u32 = 23;
pub const SIGWINCH: u32 = 28;
pub const SIGSYS: u32 = 31;

pub const SA_SIGINFO: u64 = 4;
pub const SA_ONSTACK: u64 = 0x0800_0000;
pub const SA_RESTART: u64 = 0x1000_0000;
pub const SA_NODEFER: u64 = 0x4000_0000;
pub const SA_RESETHAND: u64 = 0x8000_0000;
pub const SA_RESTORER: u64 = 0x0400_0000;

pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;

#[derive(Clone, Copy, Default, Debug)]
pub struct SigAction {
    pub handler: u64,
    pub flags: u64,
    pub restorer: u64,
    pub mask: u64,
}

/// Queued signal with its siginfo (128 bytes, already laid out).
#[derive(Clone)]
pub struct SigInfo {
    pub signo: u32,
    pub code: i32,
    pub bytes: [u8; 128],
}

impl SigInfo {
    pub fn simple(signo: u32, code: i32, pid: i32, uid: u32) -> SigInfo {
        let mut b = [0u8; 128];
        b[0..4].copy_from_slice(&(signo as i32).to_le_bytes());
        b[8..12].copy_from_slice(&code.to_le_bytes());
        b[16..20].copy_from_slice(&pid.to_le_bytes());
        b[20..24].copy_from_slice(&uid.to_le_bytes());
        SigInfo {
            signo,
            code,
            bytes: b,
        }
    }
    pub fn fault(signo: u32, code: i32, addr: u64) -> SigInfo {
        let mut b = [0u8; 128];
        b[0..4].copy_from_slice(&(signo as i32).to_le_bytes());
        b[8..12].copy_from_slice(&code.to_le_bytes());
        b[16..24].copy_from_slice(&addr.to_le_bytes());
        SigInfo {
            signo,
            code,
            bytes: b,
        }
    }
}

#[derive(Default)]
pub struct Pending {
    pub set: u64,
    pub queue: VecDeque<SigInfo>,
}

impl Pending {
    pub fn push(&mut self, info: SigInfo) {
        let bit = 1u64 << (info.signo - 1);
        // Standard signals do not queue more than once; RT signals do.
        if info.signo < 32 && self.set & bit != 0 {
            return;
        }
        self.set |= bit;
        self.queue.push_back(info);
    }
    pub fn take(&mut self, blocked: u64) -> Option<SigInfo> {
        let deliverable = self.set & !blocked;
        if deliverable == 0 {
            return None;
        }
        // Lowest signal number first (SIGKILL/SIGSEGV etc. first).
        let signo = deliverable.trailing_zeros() + 1;
        let idx = self.queue.iter().position(|s| s.signo == signo)?;
        let info = self.queue.remove(idx).unwrap();
        if !self.queue.iter().any(|s| s.signo == signo) {
            self.set &= !(1u64 << (signo - 1));
        }
        Some(info)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    Exited(i32),
    Signaled(u32, bool),
}

impl ExitStatus {
    pub fn wait_status(self) -> i32 {
        match self {
            ExitStatus::Exited(c) => (c & 0xff) << 8,
            ExitStatus::Signaled(s, core) => s as i32 | if core { 0x80 } else { 0 },
        }
    }
}

pub struct Process {
    pub pid: i32,
    pub ppid: AtomicI32,
    pub pgid: AtomicI32,
    pub sid: AtomicI32,
    pub mm: RwLock<Arc<Mm>>,
    pub fds: RwLock<Arc<Mutex<FdTable>>>,
    pub cwd: Mutex<String>,
    pub umask: AtomicU32,
    pub sigactions: RwLock<Arc<Mutex<[SigAction; 64]>>>,
    pub pending: Mutex<Pending>,
    pub threads: Mutex<Vec<Arc<Thread>>>,
    pub exit: Mutex<Option<ExitStatus>>,
    /// Set once the whole group is exiting (exit_group, fatal signal, exec).
    pub dying: AtomicBool,
    pub exe: Mutex<String>,
    pub cmdline: Mutex<Vec<u8>>,
    pub comm: Mutex<String>,
    pub uid: AtomicU32,
    pub gid: AtomicU32,
    pub groups: Mutex<Vec<u32>>,
    pub children_wq: Arc<WaitQueue>,
    /// Notified whenever a signal is queued to this process or one of its
    /// threads (signalfd readiness).
    pub signal_wq: Arc<WaitQueue>,
    /// vfork parent waits on this until exec/exit.
    pub vfork_done: Mutex<Option<Arc<WaitQueue>>>,
    pub exit_signal: AtomicU32,
    pub parent_death_signal: AtomicU32,
    pub start_time: std::time::Instant,
    pub rlimits: Mutex<[(u64, u64); 16]>,
    pub name_prctl: Mutex<String>,
    /// SELinux context (/proc/pid/attr/current) and the one requested for
    /// the next execve (attr/exec).
    pub secctx: Mutex<String>,
    pub exec_secctx: Mutex<Option<String>>,
}

impl Process {
    pub fn pgid(&self) -> u32 {
        self.pgid.load(Ordering::Relaxed) as u32
    }
    pub fn mm(&self) -> Arc<Mm> {
        self.mm.read().clone()
    }
    pub fn fds(&self) -> Arc<Mutex<FdTable>> {
        self.fds.read().clone()
    }
    pub fn sigactions(&self) -> Arc<Mutex<[SigAction; 64]>> {
        self.sigactions.read().clone()
    }
    pub fn alive(&self) -> bool {
        self.exit.lock().is_none()
    }
}

pub struct Thread {
    pub tid: i32,
    pub process: RwLock<Arc<Process>>,
    pub sigmask: AtomicU64,
    pub pending: Mutex<Pending>,
    pub clear_child_tid: AtomicU64,
    pub robust_list: AtomicU64,
    pub altstack: Mutex<(u64, u64, i32)>, // sp, size, flags
    pub exited: AtomicBool,
    /// Woken on signal arrival (blocking syscalls also poll pending signals).
    pub wake: Arc<WaitQueue>,
    pub name: Mutex<String>,
    /// Saved mask to restore after a sigsuspend/ppoll-style temporary mask.
    pub saved_mask: Mutex<Option<u64>>,
    /// Private fd table of a thread created with CLONE_THREAD but without
    /// CLONE_FILES (bionic's debuggerd pseudothread relies on this).
    pub fds: Mutex<Option<Arc<Mutex<FdTable>>>>,
    /// Diagnostics snapshot taken at syscall entry: nr (u64::MAX while
    /// running guest code), x0, x1, pc, lr, fp.
    pub snap: [AtomicU64; 6],
}

impl Thread {
    pub fn process(&self) -> Arc<Process> {
        self.process.read().clone()
    }
}

/// Per-thread syscall context.
pub struct Ctx {
    pub kernel: Arc<Kernel>,
    pub process: Arc<Process>,
    pub thread: Arc<Thread>,
    pub mm: Arc<Mm>,
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Thread>>> = const { RefCell::new(None) };
}

/// True if the calling guest thread has a deliverable signal (used by
/// blocking operations to return EINTR).
/// Guest tid of the calling host thread, if it runs a guest thread.
pub fn current_tid() -> Option<i32> {
    CURRENT.with(|c| c.borrow().as_ref().map(|t| t.tid))
}

pub fn current_signal_pending() -> bool {
    CURRENT.with(|c| {
        c.borrow().as_ref().is_some_and(|t| {
            let blocked = t.sigmask.load(Ordering::Relaxed);
            let p = t.process();
            p.dying.load(Ordering::Relaxed)
                || t.pending.lock().set & !blocked != 0
                || p.pending.lock().set & !blocked != 0
        })
    })
}

/// Raises a signal at the calling thread (e.g. SIGPIPE from a write).
pub fn raise_current(sig: u32) {
    CURRENT.with(|c| {
        if let Some(t) = c.borrow().as_ref() {
            t.pending.lock().push(SigInfo::simple(sig, 0, 0, 0));
        }
    })
}

pub fn current_thread() -> Option<Arc<Thread>> {
    CURRENT.with(|c| c.borrow().clone())
}

// ---------------------------------------------------------------------------
// Signal frames (arm64 rt_sigframe layout, see arch/arm64/kernel/signal.c).

const UC_OFF: u64 = 128; // after siginfo
const UC_STACK: u64 = 16;
const UC_SIGMASK: u64 = 40;
const UC_MCONTEXT: u64 = 176;
const SC_REGS: u64 = 8;
const SC_SP: u64 = 256;
const SC_PC: u64 = 264;
const SC_PSTATE: u64 = 272;
const SC_RESERVED: u64 = 288;
const FPSIMD_MAGIC: u32 = 0x4650_8001;
const FPSIMD_SIZE: u32 = 528;
const FRAME_SIZE: u64 = UC_OFF + UC_MCONTEXT + SC_RESERVED + 4096;

impl Ctx {
    /// The calling thread's file descriptor table.
    pub fn fds(&self) -> Arc<Mutex<FdTable>> {
        if let Some(t) = self.thread.fds.lock().as_ref() {
            return t.clone();
        }
        self.process.fds()
    }

    /// Synchronous fault signal (Linux force_sig_info): if it is blocked or
    /// ignored, reset it to the default action and unblock it, so a fault
    /// inside its own handler (stack overflow into a guard page) kills the
    /// process instead of re-faulting forever.
    pub fn force_sig(&self, info: SigInfo) {
        let sig = info.signo;
        let bit = 1u64 << (sig - 1);
        let blocked = self.thread.sigmask.load(Ordering::Relaxed) & bit != 0;
        {
            let actions = self.process.sigactions();
            let mut a = actions.lock();
            let act = &mut a[(sig - 1) as usize];
            if blocked || act.handler == SIG_IGN {
                act.handler = SIG_DFL;
            }
        }
        if blocked {
            self.thread.sigmask.fetch_and(!bit, Ordering::Relaxed);
        }
        self.thread.pending.lock().push(info);
    }

    pub fn signal_pending(&self) -> bool {
        let blocked = self.thread.sigmask.load(Ordering::Relaxed);
        self.process.dying.load(Ordering::Relaxed)
            || self.thread.pending.lock().set & !blocked != 0
            || self.process.pending.lock().set & !blocked != 0
    }

    fn take_signal(&self) -> Option<SigInfo> {
        let blocked = self.thread.sigmask.load(Ordering::Relaxed);
        // SIGKILL/SIGSTOP can't be blocked.
        let blocked = blocked & !((1 << (SIGKILL - 1)) | (1 << (SIGSTOP - 1)));
        if let Some(s) = self.thread.pending.lock().take(blocked) {
            return Some(s);
        }
        self.process.pending.lock().take(blocked)
    }

    /// Delivers pending signals before returning to guest code. Returns
    /// false if the thread must stop running (process killed).
    pub fn deliver_signals(&self, cpu: &mut Cpu) -> bool {
        while let Some(info) = self.take_signal() {
            let sig = info.signo;
            let act = self.process.sigactions().lock()[(sig - 1) as usize];
            if sig == SIGKILL || act.handler == SIG_DFL {
                match sig {
                    SIGCHLD | SIGURG | SIGWINCH | SIGCONT => continue,
                    // bionic-reserved RT signals (libbacktrace, debuggerd,
                    // heapprofd...): senders install a handler, signal, and
                    // restore it after a timeout. On a slow interpreted CPU
                    // the signal can land after the restore; ignore it
                    // rather than killing the process (zygote).
                    33..=40 if sig != SIGKILL => continue,
                    SIGSTOP | SIGTSTP | 21 | 22 => continue, // stopping not supported
                    _ => {
                        let core = matches!(
                            sig,
                            SIGQUIT
                                | SIGILL
                                | SIGTRAP
                                | SIGABRT
                                | SIGBUS
                                | SIGFPE
                                | SIGSEGV
                                | SIGSYS
                        );
                        if core || sig != SIGKILL {
                            self.kernel.log(&format!(
                                "[kernel] pid {} ({}) killed by signal {} at pc={:#x}",
                                self.process.pid,
                                self.process.comm.lock(),
                                sig,
                                cpu.pc
                            ));
                        }
                        self.kernel
                            .exit_group(&self.process, ExitStatus::Signaled(sig, core));
                        return false;
                    }
                }
            }
            if act.handler == SIG_IGN {
                continue;
            }
            if self.kernel.trace_syscalls.load(Ordering::Relaxed)
                && crate::syscall::strace_match(self)
            {
                self.kernel.log(&format!(
                    "[strace] {} {}: deliver signal {} code {} -> handler {:#x} flags {:#x} pc={}",
                    self.process.pid,
                    self.thread.tid,
                    sig,
                    info.code,
                    act.handler,
                    act.flags,
                    locate(&self.mm, cpu.pc)
                ));
            }
            if !self.setup_frame(cpu, &info, &act) {
                self.kernel.log(&format!(
                    "[kernel] pid {}: cannot build signal frame, killing",
                    self.process.pid
                ));
                self.kernel
                    .exit_group(&self.process, ExitStatus::Signaled(SIGSEGV, true));
                return false;
            }
            if act.flags & SA_RESETHAND != 0 {
                self.process.sigactions().lock()[(sig - 1) as usize] = SigAction::default();
            }
            // One handler frame at a time keeps things simple; the next
            // pending signal is delivered when this handler returns.
            break;
        }
        !self.process.dying.load(Ordering::Relaxed)
    }

    fn setup_frame(&self, cpu: &mut Cpu, info: &SigInfo, act: &SigAction) -> bool {
        let mut sp = cpu.sp;
        let (ss_sp, ss_size, ss_flags) = *self.thread.altstack.lock();
        let on_alt = ss_size != 0 && sp > ss_sp && sp <= ss_sp + ss_size;
        if act.flags & SA_ONSTACK != 0 && ss_size != 0 && ss_flags & 2 == 0 && !on_alt {
            sp = ss_sp + ss_size;
        }
        // frame record (fp, lr) above the sigframe, 16-byte aligned.
        sp = (sp - 16) & !15;
        let record = sp;
        sp = (sp - FRAME_SIZE) & !15;
        let frame = sp;
        let mm = &self.mm;
        let mut buf = vec![0u8; FRAME_SIZE as usize];
        buf[..128].copy_from_slice(&info.bytes);
        let uc = UC_OFF as usize;
        let put =
            |b: &mut [u8], off: usize, v: u64| b[off..off + 8].copy_from_slice(&v.to_le_bytes());
        // uc_stack
        put(&mut buf, uc + UC_STACK as usize, ss_sp);
        buf[uc + UC_STACK as usize + 8..uc + UC_STACK as usize + 12].copy_from_slice(
            &(if on_alt {
                1i32
            } else if ss_size == 0 {
                2
            } else {
                0
            })
            .to_le_bytes(),
        );
        put(&mut buf, uc + UC_STACK as usize + 16, ss_size);
        put(
            &mut buf,
            uc + UC_SIGMASK as usize,
            self.thread.sigmask.load(Ordering::Relaxed),
        );
        let sc = uc + UC_MCONTEXT as usize;
        let fault_addr = if matches!(info.signo, SIGSEGV | SIGBUS | SIGILL | SIGFPE | SIGTRAP) {
            u64::from_le_bytes(info.bytes[16..24].try_into().unwrap())
        } else {
            0
        };
        put(&mut buf, sc, fault_addr);
        for i in 0..31 {
            put(&mut buf, sc + SC_REGS as usize + i * 8, cpu.x[i]);
        }
        put(&mut buf, sc + SC_SP as usize, cpu.sp);
        put(&mut buf, sc + SC_PC as usize, cpu.pc);
        put(&mut buf, sc + SC_PSTATE as usize, cpu.nzcv & 0xf000_0000);
        let fp = sc + SC_RESERVED as usize;
        buf[fp..fp + 4].copy_from_slice(&FPSIMD_MAGIC.to_le_bytes());
        buf[fp + 4..fp + 8].copy_from_slice(&FPSIMD_SIZE.to_le_bytes());
        buf[fp + 8..fp + 12].copy_from_slice(&(cpu.fpsr as u32).to_le_bytes());
        buf[fp + 12..fp + 16].copy_from_slice(&(cpu.fpcr as u32).to_le_bytes());
        for i in 0..32 {
            buf[fp + 16 + i * 16..fp + 32 + i * 16].copy_from_slice(&cpu.v[i].to_le_bytes());
        }
        // Terminator {magic 0, size 0} follows (already zero).
        if !mm.write(frame, &buf) {
            return false;
        }
        if !mm.write(
            record,
            &[cpu.x[29].to_le_bytes(), cpu.x[30].to_le_bytes()].concat(),
        ) {
            return false;
        }
        // Block signals during the handler.
        let mut mask = self.thread.sigmask.load(Ordering::Relaxed) | act.mask;
        if act.flags & SA_NODEFER == 0 {
            mask |= 1 << (info.signo - 1);
        }
        self.thread.sigmask.store(mask, Ordering::Relaxed);
        cpu.x[0] = info.signo as u64;
        cpu.x[1] = frame;
        cpu.x[2] = frame + UC_OFF;
        cpu.x[29] = record;
        cpu.x[30] = if act.flags & SA_RESTORER != 0 {
            act.restorer
        } else {
            crate::elf::SIGRETURN_TRAMPOLINE
        };
        cpu.sp = frame;
        cpu.pc = act.handler;
        cpu.exclusive = None;
        true
    }

    /// rt_sigreturn: restores the context saved by setup_frame (possibly
    /// modified by the handler, as ART's fault handler does).
    pub fn sigreturn(&self, cpu: &mut Cpu) -> bool {
        let frame = cpu.sp;
        let mut buf = vec![0u8; FRAME_SIZE as usize];
        if !self.mm.read(frame, &mut buf) {
            return false;
        }
        let get = |off: usize| u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        let uc = UC_OFF as usize;
        let mask = get(uc + UC_SIGMASK as usize) & !((1 << (SIGKILL - 1)) | (1 << (SIGSTOP - 1)));
        self.thread.sigmask.store(mask, Ordering::Relaxed);
        let sc = uc + UC_MCONTEXT as usize;
        for i in 0..31 {
            cpu.x[i] = get(sc + SC_REGS as usize + i * 8);
        }
        cpu.sp = get(sc + SC_SP as usize);
        cpu.pc = get(sc + SC_PC as usize);
        cpu.nzcv = get(sc + SC_PSTATE as usize) & 0xf000_0000;
        // Walk the reserved area for the FPSIMD record.
        let mut off = sc + SC_RESERVED as usize;
        let end = off + 4096;
        while off + 8 <= end {
            let magic = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
            let size = u32::from_le_bytes(buf[off + 4..off + 8].try_into().unwrap()) as usize;
            if magic == 0 || size == 0 {
                break;
            }
            if magic == FPSIMD_MAGIC && off + 528 <= end {
                cpu.fpsr = u32::from_le_bytes(buf[off + 8..off + 12].try_into().unwrap()) as u64;
                cpu.fpcr = u32::from_le_bytes(buf[off + 12..off + 16].try_into().unwrap()) as u64;
                for i in 0..32 {
                    cpu.v[i] = u128::from_le_bytes(
                        buf[off + 16 + i * 16..off + 32 + i * 16]
                            .try_into()
                            .unwrap(),
                    );
                }
            }
            off += size;
        }
        cpu.exclusive = None;
        true
    }
}

// ---------------------------------------------------------------------------
// The run loop.

/// Instructions between checks for signals / address-space changes.
const SLICE: u64 = 100_000;
/// Slice length while profiling (more samples).
const PROFILE_SLICE: u64 = 5_000;

/// Runs a guest thread on the current host thread until it exits.
pub fn thread_main(kernel: Arc<Kernel>, thread: Arc<Thread>, mut cpu: Cpu) {
    CURRENT.with(|c| *c.borrow_mut() = Some(thread.clone()));
    loop {
        let process = thread.process();
        if process.dying.load(Ordering::Relaxed) || thread.exited.load(Ordering::Relaxed) {
            break;
        }
        let ctx = Ctx {
            kernel: kernel.clone(),
            mm: process.mm(),
            process,
            thread: thread.clone(),
        };
        if !ctx.deliver_signals(&mut cpu) {
            break;
        }
        // Run until the address space or process changes (exec), handling
        // exits inline.
        let mut slices = 0u32;
        let stop = loop {
            let slice = if kernel.profile.is_some() {
                PROFILE_SLICE
            } else {
                SLICE
            };
            match cpu.run(&*ctx.mm, slice) {
                Exit::Svc(_) => {
                    let snap = &ctx.thread.snap;
                    for (i, v) in [cpu.x[8], cpu.x[0], cpu.x[1], cpu.pc, cpu.x[30], cpu.x[29]]
                        .into_iter()
                        .enumerate()
                    {
                        snap[i].store(v, Ordering::Relaxed);
                    }
                    let flow = crate::syscall::dispatch(&ctx, &mut cpu);
                    snap[0].store(u64::MAX, Ordering::Relaxed);
                    match flow {
                        crate::syscall::Flow::Continue => {}
                        crate::syscall::Flow::Reload => break false,
                        crate::syscall::Flow::Exit => break true,
                    }
                    if ctx.signal_pending() {
                        break false;
                    }
                }
                Exit::Budget | Exit::Yield => {
                    if let Some(prof) = &kernel.profile {
                        prof.sample(&ctx.mm, cpu.pc);
                    }
                    slices += 1;
                    if ctx.signal_pending() || slices % 16 == 0 {
                        break false;
                    }
                }
                Exit::Fault { addr, access } => {
                    if kernel.trace_faults {
                        kernel.log(&format!(
                            "[kernel] pid {} tid {} fault {:?} at {addr:#x} pc={:#x}",
                            ctx.process.pid, thread.tid, access, cpu.pc
                        ));
                        kernel.log(&backtrace(&ctx.mm, &cpu));
                    }
                    // SEGV_MAPERR (1) or SEGV_ACCERR (2).
                    let code = if ctx.mm.vma_at(addr).is_some() { 2 } else { 1 };
                    ctx.force_sig(SigInfo::fault(SIGSEGV, code, addr));
                    break false;
                }
                Exit::Alignment { addr } => {
                    ctx.force_sig(SigInfo::fault(SIGBUS, 1, addr));
                    break false;
                }
                Exit::Brk(imm) => {
                    // brk #0x3e8 is how bionic/ART abort; report like Linux.
                    ctx.force_sig(SigInfo::fault(SIGTRAP, 1, cpu.pc));
                    let _ = imm;
                    break false;
                }
                Exit::Undefined(insn) => {
                    kernel.log(&format!(
                        "[kernel] pid {} ({}) undefined instruction {insn:08x} at pc={:#x}",
                        ctx.process.pid,
                        ctx.process.comm.lock(),
                        cpu.pc
                    ));
                    ctx.force_sig(SigInfo::fault(SIGILL, 1, cpu.pc));
                    break false;
                }
            }
        };
        if stop {
            break;
        }
    }
    kernel.insns.fetch_add(cpu.retired, Ordering::Relaxed);
    kernel.thread_exited(&thread);
    CURRENT.with(|c| *c.borrow_mut() = None);
}

/// Symbolic location "file+0xoff" of a guest address.
/// One line per guest thread plus a frame-pointer backtrace of where it
/// entered its current syscall (host SIGUSR1 in the CLI).
pub fn dump_threads(kernel: &crate::Kernel) -> String {
    let mut threads: Vec<_> = kernel.threads.lock().values().cloned().collect();
    threads.sort_by_key(|t| t.tid);
    let mut out = String::new();
    for t in threads {
        let p = t.process();
        let mm = p.mm.read().clone();
        let v: Vec<u64> = t.snap.iter().map(|a| a.load(Ordering::Relaxed)).collect();
        let state = if v[0] == u64::MAX {
            "running".to_string()
        } else {
            let table = t.fds.lock().clone().unwrap_or_else(|| p.fds.read().clone());
            let file = if matches!(v[0], 29 | 63 | 64 | 25 | 22) {
                table
                    .lock()
                    .get(v[1] as i32)
                    .map(|f| format!(" fd={}", f.path))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let mut epoll = String::new();
            if v[0] == 22 {
                if let Ok(f) = table.lock().get(v[1] as i32) {
                    if let Some(ep) = f.node.as_any().downcast_ref::<crate::fd::Epoll>() {
                        for (fd, it) in ep.items.lock().iter() {
                            let (path, ready) = match it.file.upgrade() {
                                Some(f) => (f.path.clone(), f.node.poll()),
                                None => ("<dead>".into(), 0),
                            };
                            epoll.push_str(&format!(
                                " [{fd} {path} ev={:#x} ready={ready:#x}{}]",
                                it.events,
                                if it.disabled { " disabled" } else { "" }
                            ));
                        }
                    }
                }
            }
            format!("syscall {} ({:#x}, {:#x}){file}{epoll}", v[0], v[1], v[2])
        };
        out.push_str(&format!(
            "tid {} pid {} {} [{}]: {}\n    pc {}\n    lr {}\n",
            t.tid,
            p.pid,
            p.comm.lock(),
            t.name.lock(),
            state,
            locate(&mm, v[3]),
            locate(&mm, v[4])
        ));
        let mut fp = v[5] & 0x00ff_ffff_ffff_ffff;
        for _ in 0..12 {
            if fp == 0 || fp & 7 != 0 {
                break;
            }
            let (Some(next), Some(ret)) = (mm.read_u64(fp), mm.read_u64(fp + 8)) else {
                break;
            };
            if ret == 0 {
                break;
            }
            out.push_str(&format!(
                "    {}\n",
                locate(&mm, ret & 0x00ff_ffff_ffff_ffff)
            ));
            if next <= fp {
                break;
            }
            fp = next;
        }
    }
    out
}

pub fn locate(mm: &Mm, pc: u64) -> String {
    match mm.vma_at(pc) {
        Some(v) => match &v.backing {
            crate::mm::Backing::File { offset, .. } => {
                format!("{}+{:#x}", v.name, offset + (pc - v.start))
            }
            _ => format!(
                "{}+{:#x}",
                if v.name.is_empty() { "[anon]" } else { &v.name },
                pc - v.start
            ),
        },
        None => format!("{pc:#x}"),
    }
}

/// Registers and a frame-pointer backtrace (x29 chain) for diagnostics.
pub fn backtrace(mm: &Mm, cpu: &Cpu) -> String {
    let mut s = String::new();
    for i in 0..31 {
        s.push_str(&format!(
            "x{i:<2}={:016x}{}",
            cpu.x[i],
            if i % 4 == 3 { "\n" } else { " " }
        ));
    }
    s.push_str(&format!(" sp={:016x}\n", cpu.sp));
    s.push_str(&format!("  #00 pc {}\n", locate(mm, cpu.pc)));
    s.push_str(&format!("  #01 lr {}\n", locate(mm, cpu.x[30])));
    let mut fp = cpu.x[29] & 0x00ff_ffff_ffff_ffff;
    for n in 2..24 {
        if fp == 0 || fp & 7 != 0 {
            break;
        }
        let (Some(next), Some(ret)) = (mm.read_u64(fp), mm.read_u64(fp + 8)) else {
            break;
        };
        if ret == 0 {
            break;
        }
        s.push_str(&format!(
            "  #{n:02} {}\n",
            locate(mm, ret & 0x00ff_ffff_ffff_ffff)
        ));
        if next <= fp {
            break;
        }
        fp = next;
    }
    s
}
