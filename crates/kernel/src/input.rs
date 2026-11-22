//! Input devices (/dev/input/eventN, evdev protocol) fed by the host, and a
//! minimal inotify.

use crate::errno::*;
use crate::sync::WaitQueue;
use crate::task::Ctx;
use crate::vfs::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_ABS: u16 = 3;
pub const SYN_REPORT: u16 = 0;
pub const BTN_TOUCH: u16 = 0x14a;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
pub const ABS_MT_PRESSURE: u16 = 0x3a;
pub const KEY_BACK: u16 = 158;
pub const KEY_HOMEPAGE: u16 = 172;
pub const KEY_APPSELECT: u16 = 0x244;
pub const KEY_POWER: u16 = 116;
pub const KEY_VOLUMEDOWN: u16 = 114;
pub const KEY_VOLUMEUP: u16 = 115;

#[derive(Clone, Copy)]
pub enum DeviceKind {
    Touchscreen { width: i32, height: i32 },
    Keys,
}

/// Shared state of one input device; every open() gets a reader with its own
/// queue (evdev clients each see all events).
pub struct InputDevice {
    pub name: String,
    pub kind: DeviceKind,
    readers: Mutex<Vec<std::sync::Weak<InputReader>>>,
    pub id: u64,
}

impl InputDevice {
    pub fn new(name: &str, kind: DeviceKind) -> Arc<InputDevice> {
        Arc::new(InputDevice {
            name: name.into(),
            kind,
            readers: Mutex::new(Vec::new()),
            id: new_id(),
        })
    }

    pub fn open(self: &Arc<Self>) -> Arc<InputReader> {
        let r = Arc::new(InputReader {
            dev: self.clone(),
            queue: Mutex::new(VecDeque::new()),
            wq: WaitQueue::new(),
            id: new_id(),
        });
        let mut rs = self.readers.lock();
        rs.retain(|w| w.strong_count() > 0);
        rs.push(Arc::downgrade(&r));
        r
    }

    /// Queues events (type, code, value) followed by SYN_REPORT.
    pub fn emit(&self, events: &[(u16, u16, i32)]) {
        let t = crate::time::monotonic();
        let mut packet = Vec::with_capacity((events.len() + 1) * 24);
        for &(ty, code, value) in events
            .iter()
            .chain(std::iter::once(&(EV_SYN, SYN_REPORT, 0)))
        {
            packet.extend_from_slice(&t.as_secs().to_le_bytes());
            packet.extend_from_slice(&(t.subsec_micros() as u64).to_le_bytes());
            packet.extend_from_slice(&ty.to_le_bytes());
            packet.extend_from_slice(&code.to_le_bytes());
            packet.extend_from_slice(&value.to_le_bytes());
        }
        for r in self.readers.lock().iter().filter_map(|w| w.upgrade()) {
            r.queue.lock().extend(packet.iter().copied());
            r.wq.notify();
        }
    }

    /// Touch helper: `id` < 0 lifts the finger in `slot`.
    pub fn touch(&self, slot: i32, id: i32, x: i32, y: i32) {
        if id < 0 {
            self.emit(&[
                (EV_ABS, ABS_MT_SLOT, slot),
                (EV_ABS, ABS_MT_TRACKING_ID, -1),
                (EV_KEY, BTN_TOUCH, 0),
            ]);
        } else {
            self.emit(&[
                (EV_ABS, ABS_MT_SLOT, slot),
                (EV_ABS, ABS_MT_TRACKING_ID, id),
                (EV_ABS, ABS_MT_POSITION_X, x),
                (EV_ABS, ABS_MT_POSITION_Y, y),
                (EV_ABS, ABS_MT_PRESSURE, 50),
                (EV_ABS, ABS_MT_TOUCH_MAJOR, 5),
                (EV_KEY, BTN_TOUCH, 1),
            ]);
        }
    }

    pub fn key(&self, code: u16, down: bool) {
        self.emit(&[(EV_KEY, code, down as i32)]);
    }
}

pub struct InputReader {
    dev: Arc<InputDevice>,
    queue: Mutex<VecDeque<u8>>,
    wq: Arc<WaitQueue>,
    id: u64,
}

fn set_bits(bits: &[u16], len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len];
    for &b in bits {
        let i = b as usize / 8;
        if i < len {
            v[i] |= 1 << (b % 8);
        }
    }
    v
}

