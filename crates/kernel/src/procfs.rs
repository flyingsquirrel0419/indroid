//! /proc: generated on lookup (file contents are snapshots taken at open).

use crate::errno::*;
use crate::task::{Ctx, Process};
use crate::vfs::*;
use std::fmt::Write;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// A read-only file whose contents were produced at open time.
pub struct TextFile {
    data: Vec<u8>,
    id: u64,
    mode: u32,
}

impl TextFile {
    pub fn new(data: impl Into<Vec<u8>>) -> Arc<TextFile> {
        Arc::new(TextFile {
            data: data.into(),
            id: new_id(),
            mode: S_IFREG | 0o444,
        })
    }
}

impl Node for TextFile {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: self.mode,
            ino: self.id,
            nlink: 1,
            size: 0, // like procfs
            blksize: 1024,
            ..Default::default()
        }
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        let off = off as usize;
        if off >= self.data.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.data.len() - off);
        buf[..n].copy_from_slice(&self.data[off..off + n]);
        Ok(n)
    }
    fn write_at(&self, _off: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        // Writes to /proc knobs (oom_score_adj, sysctls) are accepted.
        Ok(buf.len())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A synthetic directory with a fixed listing.
pub struct SynthDir {
    entries: Vec<(String, u8)>,
    id: u64,
}

impl SynthDir {
    pub fn new(entries: Vec<(String, u8)>) -> Arc<SynthDir> {
        Arc::new(SynthDir {
            entries,
            id: new_id(),
        })
    }
}

impl Node for SynthDir {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFDIR | 0o555,
            ino: self.id,
            nlink: 2,
            blksize: 1024,
            ..Default::default()
        }
    }
    fn readdir(&self) -> Result<Vec<DirEntry>> {
        let mut v = vec![
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
        for (n, k) in &self.entries {
            v.push(DirEntry {
                name: n.as_bytes().to_vec(),
                ino: new_id(),
                kind: *k,
            });
        }
        Ok(v)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct ProcFs;

const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;

fn file(s: impl Into<Vec<u8>>) -> Result<SynthEntry> {
    Ok(SynthEntry::Node(TextFile::new(s)))
}

fn dir(names: &[(&str, u8)]) -> Result<SynthEntry> {
    Ok(SynthEntry::Node(SynthDir::new(
        names.iter().map(|(n, k)| (n.to_string(), *k)).collect(),
    )))
}

/// An openable proc "magic link" (/proc/self/ns/*): readlink shows `target`,
/// open() yields this node.
pub struct MagicLink {
    pub target: String,
    id: u64,
}

impl Node for MagicLink {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFREG | 0o444,
            ino: 4026531840,
            nlink: 1,
            ..Default::default()
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

const PID_ENTRIES: &[(&str, u8)] = &[
    ("ns", DT_DIR),
    ("cmdline", DT_REG),
    ("comm", DT_REG),
    ("cwd", DT_LNK),
    ("environ", DT_REG),
    ("exe", DT_LNK),
    ("fd", DT_DIR),
    ("maps", DT_REG),
    ("smaps", DT_REG),
    ("mounts", DT_REG),
    ("stat", DT_REG),
    ("statm", DT_REG),
    ("status", DT_REG),
    ("task", DT_DIR),
    ("oom_score_adj", DT_REG),
    ("cgroup", DT_REG),
    ("auxv", DT_REG),
    ("attr", DT_DIR),
    ("mountinfo", DT_REG),
];

impl SyntheticFs for ProcFs {
    fn lookup(&self, ctx: &Ctx, rel: &str) -> Result<SynthEntry> {
        let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            let mut names: Vec<(String, u8)> = ctx
                .kernel
                .procs
                .lock()
                .keys()
                .map(|p| (p.to_string(), DT_DIR))
                .collect();
            for n in ["self", "thread-self"] {
                names.push((n.into(), DT_LNK));
            }
            for n in [
                "meminfo",
                "cpuinfo",
                "stat",
                "uptime",
                "version",
                "filesystems",
                "mounts",
                "loadavg",
                "cmdline",
                "sys",
                "net",
                "pressure",
            ] {
                names.push((
                    n.into(),
                    if n == "sys" || n == "net" || n == "pressure" {
                        DT_DIR
                    } else {
                        DT_REG
                    },
                ));
            }
            return Ok(SynthEntry::Node(SynthDir::new(names)));
        }
        match parts[0] {
            "self" => {
                if parts.len() == 1 {
                    return Ok(SynthEntry::Symlink(format!("/proc/{}", ctx.process.pid)));
                }
                return self.pid_entry(ctx, &ctx.process.clone(), ctx.thread.tid, &parts[1..]);
            }
            "thread-self" => {
                return Ok(SynthEntry::Symlink(format!("/proc/{}/task/{}", ctx.process.pid, ctx.thread.tid)));
            }
            "meminfo" => return file(
                "MemTotal:        4028248 kB\nMemFree:         2014124 kB\nMemAvailable:    3014124 kB\nBuffers:           10000 kB\nCached:           500000 kB\nSwapCached:            0 kB\nSwapTotal:             0 kB\nSwapFree:              0 kB\n",
            ),
            "cpuinfo" => {
                let mut s = String::new();
                for i in 0..8 {
                    let _ = write!(s, "processor\t: {i}\nBogoMIPS\t: 48.00\nFeatures\t: fp asimd aes pmull sha1 sha2 crc32 atomics fphp asimdhp asimdrdm lrcpc asimddp\nCPU implementer\t: 0x41\nCPU architecture: 8\nCPU variant\t: 0x0\nCPU part\t: 0xd08\nCPU revision\t: 3\n\n");
                }
                return file(s);
            }
            "stat" => {
                let mut s = String::from("cpu  100 0 100 10000 0 0 0 0 0 0\n");
                for i in 0..8 {
                    let _ = writeln!(s, "cpu{i} 10 0 10 1000 0 0 0 0 0 0");
                }
                let _ = write!(s, "intr 0\nctxt 0\nbtime {}\nprocesses 1\nprocs_running 1\nprocs_blocked 0\n", crate::time::realtime().as_secs() - crate::time::monotonic().as_secs());
                return file(s);
            }
            "uptime" => {
                let u = crate::time::monotonic().as_secs_f64();
                return file(format!("{u:.2} {u:.2}\n"));
            }
            "zoneinfo" => return file("Node 0, zone   Normal\n  pages free     500000\n        min      5000\n        low      6000\n        high     7000\n        spanned  1000000\n        present  1000000\n        managed  1000000\n    nr_free_pages 500000\n  protection: (0, 0, 0)\n"),
            "vmstat" => return file("nr_free_pages 500000\nnr_inactive_file 10000\nnr_active_file 10000\nnr_inactive_anon 10000\nnr_active_anon 10000\npgfault 0\npgmajfault 0\nworkingset_refault_file 0\n"),
            "keys" | "kallsyms" | "modules" | "swaps" | "interrupts" | "softirqs" | "partitions" | "diskstats" | "kmsg" => return file(""),
            "version" => return file("Linux version 5.15.110-indroid (indroid@neo) #1 SMP PREEMPT\n"),
            "loadavg" => return file("0.00 0.00 0.00 1/64 1\n"),
            "cmdline" => return file(format!("{}\n", cmdline())),
            "bootconfig" => return file(bootconfig()),
            "filesystems" => return file("nodev\tsysfs\nnodev\ttmpfs\nnodev\tproc\nnodev\tselinuxfs\n\text4\nnodev\tfuse\n"),
            "mounts" => return file(mounts()),
            "sys" => return self.sys(&parts[1..]),
            "net" | "pressure" => {
                if parts.len() == 1 {
                    return dir(&[]);
                }
                return file("");
            }
            p => {
                if let Ok(pid) = p.parse::<i32>() {
                    let proc_ = ctx.kernel.procs.lock().get(&pid).cloned();
                    if let Some(proc_) = proc_ {
                        return self.pid_entry(ctx, &proc_, pid, &parts[1..]);
                    }
                    // A thread id also resolves (Linux allows /proc/<tid>).
                    let t = ctx.kernel.threads.lock().get(&pid).cloned();
                    if let Some(t) = t {
                        let p = t.process();
                        return self.pid_entry(ctx, &p, pid, &parts[1..]);
                    }
                }
            }
        }
        Err(ENOENT)
    }
}

fn mounts() -> String {
    "/dev/root / ext4 ro,seclabel,relatime 0 0\ntmpfs /dev tmpfs rw,seclabel,nosuid,relatime,mode=755 0 0\nproc /proc proc rw,relatime 0 0\nsysfs /sys sysfs rw,relatime 0 0\ntmpfs /mnt tmpfs rw,relatime 0 0\n/dev/block/vdc /data ext4 rw,seclabel,nosuid,nodev,noatime 0 0\ntmpfs /apex tmpfs rw,relatime 0 0\nselinuxfs /sys/fs/selinux selinuxfs rw,relatime 0 0\n".into()
}

impl ProcFs {
    fn pid_entry(
        &self,
        ctx: &Ctx,
        p: &Arc<Process>,
        tid: i32,
        rest: &[&str],
    ) -> Result<SynthEntry> {
        if rest.is_empty() {
            return dir(PID_ENTRIES);
        }
        match rest[0] {
            "cmdline" => file(p.cmdline.lock().clone()),
            "comm" => file(format!("{}\n", p.comm.lock())),
            "exe" => Ok(SynthEntry::Symlink(p.exe.lock().clone())),
            "cwd" => Ok(SynthEntry::Symlink(p.cwd.lock().clone())),
            "root" => Ok(SynthEntry::Symlink("/".into())),
            "environ" | "auxv" => file(""),
            "oom_score_adj" | "oom_adj" | "timerslack_ns" | "sched_group_id" => file("0\n"),
            "cgroup" => file("0::/\n"),
            "ns" => {
                if rest.len() == 1 {
                    return dir(&[
                        ("mnt", DT_LNK),
                        ("net", DT_LNK),
                        ("pid", DT_LNK),
                        ("uts", DT_LNK),
                        ("ipc", DT_LNK),
                        ("user", DT_LNK),
                        ("cgroup", DT_LNK),
                    ]);
                }
                Ok(SynthEntry::Node(Arc::new(MagicLink {
                    target: format!("{}:[4026531840]", rest[1]),
                    id: new_id(),
                })))
            }
            "mounts" => file(mounts()),
            "mountinfo" => file("1 0 0:1 / / ro - ext4 /dev/root ro\n"),
            "attr" => {
                if rest.len() == 1 {
                    return dir(&[
                        ("current", DT_REG),
                        ("prev", DT_REG),
                        ("exec", DT_REG),
                        ("fscreate", DT_REG),
                        ("keycreate", DT_REG),
                        ("sockcreate", DT_REG),
                    ]);
                }
                Ok(SynthEntry::Node(Arc::new(AttrFile {
                    process: Arc::downgrade(p),
                    which: rest[1].to_string(),
                    id: new_id(),
                })))
            }
            "maps" | "smaps" => {
                let mm = p.mm();
                let mut s = String::new();
                for v in mm.vmas() {
                    let r = if v.prot & 1 != 0 { 'r' } else { '-' };
                    let w = if v.prot & 2 != 0 { 'w' } else { '-' };
                    let x = if v.prot & 4 != 0 { 'x' } else { '-' };
                    let (shared, off) = match &v.backing {
                        crate::mm::Backing::Shared { offset, .. } => ('s', *offset),
                        crate::mm::Backing::File { offset, .. } => ('p', *offset),
                        crate::mm::Backing::Anon => ('p', 0),
                    };
                    let _ = writeln!(
                        s,
                        "{:x}-{:x} {r}{w}{x}{shared} {:08x} fd:00 0 {}",
                        v.start, v.end, off, v.name
                    );
                    if rest[0] == "smaps" {
                        let kb = (v.end - v.start) / 1024;
                        let _ = write!(s, "Size: {kb} kB\nRss: {kb} kB\nPss: {kb} kB\n");
                    }
                }
                file(s)
            }
            "stat" => {
                let comm = p.comm.lock().clone();
                file(format!(
                    "{} ({}) S {} {} {} 0 -1 4194304 0 0 0 0 0 0 0 0 20 0 {} 0 {} 0 0 18446744073709551615 0 0 {} 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0\n",
                    tid,
                    comm,
                    p.ppid.load(Ordering::Relaxed),
                    p.pgid.load(Ordering::Relaxed),
                    p.sid.load(Ordering::Relaxed),
                    p.threads.lock().len(),
                    (p.start_time.elapsed().as_millis() / 10),
                    crate::elf::STACK_TOP - 16
                ))
            }
            "statm" => file("1000 500 100 10 0 400 0\n"),
            "status" => {
                let uid = p.uid.load(Ordering::Relaxed);
                let gid = p.gid.load(Ordering::Relaxed);
                file(format!(
                    "Name:\t{}\nUmask:\t{:04o}\nState:\tS (sleeping)\nTgid:\t{}\nNgid:\t0\nPid:\t{}\nPPid:\t{}\nTracerPid:\t0\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nGid:\t{gid}\t{gid}\t{gid}\t{gid}\nFDSize:\t64\nGroups:\t\nVmPeak:\t  100000 kB\nVmSize:\t  100000 kB\nVmRSS:\t   50000 kB\nThreads:\t{}\nSigQ:\t0/15000\nSigPnd:\t0000000000000000\nShdPnd:\t0000000000000000\nSigBlk:\t0000000000000000\nSigIgn:\t0000000000000000\nSigCgt:\t0000000000000000\nCapInh:\t0000000000000000\nCapPrm:\t000001ffffffffff\nCapEff:\t000001ffffffffff\nCapBnd:\t000001ffffffffff\nSeccomp:\t0\n",
                    p.comm.lock(),
                    p.umask.load(Ordering::Relaxed),
                    p.pid,
                    tid,
                    p.ppid.load(Ordering::Relaxed),
                    p.threads.lock().len()
                ))
            }
            "task" => {
                if rest.len() == 1 {
                    let names: Vec<(String, u8)> = p
                        .threads
                        .lock()
                        .iter()
                        .map(|t| (t.tid.to_string(), DT_DIR))
                        .collect();
                    return Ok(SynthEntry::Node(SynthDir::new(names)));
                }
                let t: i32 = rest[1].parse().map_err(|_| ENOENT)?;
                if !p.threads.lock().iter().any(|x| x.tid == t) {
                    return Err(ENOENT);
                }
                if rest.len() == 2 {
                    return dir(PID_ENTRIES);
                }
                if rest[2] == "comm" {
                    let name = p
                        .threads
                        .lock()
                        .iter()
                        .find(|x| x.tid == t)
                        .map(|x| x.name.lock().clone())
                        .unwrap_or_default();
                    return file(format!("{name}\n"));
                }
                self.pid_entry(ctx, p, t, &rest[2..])
            }
            "fd" => {
                let fds = p.fds();
                let t = fds.lock();
                if rest.len() == 1 {
                    let names: Vec<(String, u8)> =
                        t.fds.keys().map(|k| (k.to_string(), DT_LNK)).collect();
                    return Ok(SynthEntry::Node(SynthDir::new(names)));
                }
                let n: i32 = rest[1].parse().map_err(|_| ENOENT)?;
                let f = t.get(n).map_err(|_| ENOENT)?;
                let target = if f.path.starts_with('/') {
                    f.path.clone()
                } else {
                    format!("{}[{}]", f.path, f.node.id())
                };
                Ok(SynthEntry::Symlink(target))
            }
            _ => Err(ENOENT),
        }
    }

    fn sys(&self, rest: &[&str]) -> Result<SynthEntry> {
        let path = rest.join("/");
        if let Some(v) = SYSCTL.lock().get(&path) {
            return Ok(SynthEntry::Node(Arc::new(Sysctl {
                key: path.clone(),
                snapshot: v.clone(),
                id: new_id(),
            })));
        }
        let v = match path.as_str() {
            "" => {
                return dir(&[
                    ("kernel", DT_DIR),
                    ("vm", DT_DIR),
                    ("fs", DT_DIR),
                    ("net", DT_DIR),
                ])
            }
            "kernel/pid_max" => "4194304\n",
            "kernel/random/boot_id" => "c0ffee00-1d0r-1d00-0000-000000000001\n",
            "kernel/random/uuid" => "c0ffee00-1d0r-1d00-0000-000000000002\n",
            "kernel/osrelease" => "5.15.110-indroid\n",
            "kernel/hostname" => "localhost\n",
            "kernel/randomize_va_space" => "2\n",
            "kernel/perf_event_paranoid" => "3\n",
            "kernel/kptr_restrict" | "kernel/dmesg_restrict" => "1\n",
            "kernel/threads-max" => "60000\n",
            "vm/overcommit_memory" => "1\n",
            "vm/max_map_count" => "65530\n",
            "vm/mmap_min_addr" => "32768\n",
            "fs/pipe-max-size" => "1048576\n",
            "fs/file-max" => "100000\n",
            _ if rest.len() <= 2 && !path.contains('.') && !path.ends_with("_ms") => {
                // Unknown sysctl directories/files: present as empty files so
                // init's "write /proc/sys/..." commands succeed.
                "0\n"
            }
            _ => "0\n",
        };
        if rest.len() >= 2 {
            return Ok(SynthEntry::Node(Arc::new(Sysctl {
                key: path,
                snapshot: v.as_bytes().to_vec(),
                id: new_id(),
            })));
        }
        file(v)
    }
}

/// Values written to /proc/sys (persist for the kernel's lifetime).
static SYSCTL: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashMap<String, Vec<u8>>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

pub struct Sysctl {
    key: String,
    snapshot: Vec<u8>,
    id: u64,
}

impl Node for Sysctl {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFREG | 0o644,
            ino: self.id,
            nlink: 1,
            blksize: 1024,
            ..Default::default()
        }
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        let live = SYSCTL.lock().get(&self.key).cloned();
        let d = live.as_ref().unwrap_or(&self.snapshot);
        let off = off as usize;
        if off >= d.len() {
            return Ok(0);
        }
        let n = buf.len().min(d.len() - off);
        buf[..n].copy_from_slice(&d[off..off + n]);
        Ok(n)
    }
    fn write_at(&self, _off: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        let mut v = buf.to_vec();
        if !v.ends_with(b"\n") {
            v.push(b'\n');
        }
        SYSCTL.lock().insert(self.key.clone(), v);
        Ok(buf.len())
    }
    fn truncate(&self, _len: u64) -> Result<()> {
        Ok(())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// /proc/<pid>/attr/{current,exec,...}: the process's SELinux context.
pub struct AttrFile {
    process: std::sync::Weak<Process>,
    which: String,
    id: u64,
}

impl Node for AttrFile {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFREG | 0o666,
            ino: self.id,
            nlink: 1,
            ..Default::default()
        }
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        let Some(p) = self.process.upgrade() else {
            return Err(ESRCH);
        };
        let v = match self.which.as_str() {
            "current" | "prev" => p.secctx.lock().clone(),
            "exec" => p.exec_secctx.lock().clone().unwrap_or_default(),
            _ => String::new(),
        };
        let mut d = v.into_bytes();
        if !d.is_empty() {
            d.push(0);
        }
        let off = off as usize;
        if off >= d.len() {
            return Ok(0);
        }
        let n = buf.len().min(d.len() - off);
        buf[..n].copy_from_slice(&d[off..off + n]);
        Ok(n)
    }
    fn write_at(&self, _off: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        let Some(p) = self.process.upgrade() else {
            return Err(ESRCH);
        };
        let v = String::from_utf8_lossy(buf)
            .trim_end_matches(['\0', '\n'])
            .to_string();
        match self.which.as_str() {
            "current" => *p.secctx.lock() = v,
            "exec" => *p.exec_secctx.lock() = (!v.is_empty()).then_some(v),
            _ => {}
        }
        Ok(buf.len())
    }
    fn truncate(&self, _len: u64) -> Result<()> {
        Ok(())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Kernel command line (androidboot.* become ro.boot.* properties).
/// INDROID_CMDLINE overrides the default (emulator ROM) settings.
pub fn cmdline() -> String {
    std::env::var("INDROID_CMDLINE").unwrap_or_else(|_| "androidboot.hardware=ranchu androidboot.selinux=permissive androidboot.qemu=1 qemu=1 androidboot.hardwareegl=angle androidboot.hardware.gltransport=virtio-gpu-asg".trim().to_string())
}

/// /proc/bootconfig: the androidboot.* parameters in bootconfig syntax.
fn bootconfig() -> String {
    let mut out = String::new();
    for kv in cmdline().split_whitespace() {
        if let Some((k, v)) = kv.split_once('=') {
            if k.starts_with("androidboot.") {
                out.push_str(&format!("{k} = \"{v}\"\n"));
            }
        }
    }
    out
}
