//! Sockets: AF_UNIX (stream, datagram, seqpacket; SCM_RIGHTS and
//! SCM_CREDENTIALS; filesystem and abstract names) fully in-process, a
//! do-nothing AF_NETLINK, and AF_INET/AF_INET6 (not yet).

use crate::errno::*;
use crate::fd::*;
use crate::sync::WaitQueue;
use crate::task::Ctx;
use crate::vfs::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

const AF_UNIX: u64 = 1;
const AF_INET: u64 = 2;
const AF_INET6: u64 = 10;
const AF_NETLINK: u64 = 16;
const SOCK_STREAM: u32 = 1;
const SOCK_DGRAM: u32 = 2;
const SOCK_SEQPACKET: u32 = 5;
const SOCK_NONBLOCK: u32 = 0o4000;
const SOCK_CLOEXEC: u32 = 0o2000000;
const SOL_SOCKET: u64 = 1;
const SCM_RIGHTS: i32 = 1;
const SCM_CREDENTIALS: i32 = 2;
const MSG_PEEK: u32 = 2;
const MSG_TRUNC: u32 = 0x20;
const MSG_DONTWAIT: u32 = 0x40;
const MSG_WAITALL: u32 = 0x100;
const MSG_NOSIGNAL: u32 = 0x4000;
const MSG_CMSG_CLOEXEC: u32 = 0x4000_0000;

#[derive(Clone, Copy, Default)]
pub struct Cred {
    pub pid: i32,
    pub uid: u32,
    pub gid: u32,
}

pub struct Msg {
    data: Vec<u8>,
    /// Bytes of `data` already consumed (stream sockets).
    off: usize,
    fds: Vec<Arc<OpenFile>>,
    cred: Cred,
    from: Option<String>,
}

/// Receive side of an endpoint.
#[derive(Default)]
pub struct RxQueue {
    msgs: VecDeque<Msg>,
    bytes: usize,
    /// Peer closed / shut down its write side.
    eof: bool,
}

pub enum State {
    Unbound,
    Listening { backlog: VecDeque<Arc<UnixSocket>> },
    Connected { peer: Weak<UnixSocket> },
    Closed,
}

pub struct UnixSocket {
    pub ty: u32,
    pub rx: Mutex<RxQueue>,
    pub state: Mutex<State>,
    pub name: Mutex<Option<String>>,
    pub peer_name: Mutex<Option<String>>,
    /// Credentials of the peer at connect time (SO_PEERCRED).
    pub peer_cred: Mutex<Cred>,
    pub my_cred: Mutex<Cred>,
    pub passcred: Mutex<bool>,
    /// Default destination for connected datagram sockets.
    pub dgram_peer: Mutex<Option<Weak<UnixSocket>>>,
    pub wq: Arc<WaitQueue>,
    pub shut_wr: Mutex<bool>,
    pub rcvtimeo: Mutex<Option<std::time::Duration>>,
    id: u64,
}

const RCVBUF: usize = 256 * 1024;

impl UnixSocket {
    pub fn new(ty: u32, cred: Cred) -> Arc<UnixSocket> {
        Arc::new(UnixSocket {
            ty,
            rx: Mutex::new(RxQueue::default()),
            state: Mutex::new(State::Unbound),
            name: Mutex::new(None),
            peer_name: Mutex::new(None),
            peer_cred: Mutex::new(Cred::default()),
            my_cred: Mutex::new(cred),
            passcred: Mutex::new(false),
            dgram_peer: Mutex::new(None),
            wq: WaitQueue::new(),
            shut_wr: Mutex::new(false),
            rcvtimeo: Mutex::new(None),
            id: new_id(),
        })
    }

    fn peer(&self) -> Option<Arc<UnixSocket>> {
        match &*self.state.lock() {
            State::Connected { peer } => peer.upgrade(),
            _ => None,
        }
    }

    pub fn pair(ty: u32, cred: Cred) -> (Arc<UnixSocket>, Arc<UnixSocket>) {
        let a = UnixSocket::new(ty, cred);
        let b = UnixSocket::new(ty, cred);
        *a.state.lock() = State::Connected {
            peer: Arc::downgrade(&b),
        };
        *b.state.lock() = State::Connected {
            peer: Arc::downgrade(&a),
        };
        *a.peer_cred.lock() = cred;
        *b.peer_cred.lock() = cred;
        if ty == SOCK_DGRAM {
            *a.dgram_peer.lock() = Some(Arc::downgrade(&b));
            *b.dgram_peer.lock() = Some(Arc::downgrade(&a));
        }
        (a, b)
    }

    fn deliver(&self, msg: Msg) -> Result<()> {
        let mut rx = self.rx.lock();
        if rx.bytes + msg.data.len() > RCVBUF * 4 && self.ty != SOCK_STREAM {
            return Err(EAGAIN);
        }
        rx.bytes += msg.data.len();
        rx.msgs.push_back(msg);
        drop(rx);
        self.wq.notify();
        Ok(())
    }

