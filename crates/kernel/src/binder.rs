//! Android binder device. Each device name has an independent context.
//!
//! Driver state is isolated per Binder device context.
use crate::errno::*;
use crate::fd::OpenFile;
use crate::mm::SharedMem;
use crate::sync::WaitQueue;
use crate::task::Ctx;
use crate::vfs::{new_id, Node, Result, Stat, POLLIN, POLLOUT, S_IFCHR};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

const fn ioc(dir: u32, ty: u32, nr: u32, size: u32) -> u32 {
    (dir << 30) | (size << 16) | (ty << 8) | nr
}
const fn bc(n: u32, size: u32) -> u32 {
    ioc(if size == 0 { 0 } else { 1 }, b'c' as u32, n, size)
}
const fn br(n: u32, size: u32) -> u32 {
    ioc(if size == 0 { 0 } else { 2 }, b'r' as u32, n, size)
}
const WRITE_READ: u32 = ioc(3, b'b' as u32, 1, 48);
const SET_MAX_THREADS: u32 = ioc(1, b'b' as u32, 5, 4);
const SET_CONTEXT_MGR: u32 = ioc(1, b'b' as u32, 7, 4);
const SET_CONTEXT_MGR_EXT: u32 = ioc(1, b'b' as u32, 13, 24);
const THREAD_EXIT: u32 = ioc(1, b'b' as u32, 8, 4);
const VERSION: u32 = ioc(3, b'b' as u32, 9, 4);
const NOOP: u32 = br(12, 0);
const COMPLETE: u32 = br(6, 0);
const DEAD_REPLY: u32 = br(5, 0);
const FAILED_REPLY: u32 = br(17, 0);
const TYPE_BINDER: u32 = 0x73622a85;
const TYPE_WEAK_BINDER: u32 = 0x77622a85;
const TYPE_HANDLE: u32 = 0x73682a85;
const TYPE_WEAK_HANDLE: u32 = 0x77682a85;
const TYPE_FD: u32 = 0x66642a85;
const TYPE_PTR: u32 = 0x70742a85;
const TYPE_FDA: u32 = 0x66646185;

#[derive(Default)]
pub struct BinderContext {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    manager: Option<u64>,
    manager_node: Option<u64>,
    procs: HashMap<u64, Proc>,
    nodes: HashMap<u64, BinderNode>,
    next_node: u64,
    /// (proc, tid) of the thread whose transaction is being translated: like
    /// Linux, BR_INCREFS/BR_ACQUIRE for its own nodes go to that thread's
    /// todo so they are handled while its Parcel still pins the objects.
    ref_hint: Option<(u64, i32)>,
}
#[derive(Default)]
struct Proc {
    pid: i32,
    base: u64,
    size: u64,
    free: BTreeMap<u64, u64>,
    allocated: HashMap<u64, Allocation>,
    threads: HashMap<i32, Thread>,
    todo: VecDeque<Work>,
    max_threads: u32,
    spawn_requested: u32,
    handles: BTreeMap<u32, BinderRef>,
    wq: Option<Arc<WaitQueue>>,
}
struct Allocation {
    len: u64,
    async_node: Option<u64>,
    refs: Vec<(u32, bool)>,
    fd_array: Vec<i32>,
}
struct BinderNode {
    owner: u64,
    ptr: u64,
    cookie: u64,
    flags: u32,
    async_active: bool,
    async_todo: VecDeque<Work>,
}
struct BinderRef {
    node: u64,
    strong: u32,
    weak: u32,
    death: Option<u64>,
}
#[derive(Default)]
struct Thread {
    todo: VecDeque<Work>,
    stack: Vec<(u64, i32, bool)>,
    waiting_on: Option<u64>,
    looper: bool,
    waiting: bool,
}
// Transactions dominate the queues; boxing them would only add allocations.
#[allow(clippy::large_enum_variant)]
enum Work {
    Command(u32),
    Bytes(Vec<u8>),
    Transaction {
        data: Vec<u8>,
        offsets: Vec<u8>,
        header: [u8; 64],
        from: Option<(u64, i32, bool)>,
        reply: bool,
        prepared: Prepared,
        secctx: Option<Vec<u8>>,
        async_node: Option<u64>,
        refs: Vec<(u32, bool)>,
    },
}

#[derive(Default)]
struct Prepared {
    extra: Vec<u8>,
    // Destination offset in the complete buffer, target offset in `extra`.
    pointers: Vec<(usize, usize)>,
    fds: Vec<(usize, Arc<OpenFile>, bool)>,
}

pub struct Binder {
    id: u64,
    context: Arc<BinderContext>,
    mem: Arc<SharedMem>,
    wq: Arc<WaitQueue>,
}
static CONTEXTS: Mutex<Vec<std::sync::Weak<BinderContext>>> = Mutex::new(Vec::new());

impl BinderContext {
    pub fn new() -> Arc<Self> {
        let c = Arc::new(Self::default());
        CONTEXTS.lock().push(Arc::downgrade(&c));
        c
    }
}

