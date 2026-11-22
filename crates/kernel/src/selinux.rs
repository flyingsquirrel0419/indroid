//! SELinux, permissive: a synthetic selinuxfs that allows everything, file
//! labels computed from the ROM's file_contexts (so init's domain transitions
//! and restorecon behave), and per-process contexts.

use crate::errno::*;
use crate::procfs::{SynthDir, TextFile};
use crate::task::Ctx;
use crate::vfs::*;
use parking_lot::Mutex;
use std::sync::{Arc, OnceLock};

struct Spec {
    re: Option<regex::Regex>,
    exact: Option<String>,
    /// File type restriction ("--" regular, "-d" dir, ...); None = any.
    ftype: Option<String>,
    context: String,
}

pub struct FileContexts {
    specs: Vec<Spec>,
}

static CONTEXTS: OnceLock<FileContexts> = OnceLock::new();

fn has_meta(s: &str) -> bool {
    s.chars().any(|c| ".^$?*+|[({\\".contains(c))
}

impl FileContexts {
    fn load(rom: &std::path::Path) -> FileContexts {
        let mut specs = Vec::new();
        for f in [
            "system/etc/selinux/plat_file_contexts",
            "system_ext/etc/selinux/system_ext_file_contexts",
            "product/etc/selinux/product_file_contexts",
            "vendor/etc/selinux/vendor_file_contexts",
            "odm/etc/selinux/odm_file_contexts",
        ] {
            let Ok(text) = std::fs::read_to_string(rom.join(f)) else {
                continue;
            };
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                let (path, ftype, ctx) = match parts.len() {
                    2 => (parts[0], None, parts[1]),
                    3 => (parts[0], Some(parts[1].to_string()), parts[2]),
                    _ => continue,
                };
                if has_meta(path) {
                    if let Ok(re) = regex::Regex::new(&format!("^(?:{path})$")) {
                        specs.push(Spec {
                            re: Some(re),
                            exact: None,
                            ftype,
                            context: ctx.to_string(),
                        });
                    }
                } else {
                    specs.push(Spec {
                        re: None,
                        exact: Some(path.to_string()),
                        ftype,
                        context: ctx.to_string(),
                    });
                }
            }
        }
        FileContexts { specs }
    }

    /// Label for a guest path (`mode` = S_IF* type bits).
    pub fn lookup(&self, path: &str, mode: u32) -> String {
        let ft = |s: &Spec| -> bool {
            match s.ftype.as_deref() {
                None => true,
                Some("--") => mode & S_IFMT == S_IFREG,
                Some("-d") => mode & S_IFMT == S_IFDIR,
                Some("-l") => mode & S_IFMT == S_IFLNK,
                Some("-c") => mode & S_IFMT == S_IFCHR,
                Some("-b") => mode & S_IFMT == S_IFBLK,
                Some("-s") => mode & S_IFMT == S_IFSOCK,
                Some("-p") => mode & S_IFMT == S_IFIFO,
                _ => true,
            }
        };
        // Exact paths win; otherwise the last matching regex in file order.
        if let Some(s) = self
            .specs
            .iter()
            .rev()
            .find(|s| s.exact.as_deref() == Some(path) && ft(s))
        {
            return s.context.clone();
        }
        if let Some(s) = self
            .specs
            .iter()
            .rev()
            .find(|s| s.re.as_ref().is_some_and(|r| r.is_match(path)) && ft(s))
        {
            return s.context.clone();
        }
        "u:object_r:unlabeled:s0".into()
    }
}

pub fn init(rom: &std::path::Path) {
    let rom = rom.to_owned();
    std::thread::spawn(move || {
        CONTEXTS.get_or_init(|| FileContexts::load(&rom));
    });
}

/// Labels recorded from image xattrs (APEX payloads), see tools/apex-labels.py.
static XATTR_LABELS: OnceLock<std::collections::HashMap<String, String>> = OnceLock::new();