    /// Sends one message (blocking while the peer's buffer is full).
    fn send(
        &self,
        data: &[u8],
        fds: Vec<Arc<OpenFile>>,
        cred: Cred,
        to: Option<Arc<UnixSocket>>,
        nonblock: bool,
    ) -> Result<usize> {
        if *self.shut_wr.lock() {
            return Err(EPIPE);
        }
        let target = match to {
            Some(t) => t,
            None => {
                if self.ty == SOCK_DGRAM {
                    if let Some(p) = self.dgram_peer.lock().as_ref().and_then(|w| w.upgrade()) {
                        p
                    } else {
                        match self.peer() {
                            Some(p) => p,
                            None => {
                                return Err(
                                    if matches!(*self.state.lock(), State::Connected { .. }) {
                                        ECONNREFUSED
                                    } else {
                                        EDESTADDRREQ
                                    },
                                )
                            }
                        }
                    }
                } else {
                    match &*self.state.lock() {
                        State::Connected { peer } => peer.upgrade().ok_or(EPIPE)?,
                        _ => return Err(ENOTCONN),
                    }
                }
            }
        };
        // Flow control for stream sockets.
        loop {
            let seen = target.wq.epoch();
            let full = target.rx.lock().bytes >= RCVBUF;
            if !full || self.ty != SOCK_STREAM {
                break;
            }
            if nonblock {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            if matches!(*target.state.lock(), State::Closed) {
                return Err(EPIPE);
            }
            target.wq.wait_change(seen, None);
        }
        if matches!(*target.state.lock(), State::Closed) {
            return Err(if self.ty == SOCK_DGRAM {
                ECONNREFUSED
            } else {
                EPIPE
            });
        }
        if *LOGCAT && target.name.lock().as_deref() == Some("/dev/socket/logdw") {
            mirror_log(data, cred.pid);
        }
        target.deliver(Msg {
            data: data.to_vec(),
            off: 0,
            fds,
            cred,
            from: self.name.lock().clone(),
        })?;
        // Our own pollers (POLLOUT) don't need a wake; the peer was notified.
        Ok(data.len())
    }

    /// Receives up to `max` bytes. Returns (data, fds, cred, from, truncated_len).
    fn recv(
        &self,
        max: usize,
        flags: u32,
        nonblock: bool,
    ) -> Result<(Vec<u8>, Vec<Arc<OpenFile>>, Cred, Option<String>, usize)> {
        let deadline = self.rcvtimeo.lock().map(|d| std::time::Instant::now() + d);
        loop {
            let seen = self.wq.epoch();
            {
                let mut rx = self.rx.lock();
                if !rx.msgs.is_empty() {
                    if self.ty == SOCK_STREAM {
                        // Coalesce across messages, but never across fd-carrying
                        // boundaries (ancillary data attaches to its first byte).
                        let mut out = Vec::new();
                        let mut fds = Vec::new();
                        let mut cred = Cred::default();
                        let mut first = true;
                        let peek = flags & MSG_PEEK != 0;
                        let mut idx = 0;
                        while out.len() < max && idx < rx.msgs.len() {
                            let m = &mut rx.msgs[idx];
                            if !first && !m.fds.is_empty() {
                                break;
                            }
                            if first {
                                cred = m.cred;
                                if !peek {
                                    fds = std::mem::take(&mut m.fds);
                                }
                            }
                            first = false;
                            let avail = &m.data[m.off..];
                            let n = avail.len().min(max - out.len());
                            out.extend_from_slice(&avail[..n]);
                            if !peek {
                                m.off += n;
                            }
                            if m.off < m.data.len() || peek {
                                if peek && n == avail.len() {
                                    idx += 1;
                                    continue;
                                }
                                break;
                            }
                            idx += 1;
                        }
                        if !peek {
                            while rx.msgs.front().is_some_and(|m| m.off >= m.data.len()) {
                                rx.msgs.pop_front();
                            }
                            rx.bytes = rx.bytes.saturating_sub(out.len());
                        }
                        drop(rx);
                        self.wq.notify();
                        return Ok((out, fds, cred, None, 0));
                    }
                    let m = if flags & MSG_PEEK != 0 {
                        let m = rx.msgs.front().unwrap();
                        Msg {
                            data: m.data.clone(),
                            off: 0,
                            fds: Vec::new(),
                            cred: m.cred,
                            from: m.from.clone(),
                        }
                    } else {
                        let m = rx.msgs.pop_front().unwrap();
                        rx.bytes = rx.bytes.saturating_sub(m.data.len());
                        m
                    };
                    drop(rx);
                    self.wq.notify();
                    let full = m.data.len();
                    let mut d = m.data;
                    d.truncate(max);
                    return Ok((d, m.fds, m.cred, m.from, full));
                }
                if rx.eof || *self.shut_wr.lock() && false {
                    return Ok((Vec::new(), Vec::new(), Cred::default(), None, 0));
                }
            }
            // Release our state lock before locking the peer's (ABBA).
            let (closed, peer) = match &*self.state.lock() {
                State::Connected { peer } => (false, Some(peer.clone())),
                State::Closed => (true, None),
                _ => (false, None),
            };
            let connected_gone = closed
                || peer.is_some_and(|w| {
                    w.upgrade()
                        .is_none_or(|p| matches!(*p.state.lock(), State::Closed))
                });
            if connected_gone && self.ty != SOCK_DGRAM {
                return Ok((Vec::new(), Vec::new(), Cred::default(), None, 0));
            }
            if nonblock || flags & MSG_DONTWAIT != 0 {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                return Err(EAGAIN);
            }
            self.wq.wait_change(
                seen,
                Some(std::time::Instant::now() + std::time::Duration::from_millis(50)),
            );
        }
    }
}

impl Node for UnixSocket {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFSOCK | 0o777,
            ino: self.id,
            nlink: 1,
            blksize: 4096,
            ..Default::default()
        }
    }
    fn is_stream(&self) -> bool {
        true
    }
    fn read_at(&self, _o: u64, buf: &mut [u8], nonblock: bool) -> Result<usize> {
        let (d, _fds, _, _, _) = self.recv(buf.len(), 0, nonblock)?;
        buf[..d.len()].copy_from_slice(&d);
        Ok(d.len())
    }
    fn write_at(&self, _o: u64, buf: &[u8], nonblock: bool) -> Result<usize> {
        let cred = *self.my_cred.lock();
        match self.send(buf, Vec::new(), cred, None, nonblock) {
            Err(EPIPE) => {
                crate::task::raise_current(13);
                Err(EPIPE)
            }
            r => r,
        }
    }
    fn poll(&self) -> u32 {
        let mut r = 0;
        // Snapshot our own state, then release our locks before touching the
        // peer: two threads polling both ends must not deadlock (ABBA).
        let peer = {
            let rx = self.rx.lock();
            let state = self.state.lock();
            if rx.eof {
                r |= POLLIN | POLLRDHUP;
            }
            match &*state {
                State::Listening { backlog } => {
                    if !backlog.is_empty() {
                        r |= POLLIN;
                    }
                    None
                }
                State::Connected { peer } => {
                    if !rx.msgs.is_empty() {
                        r |= POLLIN;
                    }
                    Some(peer.clone())
                }
                State::Unbound => {
                    if !rx.msgs.is_empty() {
                        r |= POLLIN;
                    }
                    if self.ty == SOCK_DGRAM {
                        r |= POLLOUT;
                    }
                    None
                }
                State::Closed => {
                    r |= POLLHUP;
                    None
                }
            }
        };
        if let Some(peer) = peer {
            match peer.upgrade() {
                Some(p) if !matches!(*p.state.lock(), State::Closed) => {
                    if p.rx.lock().bytes < RCVBUF {
                        r |= POLLOUT;
                    }
                }
                _ => {
                    if self.ty == SOCK_DGRAM {
                        r |= POLLOUT;
                    } else {
                        r |= POLLIN | POLLHUP | POLLRDHUP;
                    }
                }
            }
        }
        r
    }
    fn wait_queue(&self) -> Option<Arc<WaitQueue>> {
        Some(self.wq.clone())
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> Result<i64> {
        if cmd == 0x541b {
            let n = self.rx.lock().bytes as u32;
            return ctx.mm.write_u32(arg, n).then_some(0).ok_or(EFAULT);
        }
        Err(ENOTTY)
    }
    fn release(&self) {
        let peer = self.peer();
        *self.state.lock() = State::Closed;
        if let Some(name) = self.name.lock().clone() {
            if name.starts_with('@') {
                if let Some(k) = crate::task::current_thread() {
                    let _ = k;
                }
                ABSTRACT
                    .lock()
                    .retain(|n, s| !(n == &name && s.strong_count() == 0));
            }
        }
        if let Some(p) = peer {
            p.rx.lock().eof = true;
            p.wq.notify();
        }
        self.wq.notify();
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Abstract-namespace sockets ("@name").
static ABSTRACT: std::sync::LazyLock<Mutex<std::collections::HashMap<String, Weak<UnixSocket>>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Netlink socket that accepts everything and never delivers events.
pub struct NetlinkSocket {
    id: u64,
    wq: Arc<WaitQueue>,
    protocol: u32,
    rx: Mutex<VecDeque<Vec<u8>>>,
}

impl NetlinkSocket {
    /// There is no network stack behind netlink: answer every request the
    /// way a host without interfaces, routes or rules would, so callers
    /// waiting for a reply (netd's RouteController, libnl dumps) proceed.
    fn request(&self, data: &[u8]) {
        if self.protocol == 15 {
            return; // NETLINK_KOBJECT_UEVENT: broadcasts only
        }
        let mut off = 0;
        let mut replies = Vec::new();
        while off + 16 <= data.len() {
            let h = &data[off..off + 16];
            let len = u32::from_le_bytes(h[0..4].try_into().unwrap()) as usize;
            let ty = u16::from_le_bytes(h[4..6].try_into().unwrap());
            let flags = u16::from_le_bytes(h[6..8].try_into().unwrap());
            let seq = &h[8..12];
            if len < 16 {
                break;
            }
            if ty >= 16 {
                let hdr = |len: u32, ty: u16, flags: u16| {
                    let mut m = Vec::with_capacity(len as usize);
                    m.extend_from_slice(&len.to_le_bytes());
                    m.extend_from_slice(&ty.to_le_bytes());
                    m.extend_from_slice(&flags.to_le_bytes());
                    m.extend_from_slice(seq);
                    m.extend_from_slice(&0u32.to_le_bytes());
                    m
                };
                let is_get = self.protocol == 0 && ty & 3 == 2;
                if flags & 0x300 != 0 {
                    // NLM_F_DUMP: an empty multipart answer.
                    let mut m = hdr(20, 3, 2); // NLMSG_DONE, NLM_F_MULTI
                    m.extend_from_slice(&0i32.to_le_bytes());
                    replies.push(m);
                } else if is_get || flags & 4 != 0 {
                    // NLMSG_ERROR: ENODEV for lookups, success for changes.
                    let mut m = hdr(36, 2, 0);
                    let err: i32 = if is_get { -19 } else { 0 };
                    m.extend_from_slice(&err.to_le_bytes());
                    m.extend_from_slice(h);
                    replies.push(m);
                }
            }
            off += (len + 3) & !3;
        }
        if !replies.is_empty() {
            self.rx.lock().extend(replies);
            self.wq.notify();
        }
    }
    fn pop(&self, nonblock: bool) -> Result<Vec<u8>> {
        loop {
            let seen = self.wq.epoch();
            if let Some(m) = self.rx.lock().pop_front() {
                return Ok(m);
            }
            if nonblock {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            self.wq.wait_change(
                seen,
                Some(std::time::Instant::now() + std::time::Duration::from_millis(100)),
            );
        }
    }
}

impl Node for NetlinkSocket {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFSOCK | 0o777,
            ino: self.id,
            nlink: 1,
            ..Default::default()
        }
    }
    fn is_stream(&self) -> bool {
        true
    }
    fn read_at(&self, _o: u64, b: &mut [u8], nonblock: bool) -> Result<usize> {
        let m = self.pop(nonblock)?;
        let n = m.len().min(b.len());
        b[..n].copy_from_slice(&m[..n]);
        Ok(n)
    }
    fn write_at(&self, _o: u64, b: &[u8], _nb: bool) -> Result<usize> {
        self.request(b);
        Ok(b.len())
    }
    fn poll(&self) -> u32 {
        if self.rx.lock().is_empty() {
            POLLOUT
        } else {
            POLLOUT | POLLIN
        }
    }
    fn wait_queue(&self) -> Option<Arc<WaitQueue>> {
        Some(self.wq.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn my_cred(ctx: &Ctx) -> Cred {
    Cred {
        pid: ctx.process.pid,
        uid: ctx.process.uid.load(Ordering::Relaxed),
        gid: ctx.process.gid.load(Ordering::Relaxed),
    }
}

fn sock(ctx: &Ctx, fd: u64) -> Result<(Arc<OpenFile>, Arc<UnixSocket>)> {
    let f = ctx.fds().lock().get(fd as i32)?;
    if f.node.as_any().downcast_ref::<UnixSocket>().is_none() {
        if f.node.as_any().downcast_ref::<NetlinkSocket>().is_some() {
            return Err(EOPNOTSUPP);
        }
        return Err(ENOTSOCK);
    }
    // Re-borrow as Arc<UnixSocket> via the node Arc.
    let node = f.node.clone();
    let raw = Arc::into_raw(node) as *const UnixSocket;
    let s = unsafe { Arc::from_raw(raw) };
    Ok((f, s))
}

/// Parses a sockaddr_un into a registry key: "/abs/path" or "@abstract".
fn read_addr(ctx: &Ctx, addr: u64, len: u64) -> Result<String> {
    if len < 2 || len > 110 {
        return Err(EINVAL);
    }
    let mut b = vec![0u8; len as usize];
    if !ctx.mm.read(addr, &mut b) {
        return Err(EFAULT);
    }
    let fam = u16::from_le_bytes([b[0], b[1]]) as u64;
    if fam != AF_UNIX {
        return Err(EAFNOSUPPORT);
    }
    let path = &b[2..];
    if path.first() == Some(&0) {
        return Ok(format!("@{}", String::from_utf8_lossy(&path[1..])));
    }
    let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    let p = String::from_utf8_lossy(&path[..end]).into_owned();
    if p.is_empty() {
        return Err(EINVAL);
    }
    Ok(p)
}

fn write_addr(ctx: &Ctx, addr: u64, lenp: u64, name: Option<&str>) {
    if addr == 0 || lenp == 0 {
        return;
    }
    let mut b = vec![1u8, 0];
    if let Some(n) = name {
        if let Some(a) = n.strip_prefix('@') {
            b.push(0);
            b.extend_from_slice(a.as_bytes());
        } else {
            b.extend_from_slice(n.as_bytes());
            b.push(0);
        }
    }
    let cap = ctx.mm.read_u32(lenp).unwrap_or(0) as usize;
    ctx.mm.write(addr, &b[..b.len().min(cap)]);
    ctx.mm.write_u32(lenp, b.len() as u32);
}

fn canonical(ctx: &Ctx, name: &str) -> Result<String> {
    if name.starts_with('@') {
        return Ok(name.to_string());
    }
    // Resolve the directory part; the socket file itself may not exist.
    let cwd = ctx.process.cwd.lock().clone();
    let (dir, file) = match name.rfind('/') {
        Some(0) => ("/".to_string(), &name[1..]),
        Some(i) => (name[..i].to_string(), &name[i + 1..]),
        None => (cwd.clone(), name),
    };
    let (d, _) = ctx.kernel.vfs.resolve(ctx, &cwd, &dir, true)?;
    Ok(if d == "/" {
        format!("/{file}")
    } else {
        format!("{d}/{file}")
    })
}

fn lookup(ctx: &Ctx, name: &str) -> Result<Arc<UnixSocket>> {
    let key = canonical(ctx, name)?;
    if key.starts_with('@') {
        return ABSTRACT
            .lock()
            .get(&key)
            .and_then(|w| w.upgrade())
            .ok_or(ECONNREFUSED);
    }
    let node = ctx.kernel.vfs.unix_sockets.lock().get(&key).cloned();
    match node {
        Some(n) => {
            let raw = Arc::into_raw(n) as *const UnixSocket;
            Ok(unsafe { Arc::from_raw(raw) })
        }
        None => {
            // Exists as a file but nobody listening?
            match ctx.kernel.vfs.resolve(ctx, "/", &key, true) {
                Ok((_, Loc::Missing { .. })) | Err(_) => Err(ENOENT),
                _ => Err(ECONNREFUSED),
            }
        }
    }
}

pub fn syscall(ctx: &Ctx, nr: u64, a: [u64; 6]) -> i64 {
    let r: Result<i64> = (|| match nr {
        198 => {
            // socket(domain, type, protocol)
            let ty = a[1] as u32 & 0xf;
            let flags = a[1] as u32;
            let node: FileRef = match a[0] {
                AF_UNIX => {
                    if ![SOCK_STREAM, SOCK_DGRAM, SOCK_SEQPACKET].contains(&ty) {
                        return Err(EPROTOTYPE);
                    }
                    UnixSocket::new(ty, my_cred(ctx))
                }
                AF_NETLINK => Arc::new(NetlinkSocket {
                    id: new_id(),
                    wq: WaitQueue::new(),
                    protocol: a[2] as u32,
                    rx: Mutex::new(VecDeque::new()),
                }),
                AF_INET | AF_INET6 => return Err(EAFNOSUPPORT),
                _ => return Err(EAFNOSUPPORT),
            };
            let f = OpenFile::new(node, O_RDWR | (flags & SOCK_NONBLOCK), "socket:");
            Ok(ctx
                .process
                .fds()
                .lock()
                .insert(f, flags & SOCK_CLOEXEC != 0)? as i64)
        }
        199 => {
            if a[0] != AF_UNIX {
                return Err(EAFNOSUPPORT);
            }
            let ty = a[1] as u32 & 0xf;
            let flags = a[1] as u32;
            let (x, y) = UnixSocket::pair(ty, my_cred(ctx));
            let fds = ctx.fds();
            let mut t = fds.lock();
            let fx = t.insert(
                OpenFile::new(x, O_RDWR | (flags & SOCK_NONBLOCK), "socket:"),
                flags & SOCK_CLOEXEC != 0,
            )?;
            let fy = t.insert(
                OpenFile::new(y, O_RDWR | (flags & SOCK_NONBLOCK), "socket:"),
                flags & SOCK_CLOEXEC != 0,
            )?;
            drop(t);
            if !ctx.mm.write(
                a[3],
                &[(fx as u32).to_le_bytes(), (fy as u32).to_le_bytes()].concat(),
            ) {
                return Err(EFAULT);
            }
            Ok(0)
        }
        200 => {
            // bind
            let f = ctx.fds().lock().get(a[0] as i32)?;
            if f.node.as_any().is::<NetlinkSocket>() {
                return Ok(0);
            }
            let (_, s) = sock(ctx, a[0])?;
            let name = read_addr(ctx, a[1], a[2])?;
            let key = canonical(ctx, &name)?;
            if key.starts_with('@') {
                let mut abs = ABSTRACT.lock();
                if abs.get(&key).is_some_and(|w| w.strong_count() > 0) {
                    return Err(EADDRINUSE);
                }
                abs.insert(key.clone(), Arc::downgrade(&s));
            } else {
                let cwd = ctx.process.cwd.lock().clone();
                match ctx.kernel.vfs.resolve(ctx, &cwd, &key, false)? {
                    (_, Loc::Missing { host, rw: true }) => {
                        std::fs::File::create(&host).map_err(|e| host_err(&e))?;
                    }
                    (_, Loc::Missing { rw: false, .. }) => return Err(EROFS),
                    _ => return Err(EADDRINUSE),
                }
                ctx.kernel
                    .vfs
                    .unix_sockets
                    .lock()
                    .insert(key.clone(), s.clone() as FileRef);
            }
            *s.name.lock() = Some(key);
            Ok(0)
        }
        201 => {
            let (_, s) = sock(ctx, a[0])?;
            let mut st = s.state.lock();
            if matches!(*st, State::Unbound) {
                *st = State::Listening {
                    backlog: VecDeque::new(),
                };
            }
            Ok(0)
        }
        202 | 242 => {
            // accept / accept4
            let (f, s) = sock(ctx, a[0])?;
            let flags = if nr == 242 { a[3] as u32 } else { 0 };
            loop {
                let seen = s.wq.epoch();
                {
                    let mut st = s.state.lock();
                    let State::Listening { backlog } = &mut *st else {
                        return Err(EINVAL);
                    };
                    if let Some(conn) = backlog.pop_front() {
                        drop(st);
                        s.wq.notify();
                        let peer_name = conn.peer_name.lock().clone();
                        let nf = OpenFile::new(
                            conn.clone(),
                            O_RDWR | (flags & SOCK_NONBLOCK),
                            "socket:",
                        );
                        let n = ctx
                            .process
                            .fds()
                            .lock()
                            .insert(nf, flags & SOCK_CLOEXEC != 0)?;
                        write_addr(ctx, a[1], a[2], peer_name.as_deref());
                        return Ok(n as i64);
                    }
                }
                if f.nonblock() {
                    return Err(EAGAIN);
                }
                if ctx.signal_pending() {
                    return Err(EINTR);
                }
                s.wq.wait_change(
                    seen,
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(50)),
                );
            }
        }
        203 => {
            // connect
            let f = ctx.fds().lock().get(a[0] as i32)?;
            if f.node.as_any().is::<NetlinkSocket>() {
                return Ok(0);
            }
            let (_, s) = sock(ctx, a[0])?;
            let name = read_addr(ctx, a[1], a[2])?;
            let server = lookup(ctx, &name)?;
            if s.ty == SOCK_DGRAM {
                *s.dgram_peer.lock() = Some(Arc::downgrade(&server));
                *s.peer_name.lock() = server.name.lock().clone();
                return Ok(0);
            }
            if server.ty != s.ty {
                return Err(EPROTOTYPE);
            }
            let me = my_cred(ctx);
            let accepted = UnixSocket::new(s.ty, *server.my_cred.lock());
            *accepted.state.lock() = State::Connected {
                peer: Arc::downgrade(&s),
            };
            *accepted.peer_cred.lock() = me;
            *accepted.name.lock() = server.name.lock().clone();
            *accepted.peer_name.lock() = s.name.lock().clone();
            *accepted.passcred.lock() = *server.passcred.lock();
            {
                let mut st = server.state.lock();
                let State::Listening { backlog } = &mut *st else {
                    return Err(ECONNREFUSED);
                };
                backlog.push_back(accepted.clone());
            }
            *s.state.lock() = State::Connected {
                peer: Arc::downgrade(&accepted),
            };
            *s.peer_cred.lock() = *server.my_cred.lock();
            *s.peer_name.lock() = server.name.lock().clone();
            // The accepted socket must stay alive until accept(): the backlog
            // holds it. Keep the connector->accepted link strong via the
            // backlog; after accept the fd table holds it.
            server.wq.notify();
            Ok(0)
        }
        204 | 205 => {
            let (_, s) = sock(ctx, a[0])?;
            let n = if nr == 204 {
                s.name.lock().clone()
            } else {
                s.peer_name.lock().clone()
            };
            if nr == 205 && s.peer().is_none() && s.dgram_peer.lock().is_none() {
                return Err(ENOTCONN);
            }
            write_addr(ctx, a[1], a[2], n.as_deref());
            Ok(0)
        }
        206 | 211 => send(ctx, nr, a),
        207 | 212 => recv(ctx, nr, a),
        243 | 269 => {
            // recvmmsg / sendmmsg: one at a time
            Err(ENOSYS)
        }
        208 => {
            // setsockopt(fd, level, opt, val, len)
            let f = ctx.fds().lock().get(a[0] as i32)?;
            let Some(s) = f.node.as_any().downcast_ref::<UnixSocket>() else {
                return Ok(0);
            };
            if a[1] == SOL_SOCKET {
                match a[2] {
                    16 => *s.passcred.lock() = ctx.mm.read_u32(a[3]).ok_or(EFAULT)? != 0,
                    20 => {
                        let sec = ctx.mm.read_u64(a[3]).ok_or(EFAULT)?;
                        let usec = ctx.mm.read_u64(a[3] + 8).ok_or(EFAULT)?;
                        let d = std::time::Duration::from_micros(sec * 1_000_000 + usec);
                        *s.rcvtimeo.lock() = (!d.is_zero()).then_some(d);
                    }
                    _ => {}
                }
            }
            Ok(0)
        }
        209 => {
            // getsockopt(fd, level, opt, val, lenp)
            let f = ctx.fds().lock().get(a[0] as i32)?;
            let s = f.node.as_any().downcast_ref::<UnixSocket>();
            let put = |v: &[u8]| -> Result<i64> {
                let cap = ctx.mm.read_u32(a[4]).ok_or(EFAULT)? as usize;
                let n = cap.min(v.len());
                ctx.mm.write(a[3], &v[..n]);
                ctx.mm.write_u32(a[4], n as u32);
                Ok(0)
            };
            match (a[1], a[2], s) {
                (SOL_SOCKET, 17, Some(s)) => {
                    // SO_PEERCRED
                    let c = *s.peer_cred.lock();
                    put(&[
                        c.pid.to_le_bytes(),
                        c.uid.to_le_bytes(),
                        c.gid.to_le_bytes(),
                    ]
                    .concat())
                }
                (SOL_SOCKET, 3, Some(s)) => put(&s.ty.to_le_bytes()),
                (SOL_SOCKET, 4, _) => put(&0u32.to_le_bytes()), // SO_ERROR
                (SOL_SOCKET, 7 | 8, _) => put(&(RCVBUF as u32).to_le_bytes()),
                (SOL_SOCKET, 16, Some(s)) => put(&(*s.passcred.lock() as u32).to_le_bytes()),
                (SOL_SOCKET, 31, _) => put(b"u:r:kernel:s0\0"), // SO_PEERSEC
                _ => put(&0u32.to_le_bytes()),
            }
        }
        210 => {
            let (_, s) = sock(ctx, a[0])?;
            let how = a[1];
            if how >= 1 {
                *s.shut_wr.lock() = true;
                if let Some(p) = s.peer() {
                    p.rx.lock().eof = true;
                    p.wq.notify();
                }
            }
            if how == 0 || how == 2 {
                s.rx.lock().eof = true;
                s.wq.notify();
            }
            Ok(0)
        }
        _ => Err(ENOSYS),
    })();
    match r {
        Ok(v) => v,
        Err(e) => e,
    }
}

fn send(ctx: &Ctx, nr: u64, a: [u64; 6]) -> Result<i64> {
    let f = ctx.fds().lock().get(a[0] as i32)?;
    let netlink = f.node.as_any().downcast_ref::<NetlinkSocket>();
    let s = if netlink.is_some() {
        None
    } else {
        Some(sock(ctx, a[0])?.1)
    };
    let (data, fds, dest, flags) = if nr == 206 {
        let mut d = vec![0u8; (a[2] as usize).min(1 << 22)];
        if !ctx.mm.read(a[1], &mut d) {
            return Err(EFAULT);
        }
        let dest = if a[4] != 0 && netlink.is_none() {
            Some(read_addr(ctx, a[4], a[5])?)
        } else {
            None
        };
        (d, Vec::new(), dest, a[3] as u32)
    } else {
        // struct msghdr: name, namelen(u32), iov, iovlen, control, controllen, flags
        let mut h = [0u8; 56];
        if !ctx.mm.read(a[1], &mut h) {
            return Err(EFAULT);
        }
        let g = |o: usize| u64::from_le_bytes(h[o..o + 8].try_into().unwrap());
        let name = g(0);
        let namelen = u32::from_le_bytes(h[8..12].try_into().unwrap()) as u64;
        let (iov, iovlen, control, controllen) = (g(16), g(24), g(32), g(40));
        let mut d = Vec::new();
        for i in 0..iovlen.min(1024) {
            let base = ctx.mm.read_u64(iov + i * 16).ok_or(EFAULT)?;
            let len = ctx.mm.read_u64(iov + i * 16 + 8).ok_or(EFAULT)? as usize;
            let mut b = vec![0u8; len.min(1 << 22)];
            if !ctx.mm.read(base, &mut b) {
                return Err(EFAULT);
            }
            d.extend_from_slice(&b);
        }
        let mut fds = Vec::new();
        let mut off = 0u64;
        while off + 16 <= controllen {
            let clen = ctx.mm.read_u64(control + off).ok_or(EFAULT)?;
            let level = ctx.mm.read_u32(control + off + 8).ok_or(EFAULT)?;
            let ty = ctx.mm.read_u32(control + off + 12).ok_or(EFAULT)? as i32;
            if clen < 16 {
                break;
            }
            if level as u64 == SOL_SOCKET && ty == SCM_RIGHTS {
                let n = (clen - 16) / 4;
                let table = ctx.fds();
                let t = table.lock();
                for i in 0..n {
                    let fd = ctx.mm.read_u32(control + off + 16 + i * 4).ok_or(EFAULT)? as i32;
                    fds.push(t.get(fd)?);
                }
            }
            off += (clen + 7) & !7;
        }
        let dest = if name != 0 && namelen > 0 && netlink.is_none() {
            Some(read_addr(ctx, name, namelen)?)
        } else {
            None
        };
        (d, fds, dest, a[2] as u32)
    };
    if let Some(n) = netlink {
        drop((fds, dest, flags));
        n.request(&data);
        return Ok(data.len() as i64);
    }
    let s = s.unwrap();
    let to = match dest {
        Some(n) if s.ty == SOCK_DGRAM => Some(lookup(ctx, &n)?),
        _ => None,
    };
    let nonblock = f.nonblock() || flags & MSG_DONTWAIT != 0;
    match s.send(&data, fds, my_cred(ctx), to, nonblock) {
        Err(EPIPE) if flags & MSG_NOSIGNAL == 0 => {
            crate::task::raise_current(13);
            Err(EPIPE)
        }
        r => Ok(r? as i64),
    }
}

fn recv(ctx: &Ctx, nr: u64, a: [u64; 6]) -> Result<i64> {
    let f = ctx.fds().lock().get(a[0] as i32)?;
    if let Some(n) = f.node.as_any().downcast_ref::<NetlinkSocket>() {
        // Replies come from the "kernel": sockaddr_nl { AF_NETLINK, pid 0 }.
        let mut nl = [0u8; 12];
        nl[0] = 16;
        let write_nl = |addr: u64, lenp: u64| {
            if addr != 0 {
                let cap = if lenp != 0 {
                    ctx.mm.read_u32(lenp).unwrap_or(0)
                } else {
                    12
                };
                let _ = ctx.mm.write(addr, &nl[..(cap as usize).min(12)]);
                if lenp != 0 {
                    ctx.mm.write_u32(lenp, 12);
                }
            }
        };
        if nr == 207 {
            let flags = a[3] as u32;
            let m = n.pop(f.nonblock() || flags & MSG_DONTWAIT != 0)?;
            let k = m.len().min(a[2] as usize);
            if !ctx.mm.write(a[1], &m[..k]) {
                return Err(EFAULT);
            }
            write_nl(a[4], a[5]);
            return Ok(if flags & MSG_TRUNC != 0 { m.len() } else { k } as i64);
        }
        let flags = a[2] as u32;
        let mut h = [0u8; 56];
        if !ctx.mm.read(a[1], &mut h) {
            return Err(EFAULT);
        }
        let g = |o: usize| u64::from_le_bytes(h[o..o + 8].try_into().unwrap());
        let (name, iov, iovlen) = (g(0), g(16), g(24));
        let m = n.pop(f.nonblock() || flags & MSG_DONTWAIT != 0)?;
        let mut off = 0;
        for i in 0..iovlen.min(1024) {
            if off >= m.len() {
                break;
            }
            let base = ctx.mm.read_u64(iov + i * 16).ok_or(EFAULT)?;
            let len = ctx.mm.read_u64(iov + i * 16 + 8).ok_or(EFAULT)? as usize;
            let k = len.min(m.len() - off);
            if !ctx.mm.write(base, &m[off..off + k]) {
                return Err(EFAULT);
            }
            off += k;
        }
        if name != 0 {
            let cap = u32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
            let _ = ctx.mm.write(name, &nl[..cap.min(12)]);
            ctx.mm.write_u32(a[1] + 8, 12);
        }
        ctx.mm.write_u64(a[1] + 40, 0); // msg_controllen
        let truncated = if off < m.len() { MSG_TRUNC } else { 0 };
        ctx.mm.write_u32(a[1] + 48, truncated);
        return Ok(if flags & MSG_TRUNC != 0 { m.len() } else { off } as i64);
    }
    let (_, s) = sock(ctx, a[0])?;
    if nr == 207 {
        let flags = a[3] as u32;
        let (d, fds, _, from, full) = s.recv(a[2] as usize, flags, f.nonblock())?;
        drop(fds);
        if !ctx.mm.write(a[1], &d) {
            return Err(EFAULT);
        }
        if a[4] != 0 {
            write_addr(ctx, a[4], a[5], from.as_deref());
        }
        return Ok(if flags & MSG_TRUNC != 0 && full > 0 {
            full
        } else {
            d.len()
        } as i64);
    }
    let flags = a[2] as u32;
    let mut h = [0u8; 56];
    if !ctx.mm.read(a[1], &mut h) {
        return Err(EFAULT);
    }
    let g = |o: usize| u64::from_le_bytes(h[o..o + 8].try_into().unwrap());
    let (name, iov, iovlen, control, controllen) = (g(0), g(16), g(24), g(32), g(40));
    let mut iovs = Vec::new();
    let mut cap = 0usize;
    for i in 0..iovlen.min(1024) {
        let base = ctx.mm.read_u64(iov + i * 16).ok_or(EFAULT)?;
        let len = ctx.mm.read_u64(iov + i * 16 + 8).ok_or(EFAULT)? as usize;
        iovs.push((base, len));
        cap += len;
    }
    let _ = MSG_WAITALL;
    let (d, fds, cred, from, full) = s.recv(cap, flags, f.nonblock())?;
    let mut off = 0;
    for (base, len) in iovs {
        if off >= d.len() {
            break;
        }
        let n = len.min(d.len() - off);
        if !ctx.mm.write(base, &d[off..off + n]) {
            return Err(EFAULT);
        }
        off += n;
    }
    // Ancillary data.
    let mut cmsg = Vec::new();
    let mut mflags = 0u32;
    if !fds.is_empty() {
        let table = ctx.fds();
        let mut t = table.lock();
        let mut nums = Vec::new();
        for f in fds {
            nums.push(t.insert(f, flags & MSG_CMSG_CLOEXEC != 0)?);
        }
        let clen = 16 + nums.len() * 4;
        cmsg.extend_from_slice(&(clen as u64).to_le_bytes());
        cmsg.extend_from_slice(&(SOL_SOCKET as u32).to_le_bytes());
        cmsg.extend_from_slice(&SCM_RIGHTS.to_le_bytes());
        for n in nums {
            cmsg.extend_from_slice(&(n as u32).to_le_bytes());
        }
        while cmsg.len() % 8 != 0 {
            cmsg.push(0);
        }
    }
    if *s.passcred.lock() {
        cmsg.extend_from_slice(&28u64.to_le_bytes());
        cmsg.extend_from_slice(&(SOL_SOCKET as u32).to_le_bytes());
        cmsg.extend_from_slice(&SCM_CREDENTIALS.to_le_bytes());
        cmsg.extend_from_slice(&cred.pid.to_le_bytes());
        cmsg.extend_from_slice(&cred.uid.to_le_bytes());
        cmsg.extend_from_slice(&cred.gid.to_le_bytes());
        cmsg.extend_from_slice(&[0u8; 4]);
    }
    if cmsg.len() as u64 > controllen {
        cmsg.truncate(controllen as usize);
        mflags |= 8; // MSG_CTRUNC
    }
    if !cmsg.is_empty() && !ctx.mm.write(control, &cmsg) {
        return Err(EFAULT);
    }
    if full > d.len() {
        mflags |= MSG_TRUNC;
    }
    // msg_controllen, msg_flags
    ctx.mm.write_u64(a[1] + 40, cmsg.len() as u64);
    ctx.mm.write_u32(a[1] + 48, mflags);
    if name != 0 {
        let mut b = vec![1u8, 0];
        if let Some(n) = &from {
            b.extend_from_slice(n.as_bytes());
            b.push(0);
        }
        let ncap = u32::from_le_bytes(h[8..12].try_into().unwrap()) as usize;
        ctx.mm.write(name, &b[..b.len().min(ncap)]);
        ctx.mm.write_u32(a[1] + 8, b.len().min(ncap) as u32);
    }
    Ok(if flags & MSG_TRUNC != 0 && full > 0 {
        full
    } else {
        d.len()
    } as i64)
}

static LOGCAT: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var_os("INDROID_LOGCAT").is_some());

/// Prints a logd write packet (android_log_header_t + prio/tag/msg) to stderr.
fn mirror_log(data: &[u8], pid: i32) {
    if data.len() < 12 {
        return;
    }
    let id = data[0];
    if id == 2 || id == 6 {
        return; // events / security (binary)
    }
    let body = &data[11..];
    let prio = body[0];
    let rest = &body[1..];
    let mut parts = rest.splitn(2, |&b| b == 0);
    let tag = String::from_utf8_lossy(parts.next().unwrap_or_default());
    let msg = String::from_utf8_lossy(parts.next().unwrap_or_default());
    let p = ['?', '?', 'V', 'D', 'I', 'W', 'E', 'F', 'S']
        .get(prio as usize)
        .copied()
        .unwrap_or('?');
    crate::console_line(&format!(
        "[log] {p} {pid} {tag}: {}\n",
        msg.trim_end_matches(['\0', '\n'])
    ));
}