impl Node for InputReader {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFCHR | 0o660,
            ino: self.dev.id,
            rdev: 0x0d40,
            nlink: 1,
            ..Default::default()
        }
    }
    fn is_stream(&self) -> bool {
        true
    }
    fn read_at(&self, _o: u64, buf: &mut [u8], nonblock: bool) -> Result<usize> {
        if buf.len() < 24 {
            return Err(EINVAL);
        }
        loop {
            let seen = self.wq.epoch();
            {
                let mut q = self.queue.lock();
                if q.len() >= 24 {
                    let n = (buf.len() / 24 * 24).min(q.len() / 24 * 24);
                    for (i, b) in q.drain(..n).enumerate() {
                        buf[i] = b;
                    }
                    return Ok(n);
                }
            }
            if nonblock {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            self.wq.wait_change(seen, None);
        }
    }
    fn write_at(&self, _o: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        Ok(buf.len())
    }
    fn poll(&self) -> u32 {
        if self.queue.lock().len() >= 24 {
            POLLIN
        } else {
            0
        }
    }
    fn wait_queue(&self) -> Option<Arc<WaitQueue>> {
        Some(self.wq.clone())
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> Result<i64> {
        let ty = (cmd >> 8) & 0xff;
        let nr = cmd & 0xff;
        let size = ((cmd >> 16) & 0x3fff) as usize;
        if ty != b'E' as u32 {
            return Err(ENOTTY);
        }
        let put = |data: &[u8]| -> Result<i64> {
            let n = data.len().min(size);
            ctx.mm
                .write(arg, &data[..n])
                .then_some(n as i64)
                .ok_or(EFAULT)
        };
        let touch = matches!(self.dev.kind, DeviceKind::Touchscreen { .. });
        match nr {
            0x01 => put(&0x010001i32.to_le_bytes()), // EVIOCGVERSION
            0x02 => {
                // EVIOCGID: bustype, vendor, product, version
                let id: [u16; 4] = [0x06, 0x18d1, if touch { 0x4ee1 } else { 0x4ee2 }, 1];
                put(&id.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
            }
            0x03 | 0x04 => Ok(0), // EVIOCGREP / EVIOCSREP
            0x06 => put(&[self.dev.name.as_bytes(), b"\0"].concat()), // EVIOCGNAME
            0x07 => put(b"indroid/input0\0"), // EVIOCGPHYS
            0x08 => put(b"\0"),   // EVIOCGUNIQ
            0x09 => put(&set_bits(if touch { &[1] } else { &[] }, size)), // EVIOCGPROP: INPUT_PROP_DIRECT
            0x18 | 0x19 | 0x1a | 0x1b => put(&vec![0u8; size]), // EVIOCGKEY/LED/SND/SW state
            0x20 => {
                // EVIOCGBIT(0): supported event types
                put(&set_bits(
                    if touch {
                        &[EV_SYN, EV_KEY, EV_ABS]
                    } else {
                        &[EV_SYN, EV_KEY]
                    },
                    size,
                ))
            }
            0x21 => {
                // EVIOCGBIT(EV_KEY)
                let keys: &[u16] = if touch {
                    &[BTN_TOUCH]
                } else {
                    &[
                        KEY_BACK,
                        KEY_HOMEPAGE,
                        KEY_APPSELECT,
                        KEY_POWER,
                        KEY_VOLUMEDOWN,
                        KEY_VOLUMEUP,
                    ]
                };
                put(&set_bits(keys, size))
            }
            0x23 => {
                // EVIOCGBIT(EV_ABS)
                let abs: &[u16] = if touch {
                    &[
                        ABS_MT_SLOT,
                        ABS_MT_TOUCH_MAJOR,
                        ABS_MT_POSITION_X,
                        ABS_MT_POSITION_Y,
                        ABS_MT_TRACKING_ID,
                        ABS_MT_PRESSURE,
                    ]
                } else {
                    &[]
                };
                put(&set_bits(abs, size))
            }
            0x22 | 0x24..=0x3f => put(&vec![0u8; size]), // other EVIOCGBIT types
            0x40..=0x7f if cmd >> 30 == 2 => {
                // EVIOCGABS(abs): value, min, max, fuzz, flat, resolution
                let abs = (nr - 0x40) as u16;
                let (w, h) = match self.dev.kind {
                    DeviceKind::Touchscreen { width, height } => (width, height),
                    _ => (0, 0),
                };
                let (min, max) = match abs {
                    ABS_MT_SLOT => (0, 9),
                    ABS_MT_POSITION_X => (0, w - 1),
                    ABS_MT_POSITION_Y => (0, h - 1),
                    ABS_MT_TRACKING_ID => (0, 65535),
                    ABS_MT_PRESSURE => (0, 255),
                    ABS_MT_TOUCH_MAJOR => (0, 255),
                    _ => (0, 0),
                };
                let v: [i32; 6] = [0, min, max, 0, 0, 0];
                put(&v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>())
            }
            0x90 | 0xa0 | 0x91 => Ok(0), // EVIOCGRAB, EVIOCSCLOCKID, EVIOCREVOKE
            _ => Ok(0),
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// inotify: watches are accepted; events are only produced for files created
/// through this kernel in watched host directories (see `notify_create`).
pub struct Inotify {
    pub watches: Mutex<Vec<(i32, String, u32)>>,
    queue: Mutex<VecDeque<u8>>,
    next: Mutex<i32>,
    wq: Arc<WaitQueue>,
    id: u64,
}

pub static INOTIFIES: std::sync::LazyLock<Mutex<Vec<std::sync::Weak<Inotify>>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

impl Inotify {
    pub fn new() -> Arc<Inotify> {
        let i = Arc::new(Inotify {
            watches: Mutex::new(Vec::new()),
            queue: Mutex::new(VecDeque::new()),
            next: Mutex::new(1),
            wq: WaitQueue::new(),
            id: new_id(),
        });
        let mut all = INOTIFIES.lock();
        all.retain(|w| w.strong_count() > 0);
        all.push(Arc::downgrade(&i));
        i
    }
    pub fn add_watch(&self, path: &str, mask: u32) -> i32 {
        let mut w = self.watches.lock();
        if let Some(e) = w.iter_mut().find(|e| e.1 == path) {
            e.2 = mask;
            return e.0;
        }
        let mut n = self.next.lock();
        let wd = *n;
        *n += 1;
        w.push((wd, path.to_string(), mask));
        wd
    }
    pub fn rm_watch(&self, wd: i32) -> bool {
        let mut w = self.watches.lock();
        let before = w.len();
        w.retain(|e| e.0 != wd);
        w.len() != before
    }
}

/// Reports `mask` (IN_CREATE=0x100, IN_DELETE=0x200, ...) for `name` in
/// guest directory `dir` to all interested inotify instances.
pub fn notify(dir: &str, name: &str, mask: u32) {
    for i in INOTIFIES.lock().iter().filter_map(|w| w.upgrade()) {
        let wds: Vec<i32> = i
            .watches
            .lock()
            .iter()
            .filter(|e| e.1 == dir && e.2 & mask != 0)
            .map(|e| e.0)
            .collect();
        for wd in wds {
            let mut nm = name.as_bytes().to_vec();
            nm.push(0);
            while nm.len() % 16 != 0 {
                nm.push(0);
            }
            let mut ev = Vec::new();
            ev.extend_from_slice(&wd.to_le_bytes());
            ev.extend_from_slice(&mask.to_le_bytes());
            ev.extend_from_slice(&0u32.to_le_bytes());
            ev.extend_from_slice(&(nm.len() as u32).to_le_bytes());
            ev.extend_from_slice(&nm);
            i.queue.lock().extend(ev);
            i.wq.notify();
        }
    }
}

impl Node for Inotify {
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
        loop {
            let seen = self.wq.epoch();
            {
                let mut q = self.queue.lock();
                if !q.is_empty() {
                    // Return whole events only.
                    let mut n = 0;
                    let v: Vec<u8> = q.iter().copied().collect();
                    while n + 16 <= v.len() {
                        let len =
                            u32::from_le_bytes(v[n + 12..n + 16].try_into().unwrap()) as usize;
                        if n + 16 + len > buf.len() {
                            break;
                        }
                        n += 16 + len;
                    }
                    if n == 0 {
                        return Err(EINVAL);
                    }
                    buf[..n].copy_from_slice(&v[..n]);
                    q.drain(..n);
                    return Ok(n);
                }
            }
            if nonblock {
                return Err(EAGAIN);
            }
            if crate::task::current_signal_pending() {
                return Err(EINTR);
            }
            self.wq.wait_change(seen, None);
        }
    }
    fn poll(&self) -> u32 {
        if self.queue.lock().is_empty() {
            0
        } else {
            POLLIN
        }
    }
    fn wait_queue(&self) -> Option<Arc<WaitQueue>> {
        Some(self.wq.clone())
    }
    fn ioctl(&self, ctx: &Ctx, cmd: u32, arg: u64) -> Result<i64> {
        if cmd == 0x541b {
            let n = self.queue.lock().len() as u32;
            return ctx.mm.write_u32(arg, n).then_some(0).ok_or(EFAULT);
        }
        Err(ENOTTY)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