pub fn file_label(rom: &std::path::Path, path: &str, mode: u32) -> String {
    let x = XATTR_LABELS.get_or_init(|| {
        std::fs::read_to_string(rom.join(".indroid-labels"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(p, l)| (p.to_string(), l.to_string()))
            .collect()
    });
    if let Some(l) = x.get(path) {
        return l.clone();
    }
    CONTEXTS
        .get_or_init(|| FileContexts::load(rom))
        .lookup(path, mode)
}

/// Domain a process enters when executing a file labeled `tcon` from domain
/// `scon` (type_transition rules follow the foo_exec -> foo convention).
pub fn compute_create(scon: &str, tcon: &str, class: &str) -> String {
    let ty = tcon.split(':').nth(2).unwrap_or("");
    if class == "process" {
        if let Some(d) = ty.strip_suffix("_exec") {
            let d = match d {
                "init" => "init",
                "shell" | "toolbox" | "system" => "shell",
                "zygote" => "zygote",
                x => x,
            };
            return format!("u:r:{d}:s0");
        }
        // Executable without a domain of its own: keep the caller's.
        return scon.to_string();
    }
    tcon.to_string()
}

// ---------------------------------------------------------------------------
// selinuxfs

/// Write-then-read transaction files (access, create, context, ...).
pub struct TxFile {
    kind: &'static str,
    resp: Mutex<Vec<u8>>,
    id: u64,
}

impl Node for TxFile {
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
    fn write_at(&self, _off: u64, buf: &[u8], _nb: bool) -> Result<usize> {
        let s = String::from_utf8_lossy(buf);
        let s = s.trim_end_matches('\0').trim();
        let parts: Vec<&str> = s.split_whitespace().collect();
        let resp = match self.kind {
            // allowed decided auditallow auditdeny seqno flags
            "access" => "ffffffff ffffffff 0 ffffffff 1 0".to_string(),
            "create" => {
                let class = parts
                    .get(2)
                    .and_then(|c| c.parse::<u32>().ok())
                    .map(class_name)
                    .unwrap_or_default();
                compute_create(
                    parts.first().copied().unwrap_or(""),
                    parts.get(1).copied().unwrap_or(""),
                    &class,
                )
            }
            "relabel" | "member" => parts.get(1).copied().unwrap_or("").to_string(),
            "context" => s.to_string(),
            "user" => format!("1 {}", parts.first().map(|_| "u:r:kernel:s0").unwrap_or("")),
            _ => String::new(),
        };
        let mut r = resp.into_bytes();
        r.push(0);
        *self.resp.lock() = r;
        Ok(buf.len())
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        let r = self.resp.lock();
        let off = off as usize;
        if off >= r.len() {
            return Ok(0);
        }
        let n = buf.len().min(r.len() - off);
        buf[..n].copy_from_slice(&r[off..off + n]);
        Ok(n)
    }
    fn is_stream(&self) -> bool {
        true
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Classes libselinux may ask for; index = position + 1.
const CLASSES: &[&str] = &[
    "security",
    "process",
    "system",
    "capability",
    "filesystem",
    "file",
    "dir",
    "fd",
    "lnk_file",
    "chr_file",
    "blk_file",
    "sock_file",
    "fifo_file",
    "socket",
    "tcp_socket",
    "udp_socket",
    "rawip_socket",
    "node",
    "netif",
    "netlink_socket",
    "packet_socket",
    "key_socket",
    "unix_stream_socket",
    "unix_dgram_socket",
    "sem",
    "msg",
    "msgq",
    "shm",
    "ipc",
    "binder",
    "property_service",
    "service_manager",
    "hwservice_manager",
    "keystore_key",
    "drmservice",
    "zygote",
    "debuggerd",
    "keystore2",
    "keystore2_key",
    "fscrypt",
    "netlink_kobject_uevent_socket",
    "capability2",
    "bpf",
    "perf_event",
    "lockdown",
    "anon_inode",
    "io_uring",
    "user_namespace",
    "cap_userns",
    "cap2_userns",
];

fn class_name(idx: u32) -> String {
    CLASSES
        .get(idx.wrapping_sub(1) as usize)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn class_index(name: &str) -> u32 {
    CLASSES
        .iter()
        .position(|c| *c == name)
        .map(|i| i as u32 + 1)
        .unwrap_or_else(|| {
            // Unknown class: stable pseudo index beyond the table.
            100 + name
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
                % 1000
        })
}

/// The selinux_kernel_status page (mapped by libselinux's avc).
pub struct StatusPage {
    mem: Arc<crate::mm::SharedMem>,
    id: u64,
}

impl StatusPage {
    fn new() -> Arc<StatusPage> {
        let mem = crate::mm::SharedMem::new(4096);
        if let Some(p) = mem.page(0) {
            let b = p.bytes_mut();
            b[0..4].copy_from_slice(&1u32.to_le_bytes()); // version
            b[4..8].copy_from_slice(&0u32.to_le_bytes()); // sequence
            b[8..12].copy_from_slice(&0u32.to_le_bytes()); // enforcing
            b[12..16].copy_from_slice(&1u32.to_le_bytes()); // policyload
            b[16..20].copy_from_slice(&0u32.to_le_bytes()); // deny_unknown
        }
        Arc::new(StatusPage { mem, id: new_id() })
    }
}

impl Node for StatusPage {
    fn id(&self) -> u64 {
        self.id
    }
    fn stat(&self) -> Stat {
        Stat {
            mode: S_IFREG | 0o444,
            ino: self.id,
            size: 20,
            nlink: 1,
            ..Default::default()
        }
    }
    fn read_at(&self, off: u64, buf: &mut [u8], _nb: bool) -> Result<usize> {
        let p = self.mem.page(0).ok_or(EIO)?;
        let off = off as usize;
        if off >= 20 {
            return Ok(0);
        }
        let n = buf.len().min(20 - off);
        buf[..n].copy_from_slice(&p.bytes()[off..off + n]);
        Ok(n)
    }
    fn shared_mem(&self) -> Option<Arc<crate::mm::SharedMem>> {
        Some(self.mem.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct SelinuxFs {
    status: Arc<StatusPage>,
}

impl SelinuxFs {
    pub fn new() -> SelinuxFs {
        SelinuxFs {
            status: StatusPage::new(),
        }
    }
}

impl SyntheticFs for SelinuxFs {
    fn lookup(&self, _ctx: &Ctx, rel: &str) -> Result<SynthEntry> {
        let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
        let f = |s: &str| Ok(SynthEntry::Node(TextFile::new(s.as_bytes().to_vec())));
        match parts.as_slice() {
            [] => Ok(SynthEntry::Node(SynthDir::new(
                [
                    "enforce",
                    "mls",
                    "policyvers",
                    "status",
                    "access",
                    "create",
                    "relabel",
                    "member",
                    "context",
                    "user",
                    "checkreqprot",
                    "deny_unknown",
                    "reject_unknown",
                    "class",
                    "null",
                    "policy",
                    "load",
                    "booleans",
                    "initial_contexts",
                ]
                .iter()
                .map(|n| {
                    (
                        n.to_string(),
                        if *n == "class" || *n == "booleans" || *n == "initial_contexts" {
                            4
                        } else {
                            8
                        },
                    )
                })
                .collect(),
            ))),
            ["enforce"] => f("0"),
            ["mls"] => f("1"),
            ["policyvers"] => f("33"),
            ["checkreqprot"] | ["deny_unknown"] | ["reject_unknown"] => f("0"),
            ["status"] => Ok(SynthEntry::Node(self.status.clone())),
            [k @ ("access" | "create" | "relabel" | "member" | "context" | "user")] => {
                let kind: &'static str = match *k {
                    "access" => "access",
                    "create" => "create",
                    "relabel" => "relabel",
                    "member" => "member",
                    "context" => "context",
                    _ => "user",
                };
                Ok(SynthEntry::Node(Arc::new(TxFile {
                    kind,
                    resp: Mutex::new(Vec::new()),
                    id: new_id(),
                })))
            }
            ["null"] => Ok(SynthEntry::Node(crate::fd::NullDev::new(0))),
            ["load"] | ["policy"] => f(""),
            ["booleans"] => Ok(SynthEntry::Node(SynthDir::new(vec![]))),
            ["initial_contexts"] => Ok(SynthEntry::Node(SynthDir::new(vec![("kernel".into(), 8)]))),
            ["initial_contexts", _] => f("u:r:kernel:s0"),
            ["class"] => Ok(SynthEntry::Node(SynthDir::new(
                CLASSES.iter().map(|c| (c.to_string(), 4)).collect(),
            ))),
            ["class", _] => Ok(SynthEntry::Node(SynthDir::new(vec![
                ("index".into(), 8),
                ("perms".into(), 4),
            ]))),
            ["class", c, "index"] => f(&class_index(c).to_string()),
            ["class", c, "perms"] => Ok(SynthEntry::Node(SynthDir::new(
                perms_of(c).iter().map(|p| (p.to_string(), 8)).collect(),
            ))),
            ["class", c, "perms", p] => match perms_of(c).iter().position(|x| x == p) {
                Some(i) => f(&(i + 1).to_string()),
                None => Err(ENOENT),
            },
            _ => Err(ENOENT),
        }
    }
}

/// Permission names per class (bit index = position + 1). Everything is
/// allowed anyway; libselinux only needs the names to exist.
fn perms_of(class: &str) -> &'static [&'static str] {
    const FILE: &[&str] = &[
        "ioctl",
        "read",
        "write",
        "create",
        "getattr",
        "setattr",
        "lock",
        "relabelfrom",
        "relabelto",
        "append",
        "map",
        "unlink",
        "link",
        "rename",
        "execute",
        "quotaon",
        "mounton",
        "audit_access",
        "open",
        "execmod",
        "watch",
        "watch_mount",
        "watch_sb",
        "watch_with_perm",
        "watch_reads",
        "execute_no_trans",
        "entrypoint",
        "search",
        "add_name",
        "remove_name",
        "reparent",
        "rmdir",
    ];
    const PROCESS: &[&str] = &[
        "fork",
        "transition",
        "sigchld",
        "sigkill",
        "sigstop",
        "signull",
        "signal",
        "ptrace",
        "getsched",
        "setsched",
        "getsession",
        "getpgid",
        "setpgid",
        "getcap",
        "setcap",
        "share",
        "getattr",
        "setexec",
        "setfscreate",
        "noatsecure",
        "siginh",
        "setrlimit",
        "rlimitinh",
        "dyntransition",
        "setcurrent",
        "execmem",
        "execstack",
        "execheap",
        "setkeycreate",
        "setsockcreate",
        "getrlimit",
    ];
    const SOCKET: &[&str] = &[
        "ioctl",
        "read",
        "write",
        "create",
        "getattr",
        "setattr",
        "lock",
        "relabelfrom",
        "relabelto",
        "append",
        "map",
        "bind",
        "connect",
        "listen",
        "accept",
        "getopt",
        "setopt",
        "shutdown",
        "recvfrom",
        "sendto",
        "name_bind",
        "connectto",
        "nlmsg_read",
        "nlmsg_write",
    ];
    match class {
        "property_service" => &["set"],
        "service_manager" | "hwservice_manager" => &["add", "find", "list"],
        "binder" => &["impersonate", "call", "set_context_mgr", "transfer"],
        "process" | "process2" => PROCESS,
        "zygote" => &[
            "specifyids",
            "specifyrlimits",
            "specifycapabilities",
            "specifyinvokewith",
            "specifyseinfo",
            "specifymounts",
        ],
        "drmservice" => &[
            "consumeRights",
            "setPlaybackStatus",
            "openDecryptSession",
            "closeDecryptSession",
            "initializeDecryptUnit",
            "decrypt",
            "finalizeDecryptUnit",
            "pread",
        ],
        "keystore_key" | "keystore2" | "keystore2_key" => &[
            "get_state",
            "get",
            "insert",
            "delete",
            "exist",
            "list",
            "reset",
            "password",
            "lock",
            "unlock",
            "is_empty",
            "sign",
            "verify",
            "grant",
            "duplicate",
            "clear_uid",
            "add_auth",
            "user_changed",
            "gen_unique_id",
            "rebind",
            "manage_blob",
            "use",
            "use_dev_id",
            "req_forced_op",
            "convert_storage_key_to_ephemeral",
            "get_info",
            "update",
            "add_auth",
            "change_password",
            "change_user",
            "clear_ns",
            "early_boot_ended",
        ],
        "security" => &[
            "compute_av",
            "compute_create",
            "compute_member",
            "check_context",
            "load_policy",
            "compute_relabel",
            "compute_user",
            "setenforce",
            "setbool",
            "setsecparam",
            "setcheckreqprot",
            "read_policy",
            "validate_trans",
        ],
        "capability" | "capability2" | "cap_userns" | "cap2_userns" => &[
            "chown",
            "dac_override",
            "dac_read_search",
            "fowner",
            "fsetid",
            "kill",
            "setgid",
            "setuid",
            "setpcap",
            "linux_immutable",
            "net_bind_service",
            "net_broadcast",
            "net_admin",
            "net_raw",
            "ipc_lock",
            "ipc_owner",
            "sys_module",
            "sys_rawio",
            "sys_chroot",
            "sys_ptrace",
            "sys_pacct",
            "sys_admin",
            "sys_boot",
            "sys_nice",
            "sys_resource",
            "sys_time",
            "sys_tty_config",
            "mknod",
            "lease",
            "audit_write",
            "audit_control",
            "setfcap",
        ],
        c if c.ends_with("socket") => SOCKET,
        _ => FILE,
    }
}
