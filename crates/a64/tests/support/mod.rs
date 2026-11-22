//! Test-only mini "kernel": loads a *static* aarch64 Linux ELF and implements
//! just enough syscalls for glibc/musl startup and stdout. The real kernel
//! emulation lives in crates/kernel; this exists so the CPU can be tested on
//! its own (differentially against qemu-aarch64-static).
#![allow(dead_code)]

use indroid_a64::{Access, AddressSpace, Cpu, Exit, PAGE_SIZE};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// Page contents and protection bits, by page number.
type PageMap = HashMap<u64, (Box<[u8; 4096]>, u8)>;

pub struct Pages {
    map: RefCell<PageMap>,
    gen: Cell<u64>,
    aot: Cell<Option<(u64, indroid_a64::mem::AotUnit)>>,
}

const R: u8 = 1;
const W: u8 = 2;
const X: u8 = 4;

impl Pages {
    pub fn new() -> Pages {
        Pages {
            map: RefCell::new(HashMap::new()),
            gen: Cell::new(0),
            aot: Cell::new(None),
        }
    }
    pub fn map(&self, addr: u64, len: u64, prot: u8) {
        let mut m = self.map.borrow_mut();
        let start = addr & !(PAGE_SIZE - 1);
        let end = (addr + len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let mut p = start;
        while p < end {
            m.entry(p >> 12)
                .or_insert_with(|| (Box::new([0; 4096]), prot))
                .1 = prot;
            p += PAGE_SIZE;
        }
        self.gen.set(self.gen.get() + 1);
    }
    pub fn set_aot(&self, page: u64, unit: indroid_a64::mem::AotUnit) {
        self.aot.set(Some((page, unit)));
        self.gen.set(self.gen.get() + 1);
    }
    fn unmap(&self, addr: u64, len: u64) {
        let mut m = self.map.borrow_mut();
        let mut p = addr & !(PAGE_SIZE - 1);
        while p < addr + len {
            m.remove(&(p >> 12));
            p += PAGE_SIZE;
        }
        self.gen.set(self.gen.get() + 1);
    }
    pub fn write(&self, addr: u64, data: &[u8]) {
        let mut m = self.map.borrow_mut();
        for (i, b) in data.iter().enumerate() {
            let a = addr + i as u64;
            m.get_mut(&(a >> 12)).expect("write to unmapped").0[(a & 0xfff) as usize] = *b;
        }
    }
    pub fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        let m = self.map.borrow();
        (0..len as u64)
            .map(|i| {
                let a = addr + i;
                m.get(&(a >> 12)).map(|p| p.0[(a & 0xfff) as usize])
            })
            .collect()
    }
}

impl AddressSpace for Pages {
    fn translate(&self, page: u64, access: Access) -> Option<*mut u8> {
        let mut m = self.map.borrow_mut();
        let e = m.get_mut(&page)?;
        let need = match access {
            Access::Read => R,
            Access::Write => W,
            Access::Exec => X,
        };
        if e.1 & need == 0 {
            return None;
        }
        Some(e.0.as_mut_ptr())
    }
    fn generation(&self) -> u64 {
        self.gen.get()
    }
    fn aot_unit(&self, page: u64) -> Option<indroid_a64::mem::AotUnit> {
        self.aot
            .get()
            .and_then(|(p, unit)| (p == page).then_some(unit))
    }
}

pub struct Outcome {
    pub stdout: Vec<u8>,
    pub exit_code: i32,
    pub instructions: u64,
}

fn u16le(b: &[u8], o: usize) -> u64 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as u64
}
fn u32le(b: &[u8], o: usize) -> u64 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as u64
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// Runs a static aarch64 ELF with `args` (argv[0] included) and returns its
/// stdout and exit status. Panics with a diagnostic on CPU exits the mini
/// kernel cannot handle.
pub fn run_elf(elf: &[u8], args: &[&str], max_insns: u64) -> Outcome {
    run_elf_with_aot(elf, args, max_insns, |_| None)
}