/// Diagnostics: queued work and transaction stacks of every binder thread.
pub fn dump() -> String {
    let mut out = String::new();
    for (i, c) in CONTEXTS
        .lock()
        .iter()
        .filter_map(|w| w.upgrade())
        .enumerate()
    {
        let Some(s) = c.state.try_lock_for(std::time::Duration::from_millis(500)) else {
            out.push_str(&format!("binder context {i}: LOCKED\n"));
            continue;
        };
        out.push_str(&format!("binder context {i}:\n"));
        let pid_of = |id: &u64| s.procs.get(id).map_or(-1, |p| p.pid);
        let mut procs: Vec<_> = s.procs.iter().collect();
        procs.sort_by_key(|(_, p)| p.pid);
        for (_, p) in procs {
            let busy: Vec<_> = p
                .threads
                .iter()
                .filter(|(_, t)| {
                    !t.todo.is_empty() || !t.stack.is_empty() || t.waiting_on.is_some()
                })
                .collect();
            if p.todo.is_empty() && busy.is_empty() {
                continue;
            }
            out.push_str(&format!("  pid {} todo {}\n", p.pid, p.todo.len()));
            for (tid, t) in busy {
                let stack: Vec<String> = t
                    .stack
                    .iter()
                    .map(|(proc_id, tid, _)| format!("{}:{}", pid_of(proc_id), tid))
                    .collect();
                out.push_str(&format!(
                    "    tid {} todo {} waiting {} looper {} waiting_on {:?} stack [{}]\n",
                    tid,
                    t.todo.len(),
                    t.waiting,
                    t.looper,
                    t.waiting_on.map(|id| pid_of(&id)),
                    stack.join(" ")
                ));
            }
        }
    }
    out
}
impl Binder {
    pub fn new(context: Arc<BinderContext>) -> Arc<Self> {
        let id = new_id();
        let wq = WaitQueue::new();
        context.state.lock().procs.insert(
            id,
            Proc {
                wq: Some(wq.clone()),
                ..Proc::default()
            },
        );
        Arc::new(Self {
            id,
            context,
            mem: SharedMem::new(0),
            wq,
        })
    }
    fn bind(&self, ctx: &Ctx) -> Result<()> {
        let mut s = self.context.state.lock();
        let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
        if p.pid != 0 && p.pid != ctx.process.pid {
            return Err(EINVAL);
        }
        p.pid = ctx.process.pid;
        Ok(())
    }
    fn write_read(&self, ctx: &Ctx, arg: u64) -> Result<i64> {
        let mut hdr = [0u8; 48];
        if !ctx.mm.read(arg, &mut hdr) {
            return Err(EFAULT);
        }
        let ws = get64(&hdr, 0);
        let mut wc = get64(&hdr, 8);
        let wp = get64(&hdr, 16);
        let rs = get64(&hdr, 24);
        let mut rc = get64(&hdr, 32);
        let rp = get64(&hdr, 40);
        if wc > ws || rc > rs || ws > 16 * 1024 * 1024 || rs > 16 * 1024 * 1024 {
            return Err(EINVAL);
        }
        while wc < ws {
            let cmd = ctx
                .mm
                .read_u32(wp.checked_add(wc).ok_or(EFAULT)?)
                .ok_or(EFAULT)?;
            let size = ((cmd >> 16) & 0x3fff) as u64;
            if wc.checked_add(4 + size).is_none_or(|n| n > ws) {
                return Err(EINVAL);
            }
            let mut payload = vec![0; size as usize];
            if !ctx.mm.read(wp + wc + 4, &mut payload) {
                return Err(EFAULT);
            }
            self.command(ctx, cmd, &payload)?;
            wc += 4 + size;
            if !ctx.mm.write_u64(arg + 8, wc) {
                return Err(EFAULT);
            }
        }
        if rs == 0 {
            return Ok(0);
        }
        // ioctl lacks an OpenFile argument. Look up this open description before
        // taking the context lock; never acquire the fd table under that lock.
        let nonblock = ctx
            .process
            .fds()
            .lock()
            .fds
            .values()
            .any(|f| f.file.node.id() == self.id && f.file.nonblock());
        loop {
            let seen = self.wq.epoch();
            let work = {
                let mut s = self.context.state.lock();
                let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
                let t = p.threads.entry(ctx.thread.tid).or_default();
                // Like binder_available_for_proc_work: only an idle looper
                // thread (no transaction in flight either way) takes process
                // work. A client thread waiting for its own reply must not
                // pick up an unrelated incoming call and run it nested.
                let available = t.looper && t.waiting_on.is_none() && t.stack.is_empty();
                let (work, thread_work) = if let Some(work) = t.todo.pop_front() {
                    (Some(work), true)
                } else if available {
                    (p.todo.pop_front(), false)
                } else {
                    (None, false)
                };
                if let Some(work) = work {
                    let size = match &work {
                        Work::Command(_) => 4,
                        Work::Bytes(bytes) => bytes.len(),
                        Work::Transaction { secctx, .. } => {
                            if secctx.is_some() {
                                76
                            } else {
                                68
                            }
                        }
                    };
                    if size as u64 > rs - rc {
                        if thread_work {
                            p.threads
                                .entry(ctx.thread.tid)
                                .or_default()
                                .todo
                                .push_front(work);
                        } else {
                            p.todo.push_front(work);
                        }
                        return Err(ENOSPC);
                    }
                    Some(work)
                } else {
                    None
                }
            };
            if let Some(work) = work {
                let out = self.receive(ctx, work)?;
                if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                    eprintln!(
                        "binder pid={} tid={} read {:#x}",
                        ctx.process.pid,
                        ctx.thread.tid,
                        get32(&out, 0)
                    );
                }
                if out.len() as u64 > rs - rc {
                    return Err(ENOSPC);
                }
                if !ctx.mm.write(rp + rc, &out) {
                    return Err(EFAULT);
                }
                rc += out.len() as u64;
                if !ctx.mm.write_u64(arg + 32, rc) {
                    return Err(EFAULT);
                }
                return Ok(0);
            }
            if nonblock {
                if rs - rc >= 4 && ctx.mm.write_u32(rp + rc, NOOP) {
                    ctx.mm.write_u64(arg + 32, rc + 4);
                }
                return Ok(0);
            }
            if ctx.signal_pending() {
                return Err(EINTR);
            }
            {
                let mut s = self.context.state.lock();
                if let Some(p) = s.procs.get_mut(&self.id) {
                    p.threads.entry(ctx.thread.tid).or_default().waiting = true;
                }
            }
            self.wq
                .wait_change(seen, Some(Instant::now() + Duration::from_millis(50)));
            {
                let mut s = self.context.state.lock();
                if let Some(t) = s
                    .procs
                    .get_mut(&self.id)
                    .and_then(|p| p.threads.get_mut(&ctx.thread.tid))
                {
                    t.waiting = false;
                }
            }
        }
    }
    fn command(&self, ctx: &Ctx, cmd: u32, b: &[u8]) -> Result<()> {
        let tid = ctx.thread.tid;
        if cmd == bc(0, 64) || cmd == bc(1, 64) || cmd == bc(17, 72) || cmd == bc(18, 72) {
            let reply = cmd == bc(1, 64) || cmd == bc(18, 72);
            let data_len = get64(b, 32);
            let offsets_len = get64(b, 40);
            if data_len > 8 * 1024 * 1024
                || offsets_len > 1024 * 1024
                || !offsets_len.is_multiple_of(8)
            {
                return Err(EINVAL);
            }
            let mut data = vec![0; data_len as usize];
            let mut offsets = vec![0; offsets_len as usize];
            if !ctx.mm.read(get64(b, 48), &mut data) || !ctx.mm.read(get64(b, 56), &mut offsets) {
                return Err(EFAULT);
            }
            let sg_size = if cmd == bc(17, 72) || cmd == bc(18, 72) {
                get64(b, 64)
            } else {
                0
            };
            let prepared = match prepare_objects(ctx, &data, &offsets, sg_size) {
                Ok(prepared) => prepared,
                Err(error) => {
                    if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                        eprintln!("binder pid={} invalid objects: {}", ctx.process.pid, error);
                    }
                    let mut s = self.context.state.lock();
                    fail_transaction(&mut s, self.id, tid, reply, None);
                    drop(s);
                    self.wq.notify();
                    return Ok(());
                }
            };
            if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                eprintln!(
                    "binder pid={} tid={} {} handle={} code={} flags={:#x} data={} offsets={} sg={}",
                    ctx.process.pid,
                    tid,
                    if reply { "reply" } else { "call" },
                    get32(b, 0),
                    get32(b, 16),
                    get32(b, 20),
                    data_len,
                    offsets_len,
                    sg_size
                );
            }
            let sender_secctx = ctx.process.secctx.lock().as_bytes().to_vec();
            let mut header = [0u8; 64];
            header.copy_from_slice(&b[..64]);
            put32(&mut header, 24, ctx.process.pid as u32);
            put32(&mut header, 28, ctx.process.uid.load(Ordering::Relaxed));
            let mut s = self.context.state.lock();
            if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                eprintln!("binder pid={} acquired context", ctx.process.pid);
            }
            let target = if reply {
                s.procs
                    .get_mut(&self.id)
                    .and_then(|p| p.threads.get_mut(&tid))
                    .and_then(|t| t.stack.pop())
            } else if get32(b, 0) == 0 {
                s.manager.map(|id| {
                    (
                        id,
                        0,
                        s.manager_node
                            .and_then(|n| s.nodes.get(&n))
                            .is_some_and(|n| n.flags & 0x100 != 0),
                    )
                })
            } else {
                s.procs
                    .get(&self.id)
                    .and_then(|p| p.handles.get(&get32(b, 0)))
                    .filter(|r| r.strong > 0)
                    .and_then(|r| s.nodes.get(&r.node))
                    .map(|node| (node.owner, 0, node.flags & 0x100 != 0))
            };
            let target_node_id = if reply {
                None
            } else if get32(b, 0) == 0 {
                s.manager_node
            } else {
                s.procs
                    .get(&self.id)
                    .and_then(|p| p.handles.get(&get32(b, 0)))
                    .map(|r| r.node)
            };
            let target_node = target_node_id
                .and_then(|id| s.nodes.get(&id))
                .map(|n| (n.ptr, n.cookie, n.flags));
            if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                eprintln!("binder pid={} route {:?}", ctx.process.pid, target);
            }
            if let Some((to, mut target_tid, allow_fds)) =
                target.filter(|(id, _, _)| s.procs.contains_key(id))
            {
                if !reply {
                    if let Some((_, waiting_tid, _)) = s
                        .procs
                        .get(&self.id)
                        .and_then(|p| p.threads.get(&tid))
                        .and_then(|t| t.stack.iter().rev().find(|(proc_id, _, _)| *proc_id == to))
                    {
                        target_tid = *waiting_tid;
                    }
                }
                if !allow_fds && !prepared.fds.is_empty() {
                    fail_transaction(
                        &mut s,
                        self.id,
                        tid,
                        reply,
                        Some((to, target_tid, allow_fds)),
                    );
                    drop(s);
                    self.wq.notify();
                    return Ok(());
                }
                let oneway = get32(b, 20) & 1 != 0;
                let from = (!reply && !oneway).then_some((self.id, tid, get32(b, 20) & 0x10 != 0));
                let mut refs = Vec::new();
                s.ref_hint = Some((self.id, tid));
                let translated =
                    translate_objects(&mut s, self.id, to, &mut data, &offsets, &mut refs);
                s.ref_hint = None;
                if let Err(error) = translated {
                    for (handle, weak) in refs {
                        let _ = change_ref(&mut s, to, handle, bc(if weak { 7 } else { 6 }, 4));
                    }
                    if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                        eprintln!(
                            "binder pid={} object translation failed: {}",
                            ctx.process.pid, error
                        );
                    }
                    fail_transaction(
                        &mut s,
                        self.id,
                        tid,
                        reply,
                        Some((to, target_tid, allow_fds)),
                    );
                    drop(s);
                    self.wq.notify();
                    return Ok(());
                }
                if reply {
                    if let Some(thread) = s
                        .procs
                        .get_mut(&to)
                        .and_then(|p| p.threads.get_mut(&target_tid))
                    {
                        thread.waiting_on = None;
                    }
                } else if from.is_some() {
                    s.procs
                        .get_mut(&self.id)
                        .unwrap()
                        .threads
                        .entry(tid)
                        .or_default()
                        .waiting_on = Some(to);
                }
                let (ptr, cookie, flags) = target_node.unwrap_or((0, 0, 0));
                let secctx = if !reply && flags & 0x1000 != 0 {
                    let mut sec = sender_secctx;
                    sec.push(0);
                    Some(sec)
                } else {
                    None
                };
                put64(&mut header, 0, ptr);
                put64(&mut header, 8, cookie);
                let work = Work::Transaction {
                    data,
                    offsets,
                    header,
                    from,
                    reply,
                    prepared,
                    secctx,
                    async_node: if oneway && !reply {
                        target_node_id
                    } else {
                        None
                    },
                    refs,
                };
                let already_active = if oneway && !reply {
                    if let Some(node) = target_node_id.and_then(|id| s.nodes.get_mut(&id)) {
                        let active = node.async_active;
                        node.async_active = true;
                        active
                    } else {
                        false
                    }
                } else {
                    false
                };
                if already_active {
                    s.nodes
                        .get_mut(&target_node_id.unwrap())
                        .unwrap()
                        .async_todo
                        .push_back(work);
                } else {
                    let target_proc = s.procs.get_mut(&to).unwrap();
                    if target_tid == 0 {
                        target_proc.todo.push_back(work);
                    } else {
                        target_proc
                            .threads
                            .entry(target_tid)
                            .or_default()
                            .todo
                            .push_back(work);
                    }
                    if target_tid == 0
                        && target_proc.max_threads > 0
                        && target_proc.spawn_requested == 0
                    {
                        let looping =
                            target_proc.threads.values().filter(|t| t.looper).count() as u32;
                        let idle = target_proc.threads.values().any(|t| t.looper && t.waiting);
                        if !idle && looping < target_proc.max_threads {
                            target_proc.todo.push_front(Work::Command(br(13, 0)));
                            target_proc.spawn_requested = 1;
                        }
                    }
                    if let Some(wq) = &target_proc.wq {
                        wq.notify();
                    }
                }
                let p = s.procs.get_mut(&self.id).unwrap();
                p.threads
                    .entry(tid)
                    .or_default()
                    .todo
                    .push_back(Work::Command(COMPLETE));
            } else {
                if std::env::var_os("INDROID_BINDER_TRACE").is_some() {
                    eprintln!(
                        "binder pid={} dead target handle={}",
                        ctx.process.pid,
                        get32(b, 0)
                    );
                }
                let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
                p.threads
                    .entry(tid)
                    .or_default()
                    .todo
                    .push_back(Work::Command(DEAD_REPLY));
            }
            self.wq.notify();
        } else if cmd == bc(3, 8) {
            let addr = get64(b, 0);
            let mut s = self.context.state.lock();
            let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
            let off = addr.checked_sub(p.base).ok_or(EINVAL)?;
            let allocation = p.allocated.remove(&off).ok_or(EINVAL)?;
            let len = allocation.len;
            let mut start = off;
            let mut total = len;
            if let Some((&prev, &prev_len)) = p.free.range(..off).next_back() {
                if prev + prev_len == off {
                    p.free.remove(&prev);
                    start = prev;
                    total += prev_len;
                }
            }
            if let Some((&next, &next_len)) = p.free.range(off..).next() {
                if start + total == next {
                    p.free.remove(&next);
                    total += next_len;
                }
            }
            p.free.insert(start, total);
            for (handle, weak) in allocation.refs {
                let _ = change_ref(&mut s, self.id, handle, bc(if weak { 7 } else { 6 }, 4));
            }
            if let Some(id) = allocation.async_node {
                if let Some(node) = s.nodes.get_mut(&id) {
                    let next = node.async_todo.pop_front();
                    node.async_active = next.is_some();
                    if let Some(work) = next {
                        let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
                        p.todo.push_back(work);
                        self.wq.notify();
                    }
                }
            }
            drop(s);
            if !allocation.fd_array.is_empty() {
                let fds = ctx.fds();
                let mut table = fds.lock();
                for fd in allocation.fd_array {
                    let _ = table.close(fd);
                }
            }
        } else if [bc(4, 4), bc(5, 4), bc(6, 4), bc(7, 4)].contains(&cmd) {
            let mut s = self.context.state.lock();
            change_ref(&mut s, self.id, get32(b, 0), cmd)?;
        } else if cmd == bc(14, 12) || cmd == bc(15, 12) {
            let mut s = self.context.state.lock();
            let handle = get32(b, 0);
            let cookie = get64(b, 4);
            let node_id = s
                .procs
                .get(&self.id)
                .and_then(|p| p.handles.get(&handle))
                .map(|r| r.node)
                .ok_or(EINVAL)?;
            let dead = s
                .nodes
                .get(&node_id)
                .is_none_or(|node| !s.procs.contains_key(&node.owner));
            let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
            let reference = p.handles.get_mut(&handle).ok_or(EINVAL)?;
            if cmd == bc(14, 12) {
                if reference.death.is_some() {
                    return Err(EINVAL);
                }
                reference.death = Some(cookie);
                if dead {
                    p.todo.push_back(cookie_work(15, cookie));
                }
            } else {
                if reference.death != Some(cookie) {
                    return Err(EINVAL);
                }
                reference.death = None;
                p.threads
                    .entry(tid)
                    .or_default()
                    .todo
                    .push_back(cookie_work(16, cookie));
            }
            self.wq.notify();
        } else if [bc(11, 0), bc(12, 0), bc(13, 0)].contains(&cmd) {
            let mut s = self.context.state.lock();
            let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
            if cmd == bc(11, 0) {
                p.spawn_requested = p.spawn_requested.saturating_sub(1);
            }
            p.threads.entry(tid).or_default().looper = cmd != bc(13, 0);
        } else if [bc(8, 16), bc(9, 16), bc(16, 8)].contains(&cmd) {
            // Acknowledgements.
        } else {
            return Err(EINVAL);
        }
        Ok(())
    }
    fn receive(&self, ctx: &Ctx, work: Work) -> Result<Vec<u8>> {
        match work {
            Work::Command(cmd) => Ok(cmd.to_le_bytes().to_vec()),
            Work::Bytes(bytes) => Ok(bytes),
            Work::Transaction {
                data,
                offsets,
                mut header,
                from,
                reply,
                prepared,
                secctx,
                async_node,
                refs,
            } => {
                let data_aligned = (data.len() as u64 + 7) & !7;
                let extra_start = (data_aligned + offsets.len() as u64 + 7) & !7;
                let sec_start = (extra_start + prepared.extra.len() as u64 + 7) & !7;
                let len = (sec_start + secctx.as_ref().map_or(0, |s| s.len() as u64)).max(8);
                let (off, base) = {
                    let mut s = self.context.state.lock();
                    let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
                    let (&off, &size) = p
                        .free
                        .iter()
                        .filter(|(_, n)| **n >= len)
                        .min_by_key(|(_, n)| *n)
                        .ok_or(ENOMEM)?;
                    p.free.remove(&off);
                    if size > len {
                        p.free.insert(off + len, size - len);
                    }
                    p.allocated.insert(
                        off,
                        Allocation {
                            len,
                            async_node,
                            refs,
                            fd_array: Vec::new(),
                        },
                    );
                    if let Some(sender) = from {
                        p.threads
                            .entry(ctx.thread.tid)
                            .or_default()
                            .stack
                            .push(sender);
                    }
                    (off, p.base)
                };
                let mut combined = vec![0; len as usize];
                combined[..data.len()].copy_from_slice(&data);
                combined[data_aligned as usize..data_aligned as usize + offsets.len()]
                    .copy_from_slice(&offsets);
                combined[extra_start as usize..extra_start as usize + prepared.extra.len()]
                    .copy_from_slice(&prepared.extra);
                for (where_, target) in prepared.pointers {
                    put64(
                        &mut combined,
                        where_,
                        base + off + extra_start + target as u64,
                    );
                }
                let table = ctx.fds();
                let mut table = table.lock();
                let mut installed = Vec::new();
                let mut fd_array = Vec::new();
                for (where_, file, is_array) in prepared.fds {
                    match table.insert(file, true) {
                        Ok(fd) => {
                            installed.push(fd);
                            if is_array {
                                fd_array.push(fd);
                            }
                            put32(&mut combined, where_, fd as u32);
                        }
                        Err(error) => {
                            for fd in installed {
                                let _ = table.close(fd);
                            }
                            return Err(error);
                        }
                    }
                }
                drop(table);
                if !fd_array.is_empty() {
                    let mut s = self.context.state.lock();
                    if let Some(allocation) = s
                        .procs
                        .get_mut(&self.id)
                        .and_then(|p| p.allocated.get_mut(&off))
                    {
                        allocation.fd_array = fd_array;
                    }
                }
                if let Some(sec) = &secctx {
                    combined[sec_start as usize..sec_start as usize + sec.len()]
                        .copy_from_slice(sec);
                }
                self.write_shared(off, &combined);
                put64(&mut header, 48, base + off);
                put64(&mut header, 56, base + off + data_aligned);
                let mut out = br(
                    if reply { 3 } else { 2 },
                    if secctx.is_some() { 72 } else { 64 },
                )
                .to_le_bytes()
                .to_vec();
                out.extend_from_slice(&header);
                if secctx.is_some() {
                    out.extend_from_slice(&(base + off + sec_start).to_le_bytes());
                }
                Ok(out)
            }
        }
    }
    fn write_shared(&self, mut off: u64, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let page = self.mem.page(off / 4096).unwrap();
            let start = off as usize % 4096;
            let n = bytes.len().min(4096 - start);
            page.bytes_mut()[start..start + n].copy_from_slice(&bytes[..n]);
            off += n as u64;
            bytes = &bytes[n..];
        }
    }
}
// Snapshot scatter-gather bytes and open file descriptions before taking the
// context lock. Pointer and FD installation happens when the target reads.
fn prepare_objects(ctx: &Ctx, data: &[u8], offsets: &[u8], sg_size: u64) -> Result<Prepared> {
    if sg_size > 8 * 1024 * 1024 {
        return Err(EINVAL);
    }
    let extra_start = ((data.len() + 7) & !7) + offsets.len();
    let extra_start = (extra_start + 7) & !7;
    let mut prepared = Prepared::default();
    let mut parents: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut previous_end = 0;
    for (index, chunk) in offsets.as_chunks::<8>().0.iter().enumerate() {
        let off = u64::from_le_bytes(*chunk) as usize;
        if off < previous_end || off.checked_add(24).is_none_or(|end| end > data.len()) {
            return Err(EINVAL);
        }
        let typ = get32(data, off);
        let size = if typ == TYPE_PTR {
            40
        } else if typ == TYPE_FDA {
            32
        } else {
            24
        };
        if off.checked_add(size).is_none_or(|end| end > data.len()) {
            return Err(EINVAL);
        }
        previous_end = off + size;
        if typ == TYPE_PTR {
            let len = usize::try_from(get64(data, off + 16)).map_err(|_| EINVAL)?;
            let start = (prepared.extra.len() + 7) & !7;
            let end = start.checked_add(len).ok_or(EINVAL)?;
            if end > sg_size as usize {
                return Err(EINVAL);
            }
            prepared.extra.resize(end, 0);
            if !ctx
                .mm
                .read(get64(data, off + 8), &mut prepared.extra[start..end])
            {
                return Err(EFAULT);
            }
            parents.insert(index, (start, len));
            prepared.pointers.push((off + 8, start));
            if get32(data, off + 4) & 1 != 0 {
                let parent = usize::try_from(get64(data, off + 24)).map_err(|_| EINVAL)?;
                let parent_off = usize::try_from(get64(data, off + 32)).map_err(|_| EINVAL)?;
                let (base, parent_len) = *parents.get(&parent).ok_or(EINVAL)?;
                if parent >= index || parent_off.checked_add(8).is_none_or(|v| v > parent_len) {
                    return Err(EINVAL);
                }
                prepared
                    .pointers
                    .push((extra_start + base + parent_off, start));
            }
        } else if typ == TYPE_FD {
            let file = ctx.fds().lock().get(get32(data, off + 8) as i32)?;
            prepared.fds.push((off + 8, file, false));
        } else if typ == TYPE_FDA {
            let count = usize::try_from(get64(data, off + 8)).map_err(|_| EINVAL)?;
            let parent = usize::try_from(get64(data, off + 16)).map_err(|_| EINVAL)?;
            let parent_off = usize::try_from(get64(data, off + 24)).map_err(|_| EINVAL)?;
            let (base, len) = *parents.get(&parent).ok_or(EINVAL)?;
            if parent >= index
                || parent_off % 4 != 0
                || count
                    .checked_mul(4)
                    .and_then(|n| parent_off.checked_add(n))
                    .is_none_or(|end| end > len)
            {
                return Err(EINVAL);
            }
            let table = ctx.fds();
            let table = table.lock();
            for i in 0..count {
                let pos = base + parent_off + i * 4;
                prepared.fds.push((
                    extra_start + pos,
                    table.get(get32(&prepared.extra, pos) as i32)?,
                    true,
                ));
            }
        } else if ![TYPE_BINDER, TYPE_WEAK_BINDER, TYPE_HANDLE, TYPE_WEAK_HANDLE].contains(&typ) {
            return Err(EINVAL);
        }
    }
    Ok(prepared)
}
fn cookie_work(number: u32, cookie: u64) -> Work {
    let mut bytes = br(number, 8).to_le_bytes().to_vec();
    bytes.extend_from_slice(&cookie.to_le_bytes());
    Work::Bytes(bytes)
}
fn fail_transaction(
    s: &mut State,
    proc_id: u64,
    tid: i32,
    reply: bool,
    target: Option<(u64, i32, bool)>,
) {
    let target = if reply {
        target.or_else(|| {
            s.procs
                .get_mut(&proc_id)
                .and_then(|p| p.threads.get_mut(&tid))
                .and_then(|t| t.stack.pop())
        })
    } else {
        None
    };
    if let Some((to, to_tid, _)) = target {
        if let Some(p) = s.procs.get_mut(&to) {
            if let Some(t) = p.threads.get_mut(&to_tid) {
                t.waiting_on = None;
                t.todo.push_back(Work::Command(FAILED_REPLY));
            }
            if let Some(wq) = &p.wq {
                wq.notify();
            }
        }
    }
    if let Some(p) = s.procs.get_mut(&proc_id) {
        p.threads
            .entry(tid)
            .or_default()
            .todo
            .push_back(Work::Command(if reply { COMPLETE } else { FAILED_REPLY }));
        if let Some(wq) = &p.wq {
            wq.notify();
        }
    }
}
fn node_counts(s: &State, node: u64) -> (u64, u64) {
    s.procs
        .values()
        .flat_map(|p| p.handles.values())
        .filter(|r| r.node == node)
        .fold((0, 0), |(strong, total), r| {
            (
                strong + r.strong as u64,
                total + r.strong as u64 + r.weak as u64,
            )
        })
}
fn change_ref(s: &mut State, proc_id: u64, handle: u32, cmd: u32) -> Result<()> {
    if handle == 0 {
        return Ok(());
    }
    let node_id = s
        .procs
        .get(&proc_id)
        .and_then(|p| p.handles.get(&handle))
        .map(|r| r.node)
        .ok_or(EINVAL)?;
    let (old_strong, old_total) = node_counts(s, node_id);
    let reference = s
        .procs
        .get_mut(&proc_id)
        .and_then(|p| p.handles.get_mut(&handle))
        .ok_or(EINVAL)?;
    match cmd {
        c if c == bc(4, 4) => reference.weak = reference.weak.saturating_add(1),
        c if c == bc(5, 4) => reference.strong = reference.strong.saturating_add(1),
        c if c == bc(6, 4) => reference.strong = reference.strong.saturating_sub(1),
        _ => reference.weak = reference.weak.saturating_sub(1),
    }
    let remove = reference.strong == 0 && reference.weak == 0 && reference.death.is_none();
    if remove {
        s.procs.get_mut(&proc_id).unwrap().handles.remove(&handle);
    }
    notify_ref_change(s, node_id, (old_strong, old_total))?;
    Ok(())
}
fn notify_ref_change(s: &mut State, node_id: u64, old: (u64, u64)) -> Result<()> {
    let (new_strong, new_total) = node_counts(s, node_id);
    let node = s.nodes.get(&node_id).ok_or(EINVAL)?;
    let (owner, ptr, cookie) = (node.owner, node.ptr, node.cookie);
    for (needed, number) in [
        (old.1 == 0 && new_total > 0, 7),
        (old.0 == 0 && new_strong > 0, 8),
        (old.0 > 0 && new_strong == 0, 9),
        (old.1 > 0 && new_total == 0, 10),
    ] {
        if needed {
            let mut bytes = br(number, 16).to_le_bytes().to_vec();
            bytes.extend_from_slice(&ptr.to_le_bytes());
            bytes.extend_from_slice(&cookie.to_le_bytes());
            let hint = s
                .ref_hint
                .filter(|(p, _)| *p == owner && (number == 7 || number == 8));
            if let Some(owner) = s.procs.get_mut(&owner) {
                match hint {
                    Some((_, tid)) => owner
                        .threads
                        .entry(tid)
                        .or_default()
                        .todo
                        .push_back(Work::Bytes(bytes)),
                    None => owner.todo.push_back(Work::Bytes(bytes)),
                }
                if let Some(wq) = &owner.wq {
                    wq.notify();
                }
            }
        }
    }
    // A node nobody references any more is forgotten once its owner has been
    // told (BR_DECREFS); a later object at the same address is a new node.
    if old.1 > 0 && new_total == 0 && s.manager_node != Some(node_id) {
        if let Some(n) = s.nodes.get(&node_id) {
            if !n.async_active && n.async_todo.is_empty() {
                s.nodes.remove(&node_id);
            }
        }
    }
    Ok(())
}
// Rewrite object identities while holding the context lock. Offsets must be
// ordered and may only name complete, non-overlapping objects.
fn translate_objects(
    s: &mut State,
    from: u64,
    to: u64,
    data: &mut [u8],
    offsets: &[u8],
    refs: &mut Vec<(u32, bool)>,
) -> Result<()> {
    let mut previous_end = 0usize;
    for chunk in offsets.as_chunks::<8>().0 {
        let offset = u64::from_le_bytes(*chunk) as usize;
        if offset < previous_end || offset.checked_add(24).is_none_or(|end| end > data.len()) {
            return Err(EINVAL);
        }
        let typ = get32(data, offset);
        let object_size = if typ == TYPE_PTR {
            40
        } else if typ == TYPE_FDA {
            32
        } else {
            24
        };
        if offset
            .checked_add(object_size)
            .is_none_or(|end| end > data.len())
        {
            return Err(EINVAL);
        }
        previous_end = offset + object_size;
        if typ == TYPE_FD || typ == TYPE_PTR || typ == TYPE_FDA {
            continue;
        }
        let weak = typ == TYPE_WEAK_BINDER || typ == TYPE_WEAK_HANDLE;
        let node_id = if typ == TYPE_BINDER || typ == TYPE_WEAK_BINDER {
            let ptr = get64(data, offset + 8);
            let cookie = get64(data, offset + 16);
            if let Some((&id, node)) = s
                .nodes
                .iter()
                .find(|(_, node)| node.owner == from && node.ptr == ptr)
            {
                if node.cookie != cookie {
                    return Err(EINVAL);
                }
                id
            } else {
                s.next_node += 1;
                let id = s.next_node;
                s.nodes.insert(
                    id,
                    BinderNode {
                        owner: from,
                        ptr,
                        cookie,
                        flags: get32(data, offset + 4),
                        async_active: false,
                        async_todo: VecDeque::new(),
                    },
                );
                id
            }
        } else if typ == TYPE_HANDLE || typ == TYPE_WEAK_HANDLE {
            let handle = get32(data, offset + 8);
            if handle == 0 {
                s.manager_node.ok_or(EINVAL)?
            } else {
                s.procs
                    .get(&from)
                    .and_then(|p| p.handles.get(&handle))
                    .filter(|r| weak || r.strong > 0)
                    .map(|r| r.node)
                    .ok_or(EINVAL)?
            }
        } else {
            return Err(EINVAL);
        };
        let node = s.nodes.get(&node_id).ok_or(EINVAL)?;
        let (owner, ptr, cookie, flags) = (node.owner, node.ptr, node.cookie, node.flags);
        if owner == to {
            put32(
                data,
                offset,
                if weak { TYPE_WEAK_BINDER } else { TYPE_BINDER },
            );
            put32(data, offset + 4, flags);
            put64(data, offset + 8, ptr);
            put64(data, offset + 16, cookie);
        } else {
            let target = s.procs.get_mut(&to).ok_or(ECONNREFUSED)?;
            let handle =
                if let Some((&h, _)) = target.handles.iter().find(|(_, r)| r.node == node_id) {
                    h
                } else {
                    let mut h = 1;
                    while target.handles.contains_key(&h) {
                        h += 1;
                    }
                    target.handles.insert(
                        h,
                        BinderRef {
                            node: node_id,
                            strong: 0,
                            weak: 0,
                            death: None,
                        },
                    );
                    h
                };
            let _ = target;
            change_ref(s, to, handle, bc(if weak { 4 } else { 5 }, 4))?;
            refs.push((handle, weak));
            put32(
                data,
                offset,
                if weak { TYPE_WEAK_HANDLE } else { TYPE_HANDLE },
            );
            put64(data, offset + 8, handle as u64);
            put64(data, offset + 16, 0);
        }
    }
    Ok(())
}
fn get32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn get64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}
fn put32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}
impl Node for Binder {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            ino: self.id,
            mode: S_IFCHR | 0o666,
            nlink: 1,
            ..Stat::default()
        }
    }
    fn shared_mem(&self) -> Option<Arc<SharedMem>> {
        Some(self.mem.clone())
    }
    fn mmapped(&self, ctx: &Ctx, addr: u64, len: u64, offset: u64) -> Result<()> {
        self.bind(ctx)?;
        if offset != 0 || len == 0 || len > 16 * 1024 * 1024 {
            return Err(EINVAL);
        }
        let mut s = self.context.state.lock();
        let p = s.procs.get_mut(&self.id).ok_or(ECONNREFUSED)?;
        if p.size != 0 {
            return Err(EBUSY);
        }
        p.base = addr;
        p.size = len;
        p.free.insert(0, len);
        drop(s);
        self.mem.resize(len);
        Ok(())
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> Result<i64> {
        self.bind(ctx)?;
        match cmd {
            WRITE_READ => {
                let r = self.write_read(ctx, arg);
                if let Err(e) = r {
                    if std::env::var_os("INDROID_BINDER_TRACE").is_some() || e == ENOTTY {
                        eprintln!(
                            "binder pid={} tid={} write_read error {e}",
                            ctx.process.pid, ctx.thread.tid
                        );
                    }
                }
                r
            }
            VERSION => ctx.mm.write_u32(arg, 8).then_some(0).ok_or(EFAULT),
            SET_MAX_THREADS => {
                let n = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                self.context
                    .state
                    .lock()
                    .procs
                    .get_mut(&self.id)
                    .ok_or(ECONNREFUSED)?
                    .max_threads = n;
                Ok(0)
            }
            SET_CONTEXT_MGR | SET_CONTEXT_MGR_EXT => {
                let mut obj = [0u8; 24];
                if cmd == SET_CONTEXT_MGR_EXT && !ctx.mm.read(arg, &mut obj) {
                    return Err(EFAULT);
                }
                let mut s = self.context.state.lock();
                if s.manager.is_some() {
                    return Err(EBUSY);
                }
                s.next_node += 1;
                let id = s.next_node;
                s.nodes.insert(
                    id,
                    BinderNode {
                        owner: self.id,
                        ptr: get64(&obj, 8),
                        cookie: get64(&obj, 16),
                        flags: if cmd == SET_CONTEXT_MGR_EXT {
                            get32(&obj, 4)
                        } else {
                            0x100
                        },
                        async_active: false,
                        async_todo: VecDeque::new(),
                    },
                );
                s.manager = Some(self.id);
                s.manager_node = Some(id);
                Ok(0)
            }
            THREAD_EXIT => {
                let mut s = self.context.state.lock();
                let thread = s
                    .procs
                    .get_mut(&self.id)
                    .ok_or(ECONNREFUSED)?
                    .threads
                    .remove(&ctx.thread.tid);
                if let Some(thread) = &thread {
                    for &(sender, tid, _) in &thread.stack {
                        if let Some(p) = s.procs.get_mut(&sender) {
                            if let Some(t) = p.threads.get_mut(&tid) {
                                t.waiting_on = None;
                                t.todo.push_back(Work::Command(DEAD_REPLY));
                            }
                            if let Some(wq) = &p.wq {
                                wq.notify();
                            }
                        }
                    }
                }
                drop(s);
                drop(thread);
                Ok(0)
            }
            c if c == ioc(1, b'b' as u32, 3, 8)
                || c == ioc(1, b'b' as u32, 6, 4)
                || c == ioc(1, b'b' as u32, 14, 12)
                || c == ioc(1, b'b' as u32, 16, 4) =>
            {
                Ok(0)
            }
            c if c == ioc(3, b'b' as u32, 11, 24) => {
                ctx.mm.write(arg, &[0; 24]).then_some(0).ok_or(EFAULT)
            }
            c if c == ioc(3, b'b' as u32, 12, 24) => {
                let handle = ctx.mm.read_u32(arg).ok_or(EFAULT)?;
                // GET_NODE_INFO_FOR_REF reports the *node's* counts like
                // Linux (internal refs: one per process holding a strong /
                // any reference), not the caller's own ref; hwservicemanager
                // uses strong > 1 to decide whether a lazy HAL has clients.
                let (strong, weak) = {
                    let s = self.context.state.lock();
                    let node = s
                        .procs
                        .get(&self.id)
                        .and_then(|p| p.handles.get(&handle))
                        .ok_or(EINVAL)?
                        .node;
                    s.procs
                        .values()
                        .flat_map(|p| p.handles.values())
                        .filter(|r| r.node == node)
                        .fold((0u32, 0u32), |(st, wk), r| {
                            (
                                st + (r.strong > 0) as u32,
                                wk + (r.strong > 0 || r.weak > 0) as u32,
                            )
                        })
                };
                let mut result = [0u8; 24];
                put32(&mut result, 0, handle);
                put32(&mut result, 4, strong);
                put32(&mut result, 8, weak);
                ctx.mm.write(arg, &result).then_some(0).ok_or(EFAULT)
            }
            c if c == ioc(3, b'b' as u32, 15, 12) => {
                if !ctx.mm.write(arg + 4, &[0; 8]) {
                    return Err(EFAULT);
                }
                Ok(0)
            }
            c if c == ioc(3, b'b' as u32, 17, 12) => {
                ctx.mm.write(arg, &[0; 12]).then_some(0).ok_or(EFAULT)
            }
            _ => Err(ENOTTY),
        }
    }
    fn poll(&self) -> u32 {
        // Like Linux binder_poll: readable if the process queue or the
        // *polling thread's* own queue has work (servicemanager polls its
        // single looper thread through epoll).
        let tid = crate::task::current_tid();
        let s = self.context.state.lock();
        let ready = s.procs.get(&self.id).is_some_and(|p| {
            let thread = tid.and_then(|tid| p.threads.get(&tid));
            let available =
                thread.is_none_or(|t| t.looper && t.waiting_on.is_none() && t.stack.is_empty());
            (available && !p.todo.is_empty()) || thread.is_some_and(|t| !t.todo.is_empty())
        });
        POLLOUT | if ready { POLLIN } else { 0 }
    }
    fn wait_queue(&self) -> Option<Arc<WaitQueue>> {
        Some(self.wq.clone())
    }
    fn release(&self) {
        let mut s = self.context.state.lock();
        let ref_nodes: Vec<u64> = s
            .procs
            .get(&self.id)
            .map(|p| p.handles.values().map(|r| r.node).collect())
            .unwrap_or_default();
        let mut counts = HashMap::new();
        for node in ref_nodes {
            counts.entry(node).or_insert_with(|| node_counts(&s, node));
        }
        let removed = s.procs.remove(&self.id);
        for (node, old) in counts {
            let _ = notify_ref_change(&mut s, node, old);
        }
        let dead_nodes: Vec<u64> = s
            .nodes
            .iter()
            .filter_map(|(&id, n)| (n.owner == self.id).then_some(id))
            .collect();
        let mut abandoned = Vec::new();
        for id in &dead_nodes {
            if let Some(node) = s.nodes.get_mut(id) {
                abandoned.extend(node.async_todo.drain(..));
                node.async_active = false;
            }
        }
        for proc_ in s.procs.values_mut() {
            for reference in proc_.handles.values() {
                if dead_nodes.contains(&reference.node) {
                    if let Some(cookie) = reference.death {
                        proc_.todo.push_back(cookie_work(15, cookie));
                    }
                }
            }
            for thread in proc_.threads.values_mut() {
                if thread.waiting_on == Some(self.id) {
                    thread.waiting_on = None;
                    thread.todo.push_back(Work::Command(DEAD_REPLY));
                }
            }
            if let Some(wq) = &proc_.wq {
                wq.notify();
            }
        }
        if s.manager == Some(self.id) {
            s.manager = None;
            s.manager_node = None;
        }
        drop(s);
        drop(removed);
        drop(abandoned);
        self.wq.notify();
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