pub fn run_elf_with_aot(
    elf: &[u8],
    args: &[&str],
    max_insns: u64,
    lookup: impl Fn(u64) -> Option<indroid_a64::mem::AotUnit>,
) -> Outcome {
    assert_eq!(&elf[..4], b"\x7fELF");
    assert_eq!(u16le(elf, 18), 183, "not aarch64");
    let mem = Pages::new();
    let entry = u64le(elf, 24);
    let phoff = u64le(elf, 32) as usize;
    let phentsize = u16le(elf, 54) as usize;
    let phnum = u16le(elf, 56) as usize;
    let mut brk = 0u64;
    let mut phdr_addr = 0u64;
    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        let ptype = u32le(elf, ph);
        let flags = u32le(elf, ph + 4) as u8;
        let off = u64le(elf, ph + 8) as usize;
        let vaddr = u64le(elf, ph + 16);
        let filesz = u64le(elf, ph + 32) as usize;
        let memsz = u64le(elf, ph + 40);
        if ptype == 6 {
            phdr_addr = vaddr;
        }
        if ptype != 1 {
            continue;
        }
        if off <= phoff && phoff < off + filesz && phdr_addr == 0 {
            phdr_addr = vaddr + (phoff - off) as u64;
        }
        let prot = (if flags & 4 != 0 { R } else { 0 })
            | (if flags & 2 != 0 { W } else { 0 })
            | (if flags & 1 != 0 { X } else { 0 });
        mem.map(vaddr, memsz, prot | W);
        if filesz != 0 {
            mem.write(vaddr, &elf[off..off + filesz]);
        }
        mem.map(vaddr, memsz, prot);
        if prot & X != 0 && (vaddr & (PAGE_SIZE - 1)) == (off as u64 & (PAGE_SIZE - 1)) {
            let mut offset = (off as u64) & !(PAGE_SIZE - 1);
            let mut addr = vaddr & !(PAGE_SIZE - 1);
            while addr < vaddr + memsz {
                if let Some(unit) = lookup(offset) {
                    mem.set_aot(addr >> 12, unit);
                }
                addr += PAGE_SIZE;
                offset += PAGE_SIZE;
            }
        }
        brk = brk.max((vaddr + memsz + PAGE_SIZE - 1) & !(PAGE_SIZE - 1));
    }
    // Stack.
    let stack_top = 0x7fff_0000_0000u64;
    let stack_size = 8 << 20;
    mem.map(stack_top - stack_size, stack_size, R | W);
    let mut sp = stack_top - 4096;
    let mut strings = Vec::new();
    let push_str = |s: &str, sp: &mut u64| {
        *sp -= s.len() as u64 + 1;
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        mem.write(*sp, &b);
        *sp
    };
    for a in args {
        strings.push(push_str(a, &mut sp));
    }
    let platform = push_str("aarch64", &mut sp);
    sp -= 16;
    let random = sp;
    mem.write(random, &[0x5a; 16]);
    sp &= !15;
    // AT_HWCAP: FP | ASIMD | EVTSTRM-less; nothing optional so libc takes
    // baseline code paths.
    let hwcap = 0x3u64;
    let auxv: Vec<(u64, u64)> = vec![
        (3, phdr_addr),
        (4, phentsize as u64),
        (5, phnum as u64),
        (6, 4096),
        (9, entry),
        (11, 0),
        (12, 0),
        (13, 0),
        (14, 0),
        (16, hwcap),
        (26, 0),
        (23, 0),
        (25, random),
        (15, platform),
        (0, 0),
    ];
    let mut words: Vec<u64> = vec![args.len() as u64];
    words.extend(strings.iter());
    words.push(0);
    words.push(0); // envp
    for (k, v) in &auxv {
        words.push(*k);
        words.push(*v);
    }
    sp = (sp - (words.len() * 8) as u64) & !15;
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    mem.write(sp, &bytes);

    let mut cpu = Cpu::new();
    cpu.pc = entry;
    cpu.sp = sp;
    let mut stdout = Vec::new();
    let mut mmap_next = 0x10_0000_0000u64;
    let start_brk = brk;
    let sampling = std::env::var_os("INDROID_A64_SAMPLE").is_some();
    let mut samples = [0u64; 16];
    loop {
        let left = max_insns.saturating_sub(cpu.retired);
        if left == 0 {
            panic!("instruction limit reached at pc={:#x}", cpu.pc);
        }
        if sampling {
            if let Some(bytes) = mem.read(cpu.pc, 4) {
                let insn = u32::from_le_bytes(bytes.try_into().unwrap());
                samples[((insn >> 25) & 15) as usize] += 1;
            }
        }
        match cpu.run(&mem, if sampling { left.min(4093) } else { left }) {
            Exit::Budget => continue,
            Exit::Yield => continue,
            Exit::Svc(_) => {
                let nr = cpu.x[8];
                let a = [cpu.x[0], cpu.x[1], cpu.x[2], cpu.x[3], cpu.x[4], cpu.x[5]];
                let ret: i64 = match nr {
                    // write / writev
                    64 => {
                        let data = mem.read(a[1], a[2] as usize).expect("bad write buffer");
                        if a[0] == 1 || a[0] == 2 {
                            if a[0] == 1 {
                                stdout.extend_from_slice(&data);
                            } else {
                                eprint!("{}", String::from_utf8_lossy(&data));
                            }
                        }
                        a[2] as i64
                    }
                    66 => {
                        let mut total = 0;
                        for i in 0..a[2] {
                            let iov = mem.read(a[1] + i * 16, 16).unwrap();
                            let base = u64le(&iov, 0);
                            let len = u64le(&iov, 8) as usize;
                            let data = mem.read(base, len).unwrap();
                            if a[0] == 1 {
                                stdout.extend_from_slice(&data);
                            } else {
                                eprint!("{}", String::from_utf8_lossy(&data));
                            }
                            total += len as i64;
                        }
                        total
                    }
                    93 | 94 => {
                        if sampling {
                            eprintln!(
                                "opcode top bits 28:25 samples for {}: {:?}",
                                args[0], samples
                            );
                        }
                        return Outcome {
                            stdout,
                            exit_code: a[0] as i32,
                            instructions: cpu.retired,
                        };
                    }
                    214 => {
                        // brk
                        if a[0] == 0 || a[0] < start_brk {
                            brk as i64
                        } else {
                            let new = (a[0] + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                            if new > brk {
                                mem.map(brk, new - brk, R | W);
                            }
                            brk = new;
                            a[0] as i64
                        }
                    }
                    222 => {
                        // mmap (anonymous only)
                        let len = (a[1] + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                        let addr = if a[3] & 0x10 != 0 {
                            a[0]
                        } else {
                            let r = mmap_next;
                            mmap_next += len + PAGE_SIZE;
                            r
                        };
                        let prot = a[2] as u8 & 7;
                        let prot = (if prot & 1 != 0 { R } else { 0 })
                            | (if prot & 2 != 0 { W } else { 0 })
                            | (if prot & 4 != 0 { X } else { 0 });
                        mem.map(addr, len, prot);
                        addr as i64
                    }
                    215 => {
                        mem.unmap(a[0], a[1]);
                        0
                    }
                    226 => {
                        let prot = a[2] as u8 & 7;
                        let prot = (if prot & 1 != 0 { R } else { 0 })
                            | (if prot & 2 != 0 { W } else { 0 })
                            | (if prot & 4 != 0 { X } else { 0 });
                        mem.map(a[0], a[1], prot);
                        0
                    }
                    96 => 1000,                // set_tid_address
                    99 | 134 | 135 | 233 => 0, // set_robust_list, rt_sigaction, rt_sigprocmask, madvise
                    293 => -38,                // rseq
                    261 => -38,                // prlimit64
                    172 | 178 => 1000,         // getpid, gettid
                    174..=177 => 0,            // getuid...
                    278 => {
                        // getrandom
                        mem.write(a[0], &vec![0x42; a[1] as usize]);
                        a[1] as i64
                    }
                    160 => {
                        // uname
                        let mut buf = vec![0u8; 65 * 6];
                        for (i, s) in ["Linux", "indroid", "6.1.0", "#1", "aarch64", ""]
                            .iter()
                            .enumerate()
                        {
                            buf[i * 65..i * 65 + s.len()].copy_from_slice(s.as_bytes());
                        }
                        mem.write(a[0], &buf);
                        0
                    }
                    113 => {
                        // clock_gettime: fixed time keeps output deterministic
                        mem.write(a[1], &[0u8; 16]);
                        0
                    }
                    78 => -2,  // readlinkat
                    29 => -25, // ioctl: ENOTTY
                    80 => {
                        // fstat: pretend char device for stdout
                        let mut st = vec![0u8; 128];
                        st[16..20].copy_from_slice(&0o20620u32.to_le_bytes());
                        mem.write(a[1], &st);
                        0
                    }
                    98 => 0,       // futex
                    56 | 48 => -2, // openat, faccessat
                    57 => 0,
                    _ => panic!("mini kernel: unsupported syscall {nr} at pc={:#x}", cpu.pc),
                };
                cpu.x[0] = ret as u64;
            }
            other => panic!(
                "cpu exit {other:x?} at pc={:#x} insn={:08x?}",
                cpu.pc,
                mem.read(cpu.pc, 4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            ),
        }
    }
}

/// Compiles tests/c/<name>.c with aarch64-linux-gnu-gcc -static, runs it here
/// and under qemu-aarch64-static, and asserts identical stdout + exit code.
/// Skips (returns false) if the cross toolchain or qemu is missing.
pub fn differential(name: &str, cflags: &[&str]) -> bool {
    use std::process::Command;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/c");
    let src = dir.join(format!("{name}.c"));
    let out_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("a64-c");
    std::fs::create_dir_all(&out_dir).unwrap();
    let suffix = cflags.join("").replace(['-', '='], "");
    let bin = out_dir.join(format!("{name}{suffix}"));
    let cc = Command::new("aarch64-linux-gnu-gcc")
        .args(cflags)
        .arg("-static")
        .arg("-o")
        .arg(&bin)
        .arg(&src)
        .arg("-lm")
        .status();
    match cc {
        Ok(s) if s.success() => {}
        Ok(_) => panic!("compile failed: {name}"),
        Err(_) => {
            eprintln!("skipping {name}: aarch64-linux-gnu-gcc not found");
            return false;
        }
    }
    let Ok(q) = Command::new("qemu-aarch64-static").arg(&bin).output() else {
        eprintln!("skipping {name}: qemu-aarch64-static not found");
        return false;
    };
    let elf = std::fs::read(&bin).unwrap();
    let o = run_elf(&elf, &[name], 20_000_000_000);
    let want = String::from_utf8_lossy(&q.stdout);
    let got = String::from_utf8_lossy(&o.stdout);
    if want != got {
        for (i, (w, g)) in want.lines().zip(got.lines()).enumerate() {
            if w != g {
                panic!(
                    "{name}: first difference at line {}:\n qemu:   {w}\n indroid: {g}",
                    i + 1
                );
            }
        }
        panic!("{name}: output length differs\nqemu:\n{want}\nindroid:\n{got}");
    }
    assert_eq!(Some(o.exit_code), q.status.code(), "{name}: exit code");
    eprintln!("{name}: ok, {} instructions", o.instructions);
    true
}
