//! Fetch, decode and execute A64 integer instructions.
use crate::access::*;
use crate::cpu::{Cpu, Exit};
use crate::mem::{Access, AddressSpace};

#[inline]
fn bits(x: u32, lo: u32, n: u32) -> u32 {
    (x >> lo) & ((1u32 << n) - 1)
}
#[inline]
fn sext(x: u64, n: u32) -> u64 {
    ((x << (64 - n)) as i64 >> (64 - n)) as u64
}
#[inline]
fn reg(c: &Cpu, r: u32) -> u64 {
    if r < 31 {
        c.x[r as usize]
    } else {
        0
    }
}
#[inline]
fn base(c: &Cpu, r: u32) -> u64 {
    if r == 31 {
        c.sp
    } else {
        reg(c, r)
    }
}
#[inline]
fn put(c: &mut Cpu, r: u32, v: u64, w: bool) {
    if r < 31 {
        c.x[r as usize] = if w { v as u32 as u64 } else { v }
    }
}
#[inline]
fn putsp(c: &mut Cpu, r: u32, v: u64, w: bool) {
    let v = if w { v as u32 as u64 } else { v };
    if r < 31 {
        c.x[r as usize] = v
    } else {
        c.sp = v
    }
}
#[inline]
fn mask(n: u32) -> u64 {
    if n == 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}
#[inline]
fn ror(v: u64, s: u32, n: u32) -> u64 {
    let s = s % n;
    let v = v & mask(n);
    if s == 0 {
        v
    } else {
        (v >> s) | (v << (n - s) & mask(n))
    }
}
#[inline]
fn shift(v: u64, t: u32, s: u32, n: u32) -> u64 {
    let v = v & mask(n);
    match t {
        0 => v.wrapping_shl(s) & mask(n),
        1 => v >> s,
        2 => ((sext(v, n) as i64) >> s) as u64 & mask(n),
        _ => ror(v, s, n),
    }
}
#[inline]
fn add(c: &mut Cpu, a: u64, b: u64, carry: bool, n: u32, flags: bool) -> u64 {
    let m = mask(n);
    let a = a & m;
    let b = b & m;
    let wide = a as u128 + b as u128 + carry as u128;
    let r = wide as u64 & m;
    if flags {
        let sign = 1u64 << (n - 1);
        c.nzcv = ((r & sign != 0) as u64) << 31
            | ((r == 0) as u64) << 30
            | ((wide >> n != 0) as u64) << 29
            | (((!(a ^ b) & (a ^ r) & sign) != 0) as u64) << 28;
    }
    r
}
#[inline]
fn cond(c: &Cpu, k: u32) -> bool {
    let n = c.nzcv >> 31 & 1 != 0;
    let z = c.nzcv >> 30 & 1 != 0;
    let carry = c.nzcv >> 29 & 1 != 0;
    let v = c.nzcv >> 28 & 1 != 0;
    let x = match k >> 1 {
        0 => z,
        1 => carry,
        2 => n,
        3 => v,
        4 => carry && !z,
        5 => n == v,
        6 => n == v && !z,
        _ => true,
    };
    if k & 1 != 0 && k != 15 {
        !x
    } else {
        x
    }
}
fn bitmasks(n: u32, imms: u32, immr: u32, width: u32, immediate: bool) -> Option<(u64, u64)> {
    let v = (n << 6) | (!imms & 63);
    if v == 0 {
        return None;
    }
    let len = 31 - v.leading_zeros();
    if len < 1 {
        return None;
    }
    let levels = (1 << len) - 1;
    let s = imms & levels;
    let r = immr & levels;
    if immediate && s == levels {
        return None;
    }
    let esize = 1u32 << len;
    if esize > width {
        return None;
    }
    let we = ror(mask(s + 1), r, esize);
    let te = mask(((s.wrapping_sub(r)) & levels) + 1);
    let mut w = 0;
    let mut t = 0;
    let mut i = 0;
    while i < width {
        w |= we << i;
        t |= te << i;
        i += esize;
    }
    Some((w, t))
}
#[inline]
fn load<A: AddressSpace + ?Sized>(c: &mut Cpu, m: &A, a: u64, size: u32) -> Result<u64, Exit> {
    Ok(match size {
        0 => read_u8(c, m, a)? as u64,
        1 => read_u16(c, m, a)? as u64,
        2 => read_u32(c, m, a)? as u64,
        _ => read_u64(c, m, a)?,
    })
}
#[inline]
fn store<A: AddressSpace + ?Sized>(
    c: &mut Cpu,
    m: &A,
    a: u64,
    size: u32,
    v: u64,
) -> Result<(), Exit> {
    match size {
        0 => write_u8(c, m, a, v as u8),
        1 => write_u16(c, m, a, v as u16),
        2 => write_u32(c, m, a, v as u32),
        _ => write_u64(c, m, a, v),
    }
}
// Pair atomics use the host's 64-bit or 128-bit compare-exchange.
fn atomic_pair<A: AddressSpace + ?Sized>(c: &mut Cpu, m: &A, i: u32) -> Result<(), Exit> {
    use core::sync::atomic::{AtomicU64, Ordering};
    let size = bits(i, 30, 2);
    let cas = size < 2;
    let wide = if cas { size == 1 } else { size == 3 };
    let bytes = if wide { 16 } else { 8 };
    let addr = untag(base(c, bits(i, 5, 5)));
    let load_only = !cas && i & (1 << 22) != 0;
    let p = if load_only {
        if addr & (bytes - 1) != 0 {
            return Err(Exit::Alignment { addr });
        }
        c.tlb.lookup(m, addr, Access::Read).ok_or(Exit::Fault {
            addr,
            access: Access::Read,
        })?
    } else {
        atomic_ptr(c, m, addr, bytes)?
    };
    let rt = bits(i, 0, 5);
    let rs = bits(i, 16, 5);
    let rt2 = if cas { rt + 1 } else { bits(i, 10, 5) };
    if cas && (rt & 1 != 0 || rs & 1 != 0 || rt == 31 || rs == 31) {
        return Err(Exit::Undefined(i));
    }
    let join = |a: u64, b: u64| -> u128 {
        if wide {
            (a as u128) | ((b as u128) << 64)
        } else {
            (a as u32 as u128) | ((b as u32 as u128) << 32)
        }
    };
    let expected = if cas {
        join(reg(c, rs), reg(c, rs + 1))
    } else {
        c.exclusive_value
    };
    let replacement = join(reg(c, rt), reg(c, rt2));
    let is_load = !cas && i & (1 << 22) != 0;
    let eligible = cas || is_load || c.exclusive.take() == Some((addr, bytes as u8));
    let old;
    let mut success = false;
    if wide {
        if (p as usize) & 15 != 0 {
            return Err(Exit::Alignment { addr });
        }
        // A compare-exchange of zero with zero also obtains an atomic snapshot.
        let compare = if is_load || !eligible { 0 } else { expected };
        let replace = if is_load || !eligible { 0 } else { replacement };
        old = unsafe { crate::base_atomic::cas128(p, compare, replace) };
        success = !is_load && eligible && old == expected;
    } else {
        let a = unsafe { &*(p as *const AtomicU64) };
        if is_load || !eligible {
            old = a.load(Ordering::SeqCst) as u128;
        } else {
            match a.compare_exchange(
                expected as u64,
                replacement as u64,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(v) => {
                    old = v as u128;
                    success = true
                }
                Err(v) => old = v as u128,
            }
        }
    }
    if is_load {
        c.exclusive = Some((addr, bytes as u8));
        c.exclusive_value = old;
    }
    if is_load || cas {
        let dest = if cas { rs } else { rt };
        let dest2 = if cas { rs + 1 } else { rt2 };
        put(c, dest, old as u64, !wide);
        put(c, dest2, (old >> if wide { 64 } else { 32 }) as u64, !wide);
    } else {
        put(c, rs, (!success) as u64, true);
    }
    Ok(())
}
// Atomic operations use sequentially consistent host atomics, stronger than every
// ordering requested by the guest.  Single-register exclusives use a CAS.
fn atomic<A: AddressSpace + ?Sized>(c: &mut Cpu, m: &A, i: u32) -> Result<(), Exit> {
    use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
    let size = bits(i, 30, 2);
    let bytes = 1u64 << size;
    let addr = untag(base(c, bits(i, 5, 5)));
    let rt = bits(i, 0, 5);
    let rs = bits(i, 16, 5);
    if i & 0x3f00_0000 == 0x0800_0000 && i & (1 << 21) != 0 && i & (1 << 23) == 0 {
        return atomic_pair(c, m, i);
    }
    let pure_load = i & 0x3f00_0000 == 0x0800_0000
        && i & (1 << 22) != 0
        && (i & (1 << 23) == 0 || i & (1 << 21) == 0);
    let pure_store = i & 0x3f00_0000 == 0x0800_0000
        && i & (1 << 23) != 0
        && i & (1 << 22) == 0
        && i & (1 << 21) == 0;
    let p = if pure_load || pure_store {
        if addr & (bytes - 1) != 0 {
            return Err(Exit::Alignment { addr });
        }
        let access = if pure_load {
            Access::Read
        } else {
            Access::Write
        };
        c.tlb
            .lookup(m, addr, access)
            .ok_or(Exit::Fault { addr, access })?
    } else {
        atomic_ptr(c, m, addr, bytes)?
    };
    macro_rules! access {
        ($ty:ty,$at:ty) => {{
            let a = unsafe { &*(p as *const $at) };
            let old: u64;
            if i & 0x3f00_0000 == 0x0800_0000 {
                if i & (1 << 23) != 0 {
                    // CAS or acquire/release load/store
                    if i & (1 << 21) != 0 {
                        let expected = reg(c, rs) as $ty;
                        let new = reg(c, rt) as $ty;
                        old = match a.compare_exchange(
                            expected,
                            new,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(v) | Err(v) => v as u64,
                        };
                        put(c, rs, old, size < 3);
                    } else if i & (1 << 22) != 0 {
                        old = a.load(Ordering::SeqCst) as u64;
                        put(c, rt, old, size < 3)
                    } else {
                        a.store(reg(c, rt) as $ty, Ordering::SeqCst)
                    }
                } else if i & (1 << 22) != 0 {
                    old = a.load(Ordering::SeqCst) as u64;
                    c.exclusive = Some((addr, bytes as u8));
                    c.exclusive_value = old as u128;
                    put(c, rt, old, size < 3);
                } else {
                    let expected = c.exclusive_value as $ty;
                    let good = c.exclusive.take() == Some((addr, bytes as u8))
                        && a.compare_exchange(
                            expected,
                            reg(c, rt) as $ty,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok();
                    put(c, rs, (!good) as u64, true);
                }
            } else {
                let val = reg(c, rs) as $ty;
                let op = bits(i, 12, 4);
                old = a
                    .try_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                        let v = match op {
                            0 => x.wrapping_add(val),
                            1 => x & !val,
                            2 => x ^ val,
                            3 => x | val,
                            4 => {
                                if ((x as i64) << (64 - bytes * 8))
                                    >= ((val as i64) << (64 - bytes * 8))
                                {
                                    x
                                } else {
                                    val
                                }
                            }
                            5 => {
                                if ((x as i64) << (64 - bytes * 8))
                                    <= ((val as i64) << (64 - bytes * 8))
                                {
                                    x
                                } else {
                                    val
                                }
                            }
                            6 => x.max(val),
                            7 => x.min(val),
                            8 => val,
                            _ => return None,
                        };
                        Some(v)
                    })
                    .map_err(|_| Exit::Undefined(i))? as u64;
                put(c, rt, old, size < 3)
            }
        }};
    }
    match size {
        0 => access!(u8, AtomicU8),
        1 => access!(u16, AtomicU16),
        2 => access!(u32, AtomicU32),
        _ => access!(u64, AtomicU64),
    };
    Ok(())
}
fn ldst<A: AddressSpace + ?Sized>(c: &mut Cpu, m: &A, i: u32) -> Result<(), Exit> {
    let rn = bits(i, 5, 5);
    let rt = bits(i, 0, 5);
    let size = bits(i, 30, 2);
    let opc = bits(i, 22, 2);
    let undef = || Exit::Undefined(i);
    // RCpc acquire loads and unscaled acquire/release transfers.
    if i & 0x3fff_fc00 == 0x38bf_c000 {
        let v = load(c, m, base(c, rn), size)?;
        core::sync::atomic::fence(core::sync::atomic::Ordering::Acquire);
        put(c, rt, v, size < 3);
        return Ok(());
    }
    if i & 0x3f20_0c00 == 0x1900_0000 {
        let addr = base(c, rn).wrapping_add(sext(bits(i, 12, 9) as u64, 9));
        if opc == 0 {
            core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
            store(c, m, addr, size, reg(c, rt))?;
        } else {
            let v = load(c, m, addr, size)?;
            core::sync::atomic::fence(core::sync::atomic::Ordering::Acquire);
            let v = if opc >= 2 { sext(v, 8 << size) } else { v };
            put(c, rt, v, if opc >= 2 { opc == 3 } else { size < 3 });
        }
        return Ok(());
    }
    if i & 0x3f00_0000 == 0x0800_0000 || i & 0x3f20_0c00 == 0x3820_0000 {
        return atomic(c, m, i);
    }
    // literal
    if i & 0x3b00_0000 == 0x1800_0000 {
        let a = c.pc.wrapping_add(sext((bits(i, 5, 19) as u64) << 2, 21));
        match bits(i, 30, 2) {
            0 => {
                let v = load(c, m, a, 2)?;
                put(c, rt, v, true)
            }
            1 => {
                let v = load(c, m, a, 3)?;
                put(c, rt, v, false)
            }
            2 => {
                let v = load(c, m, a, 2)?;
                put(c, rt, sext(v, 32), false)
            }
            _ => {}
        }
        return Ok(());
    }
    // pair
    if i & 0x3a00_0000 == 0x2800_0000 {
        let op = bits(i, 30, 2);
        if op == 3 {
            return Err(undef());
        }
        let sz = if op == 2 { 3 } else { 2 };
        let offset = sext((bits(i, 15, 7) as u64) << sz, 7 + sz);
        let mode = bits(i, 23, 2);
        let b = base(c, rn);
        let addr = if mode == 1 { b } else { b.wrapping_add(offset) };
        let rt2 = bits(i, 10, 5);
        if i & (1 << 22) != 0 {
            let x = load(c, m, addr, sz)?;
            let y = load(c, m, addr.wrapping_add(1 << sz), sz)?;
            put(c, rt, if op == 1 { sext(x, 32) } else { x }, op == 0);
            put(c, rt2, if op == 1 { sext(y, 32) } else { y }, op == 0)
        } else {
            store(c, m, addr, sz, reg(c, rt))?;
            store(c, m, addr.wrapping_add(1 << sz), sz, reg(c, rt2))?
        }
        if mode == 1 || mode == 3 {
            putsp(c, rn, b.wrapping_add(offset), false)
        }
        return Ok(());
    }
    if i & 0x3b00_0000 == 0x3900_0000
        || i & 0x3b20_0000 == 0x3800_0000
        || i & 0x3b20_0c00 == 0x3820_0800
    {
        let b = base(c, rn);
        let (addr, writeback) = if i & (1 << 24) != 0 {
            (b.wrapping_add((bits(i, 10, 12) as u64) << size), None)
        } else if i & (1 << 21) != 0 {
            let rm = reg(c, bits(i, 16, 5));
            let opt = bits(i, 13, 3);
            let x = match opt {
                2 => rm as u32 as u64,
                3 => rm,
                6 => sext(rm as u32 as u64, 32),
                7 => rm,
                _ => return Err(undef()),
            };
            (
                b.wrapping_add(x.wrapping_shl(if i & (1 << 12) != 0 { size } else { 0 })),
                None,
            )
        } else {
            let off = sext(bits(i, 12, 9) as u64, 9);
            let mode = bits(i, 10, 2);
            (
                if mode == 1 { b } else { b.wrapping_add(off) },
                if mode == 1 || mode == 3 {
                    Some(b.wrapping_add(off))
                } else {
                    None
                },
            )
        };
        if opc == 0 {
            store(c, m, addr, size, reg(c, rt))?
        } else if opc == 1 {
            let v = load(c, m, addr, size)?;
            put(c, rt, v, size != 3)
        } else if size == 3 { /* PRFM */
        } else {
            let v = sext(load(c, m, addr, size)?, 8 << size);
            put(c, rt, v, opc == 3)
        }
        if let Some(v) = writeback {
            putsp(c, rn, v, false)
        }
        return Ok(());
    }
    Err(undef())
}
fn execute<A: AddressSpace + ?Sized>(c: &mut Cpu, m: &A, i: u32) -> Result<Option<Exit>, Exit> {
    let d = bits(i, 0, 5);
    let rn = bits(i, 5, 5);
    let rm = bits(i, 16, 5);
    let sf = i >> 31 != 0;
    let width = if sf { 64 } else { 32 };
    let undef = || Exit::Undefined(i);
    let op0 = bits(i, 25, 4);
    if op0 & 7 == 7 {
        crate::simd::exec_dp(c, m, i)?;
        return Ok(None);
    }
    if op0 & 5 == 4 {
        if i & (1 << 26) != 0 {
            crate::simd::exec_ldst(c, m, i)?
        } else {
            ldst(c, m, i)?
        }
        return Ok(None);
    }
    // Pointer authentication is disabled: authenticated branches are plain branches.
    if i == 0xd65f0bff || i == 0xd65f0fff {
        c.pc = c.x[30];
        return Ok(Some(Exit::Budget));
    }
    if i & 0xffdf_f800 == 0xd71f_0800 || i & 0xffdf_f81f == 0xd61f_081f {
        let target = reg(c, rn);
        if i & (1 << 21) != 0 {
            c.x[30] = c.pc.wrapping_add(4)
        }
        c.pc = target;
        return Ok(Some(Exit::Budget));
    }
    // PC-relative address
    if i & 0x1f00_0000 == 0x1000_0000 {
        let imm = sext(((bits(i, 5, 19) << 2) | bits(i, 29, 2)) as u64, 21);
        let v = if i >> 31 != 0 {
            (c.pc & !4095).wrapping_add(imm << 12)
        } else {
            c.pc.wrapping_add(imm)
        };
        put(c, d, v, false);
        return Ok(None);
    }
    // add/sub immediate
    if i & 0x1f00_0000 == 0x1100_0000 {
        let sub = i & (1 << 30) != 0;
        let flags = i & (1 << 29) != 0;
        let imm = (bits(i, 10, 12) as u64) << if i & (1 << 22) != 0 { 12 } else { 0 };
        let r = add(
            c,
            base(c, rn),
            if sub { !imm } else { imm },
            sub,
            width,
            flags,
        );
        if flags {
            put(c, d, r, !sf)
        } else {
            putsp(c, d, r, !sf)
        }
        return Ok(None);
    }
    // logical immediate
    if i & 0x1f80_0000 == 0x1200_0000 {
        let (imm, _) = bitmasks(bits(i, 22, 1), bits(i, 10, 6), bits(i, 16, 6), width, true)
            .ok_or_else(undef)?;
        let a = reg(c, rn);
        let opc = bits(i, 29, 2);
        let v = match opc {
            0 | 3 => a & imm,
            1 => a | imm,
            _ => a ^ imm,
        };
        if opc == 3 {
            c.nzcv = ((v >> (width - 1) & 1) << 31) | (((v & mask(width) == 0) as u64) << 30);
            put(c, d, v, !sf)
        } else {
            putsp(c, d, v, !sf)
        }
        return Ok(None);
    }
    // move wide
    if i & 0x1f80_0000 == 0x1280_0000 {
        let sh = bits(i, 21, 2) * 16;
        if sh >= width {
            return Err(undef());
        }
        let imm = (bits(i, 5, 16) as u64) << sh;
        let v = match bits(i, 29, 2) {
            0 => !imm,
            2 => imm,
            3 => (reg(c, d) & !(0xffffu64 << sh)) | imm,
            _ => return Err(undef()),
        };
        put(c, d, v, !sf);
        return Ok(None);
    }
    // bitfield
    if i & 0x1f80_0000 == 0x1300_0000 {
        let immr = bits(i, 16, 6);
        let imms = bits(i, 10, 6);
        if immr >= width || imms >= width || bits(i, 22, 1) != sf as u32 {
            return Err(undef());
        }
        let (wm, tm) = bitmasks(bits(i, 22, 1), imms, immr, width, false).ok_or_else(undef)?;
        let src = reg(c, rn) & mask(width);
        let bot = ror(src, immr, width) & wm;
        let v = match bits(i, 29, 2) {
            0 => {
                let top = if src >> imms & 1 != 0 { mask(width) } else { 0 };
                (top & !tm) | (bot & tm)
            }
            1 => {
                let old = reg(c, d);
                let b = (old & !wm) | bot;
                (old & !tm) | (b & tm)
            }
            2 => bot & tm,
            _ => return Err(undef()),
        };
        put(c, d, v, !sf);
        return Ok(None);
    }
    if i & 0x1f80_0000 == 0x1380_0000 {
        let s = bits(i, 10, 6);
        if s >= width {
            return Err(undef());
        }
        let a = reg(c, rn) & mask(width);
        let b = reg(c, rm) & mask(width);
        put(
            c,
            d,
            if s == 0 {
                b
            } else {
                (b >> s) | (a << (width - s))
            },
            !sf,
        );
        return Ok(None);
    }
    // branches
    if i & 0x7c00_0000 == 0x1400_0000 {
        if i >> 31 != 0 {
            c.x[30] = c.pc.wrapping_add(4)
        }
        c.pc = c.pc.wrapping_add(sext((bits(i, 0, 26) as u64) << 2, 28));
        return Ok(Some(Exit::Budget));
    }
    if i & 0xff00_0010 == 0x5400_0000 {
        if cond(c, bits(i, 0, 4)) {
            c.pc = c.pc.wrapping_add(sext((bits(i, 5, 19) as u64) << 2, 21));
            return Ok(Some(Exit::Budget));
        }
        return Ok(None);
    }
    if i & 0x7e00_0000 == 0x3400_0000 {
        let v = reg(c, d) & mask(width);
        if (v != 0) == (i & (1 << 24) != 0) {
            c.pc = c.pc.wrapping_add(sext((bits(i, 5, 19) as u64) << 2, 21));
            return Ok(Some(Exit::Budget));
        }
        return Ok(None);
    }
    if i & 0x7e00_0000 == 0x3600_0000 {
        let b = bits(i, 19, 5) | (bits(i, 31, 1) << 5);
        if ((reg(c, d) >> b) & 1 != 0) == (i & (1 << 24) != 0) {
            c.pc = c.pc.wrapping_add(sext((bits(i, 5, 14) as u64) << 2, 16));
            return Ok(Some(Exit::Budget));
        }
        return Ok(None);
    }
    if i & 0xffff_fc1f == 0xd61f_0000
        || i & 0xffff_fc1f == 0xd63f_0000
        || i & 0xffff_fc1f == 0xd65f_0000
    {
        let a = reg(c, rn);
        if i & 0xffff_fc1f == 0xd63f_0000 {
            c.x[30] = c.pc.wrapping_add(4)
        }
        c.pc = a;
        return Ok(Some(Exit::Budget));
    }
    if i & 0xffe0_001f == 0xd400_0001 {
        c.pc = c.pc.wrapping_add(4);
        return Ok(Some(Exit::Svc(bits(i, 5, 16) as u16)));
    }
    if i & 0xffe0_001f == 0xd420_0000 {
        return Err(Exit::Brk(bits(i, 5, 16) as u16));
    }
    // Flag manipulation extensions (FEAT_FlagM / FlagM2).
    match i {
        0xd500401f => {
            c.nzcv ^= 1 << 29;
            return Ok(None);
        }
        0xd500405f => {
            let z = (c.nzcv >> 30) & 1;
            let carry = (c.nzcv >> 29) & 1;
            let v = (c.nzcv >> 28) & 1;
            c.nzcv = ((z | v) << 30) | ((carry & !v) << 29);
            return Ok(None);
        }
        0xd500403f => {
            let z = (c.nzcv >> 30) & 1;
            let carry = (c.nzcv >> 29) & 1;
            c.nzcv = (((carry | z) ^ 1) << 31)
                | ((z & carry) << 30)
                | ((carry | z) << 29)
                | ((z & (carry ^ 1)) << 28);
            return Ok(None);
        }
        _ => {}
    }
    if i & 0xffe0fc10 == 0xba008400 {
        let mask = (bits(i, 0, 4) as u64) << 28;
        let v = reg(c, rn).rotate_right(bits(i, 15, 6)) << 28;
        c.nzcv = (c.nzcv & !mask) | (v & mask);
        return Ok(None);
    }
    if i & 0xffffbc1f == 0x3a00080d {
        let n = if i & (1 << 14) != 0 { 16 } else { 8 };
        let v = reg(c, rn);
        let sign = (v >> (n - 1)) & 1;
        let overflow = ((v >> n) & 1) ^ sign;
        c.nzcv = (c.nzcv & (1 << 29))
            | (sign << 31)
            | (((v & mask(n) == 0) as u64) << 30)
            | (overflow << 28);
        return Ok(None);
    }
    // Common EL0 system registers.
    if i & 0xffc0_0000 == 0xd500_0000 {
        let key = i & 0xffff_ffe0;
        let read = i & (1 << 21) != 0;
        if read {
            let value = match key {
                0xd53b_d040 => c.tpidr_el0,
                0xd53b_d060 => c.tpidrro_el0,
                0xd53b_4200 => c.nzcv,
                0xd53b_4400 => c.fpcr,
                0xd53b_4420 => c.fpsr,
                0xd53b_0020 => 0x8444c004,
                0xd53b_00e0 => 4,
                0xd538_0000 => 0x410fd083,
                0xd53b_e000 => 1_000_000_000,
                0xd53b_e040 => {
                    static START: std::sync::OnceLock<std::time::Instant> =
                        std::sync::OnceLock::new();
                    START
                        .get_or_init(std::time::Instant::now)
                        .elapsed()
                        .as_nanos() as u64
                }
                _ => return Err(undef()),
            };
            put(c, d, value, false);
            return Ok(None);
        }
        match key {
            0xd51b_d040 => c.tpidr_el0 = reg(c, d),
            0xd51b_4200 => c.nzcv = reg(c, d) & 0xf000_0000,
            0xd51b_4400 => c.fpcr = reg(c, d),
            0xd51b_4420 => c.fpsr = reg(c, d),
            0xd50b_7420 => {
                let a = reg(c, d) & !63;
                for off in (0..64).step_by(8) {
                    write_u64(c, m, a + off, 0)?
                }
            }
            0xd50b_7520 => {
                // IC IVAU: instruction cache invalidate (new code written).
                crate::base_cache::CODE_EPOCH.fetch_add(1, core::sync::atomic::Ordering::Release);
            }
            0xd50b_7b20 | 0xd50b_7a20 | 0xd50b_7e20 => {}
            _ => {}
        }
        if matches!(
            key,
            0xd51b_d040
                | 0xd51b_4200
                | 0xd51b_4400
                | 0xd51b_4420
                | 0xd50b_7420
                | 0xd50b_7b20
                | 0xd50b_7a20
                | 0xd50b_7e20
                | 0xd50b_7520
        ) {
            return Ok(None);
        }
    }
    if i & 0xffff_f0ff == 0xd503_305f {
        c.exclusive = None;
        return Ok(None);
    }
    // hints and barriers
    if i & 0xffff_f01f == 0xd503_201f {
        let h = bits(i, 5, 7);
        if (1..=3).contains(&h) {
            c.pc = c.pc.wrapping_add(4);
            return Ok(Some(Exit::Yield));
        }
        return Ok(None);
    }
    if i & 0xffff_f09f == 0xd503_309f {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        return Ok(None);
    }
    // register operations
    if i & 0x1f00_0000 == 0x0a00_0000 {
        let sh = bits(i, 10, 6);
        if sh >= width {
            return Err(undef());
        }
        let a = reg(c, rn);
        let mut b = shift(reg(c, rm), bits(i, 22, 2), sh, width);
        if i & (1 << 21) != 0 {
            b = !b
        }
        let op = bits(i, 29, 2);
        let v = match op {
            0 | 3 => a & b,
            1 => a | b,
            _ => a ^ b,
        } & mask(width);
        if op == 3 {
            c.nzcv = ((v >> (width - 1)) << 31) | (((v == 0) as u64) << 30)
        }
        put(c, d, v, !sf);
        return Ok(None);
    }
    if i & 0x1f20_0000 == 0x0b00_0000 {
        let s = bits(i, 10, 6);
        if s >= width || bits(i, 22, 2) == 3 {
            return Err(undef());
        }
        let sub = i & (1 << 30) != 0;
        let b = shift(reg(c, rm), bits(i, 22, 2), s, width);
        let r = add(
            c,
            reg(c, rn),
            if sub { !b } else { b },
            sub,
            width,
            i & (1 << 29) != 0,
        );
        put(c, d, r, !sf);
        return Ok(None);
    }
    if i & 0x1f20_0000 == 0x0b20_0000 {
        let option = bits(i, 13, 3);
        let s = bits(i, 10, 3);
        if s > 4 {
            return Err(undef());
        }
        let n = 8u32 << (option & 3);
        let mut b = reg(c, rm) & mask(n);
        if option & 4 != 0 {
            b = sext(b, n)
        }
        b = b.wrapping_shl(s);
        let sub = i & (1 << 30) != 0;
        let flags = i & (1 << 29) != 0;
        let v = add(c, base(c, rn), if sub { !b } else { b }, sub, width, flags);
        if flags {
            put(c, d, v, !sf)
        } else {
            putsp(c, d, v, !sf)
        }
        return Ok(None);
    }
    if i & 0x1fe0_fc00 == 0x1a00_0000 {
        let sub = i & (1 << 30) != 0;
        let b = reg(c, rm);
        let r = add(
            c,
            reg(c, rn),
            if sub { !b } else { b },
            c.nzcv & (1 << 29) != 0,
            width,
            i & (1 << 29) != 0,
        );
        put(c, d, r, !sf);
        return Ok(None);
    }
    if i & 0x1fe0_0000 == 0x1a80_0000 {
        let a = if cond(c, bits(i, 12, 4)) {
            reg(c, rn)
        } else {
            let b = reg(c, rm);
            match (bits(i, 30, 1), bits(i, 10, 1)) {
                (0, 0) => b,
                (0, 1) => b.wrapping_add(1),
                (1, 0) => !b,
                _ => b.wrapping_neg(),
            }
        };
        put(c, d, a, !sf);
        return Ok(None);
    }
    if i & 0x1fe0_0000 == 0x1a40_0000 {
        if cond(c, bits(i, 12, 4)) {
            let b = if i & (1 << 11) != 0 {
                rm as u64
            } else {
                reg(c, rm)
            };
            let sub = i & (1 << 30) != 0;
            add(c, reg(c, rn), if sub { !b } else { b }, sub, width, true);
        } else {
            c.nzcv = (bits(i, 0, 4) as u64) << 28
        }
        return Ok(None);
    }
    if i & 0x1f00_0000 == 0x1b00_0000 {
        let op = bits(i, 21, 3);
        let a = reg(c, rn);
        let b = reg(c, rm);
        let acc = reg(c, bits(i, 10, 5));
        let v = match op {
            0 => {
                let p = a.wrapping_mul(b);
                if i & (1 << 15) != 0 {
                    acc.wrapping_sub(p)
                } else {
                    acc.wrapping_add(p)
                }
            }
            1 | 5 => {
                let p = if op == 1 {
                    (a as i32 as i64).wrapping_mul(b as i32 as i64) as u64
                } else {
                    (a as u32 as u64).wrapping_mul(b as u32 as u64)
                };
                if i & (1 << 15) != 0 {
                    acc.wrapping_sub(p)
                } else {
                    acc.wrapping_add(p)
                }
            }
            2 => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
            6 => (((a as u128) * (b as u128)) >> 64) as u64,
            _ => return Err(undef()),
        };
        put(c, d, v, !sf);
        return Ok(None);
    }
    if i & 0x5fe0_0000 == 0x5ac0_0000 {
        let a = reg(c, rn) & mask(width);
        let op = bits(i, 10, 6);
        let v = match op {
            0 => {
                if sf {
                    a.reverse_bits()
                } else {
                    (a as u32).reverse_bits() as u64
                }
            }
            1 => {
                let mut x = 0;
                for j in 0..width / 16 {
                    x |= (((a >> (j * 16)) as u16).swap_bytes() as u64) << (j * 16)
                }
                x
            }
            2 => {
                let mut x = 0;
                for j in 0..width / 32 {
                    x |= (((a >> (j * 32)) as u32).swap_bytes() as u64) << (j * 32)
                }
                x
            }
            3 if sf => a.swap_bytes(),
            4 => a.leading_zeros() as u64 - (64 - width) as u64,
            5 => {
                let v = if a >> (width - 1) != 0 {
                    !a & mask(width)
                } else {
                    a
                };
                v.leading_zeros() as u64 - (64 - width) as u64 - 1
            }
            _ => return Err(undef()),
        };
        put(c, d, v, !sf);
        return Ok(None);
    }
    if i & 0x1fe0_0000 == 0x1ac0_0000 {
        let a = reg(c, rn) & mask(width);
        let b = reg(c, rm) & mask(width);
        let v = match bits(i, 10, 6) {
            16..=23 => {
                let op = bits(i, 10, 6);
                let count = 1u32 << (op & 3);
                let mut crc = a as u32;
                let poly = if op & 4 != 0 { 0x82f63b78 } else { 0xedb88320 };
                for j in 0..count {
                    crc ^= ((b >> (8 * j)) & 255) as u32;
                    for _ in 0..8 {
                        crc = (crc >> 1) ^ (poly & (0u32.wrapping_sub(crc & 1)));
                    }
                }
                crc as u64
            }
            2 => a.checked_div(b).unwrap_or(0),
            3 => {
                let x = sext(a, width) as i64;
                let y = sext(b, width) as i64;
                if y == 0 {
                    0
                } else {
                    x.wrapping_div(y) as u64
                }
            }
            8..=11 => shift(a, bits(i, 10, 6) - 8, (b as u32) & (width - 1), width),
            _ => return Err(undef()),
        };
        put(c, d, v, !sf);
        return Ok(None);
    }

    Err(undef())
}
#[inline(always)]
fn execute_cached<A: AddressSpace + ?Sized>(
    c: &mut Cpu,
    m: &A,
    op: crate::base_cache::Op,
) -> Result<Option<Exit>, Exit> {
    let i = op.raw;
    let d = op.rd as u32;
    let rn = op.rn as u32;
    let rm = op.rm as u32;
    let sf = i >> 31 != 0;
    let width = if sf { 64 } else { 32 };
    match op.kind {
        1 => {
            let sub = i & (1 << 30) != 0;
            let flags = i & (1 << 29) != 0;
            let v = add(
                c,
                base(c, rn),
                if sub { !op.imm } else { op.imm },
                sub,
                width,
                flags,
            );
            if flags {
                put(c, d, v, !sf)
            } else {
                putsp(c, d, v, !sf)
            };
            Ok(None)
        }
        2 => {
            let v = match bits(i, 29, 2) {
                0 => !op.imm,
                2 => op.imm,
                3 => {
                    let sh = bits(i, 21, 2) * 16;
                    (reg(c, d) & !(0xffffu64 << sh)) | op.imm
                }
                _ => unreachable!(),
            };
            put(c, d, v, !sf);
            Ok(None)
        }
        3 => {
            if i >> 31 != 0 {
                c.x[30] = c.pc.wrapping_add(4)
            }
            c.pc = c.pc.wrapping_add(op.imm);
            Ok(Some(Exit::Budget))
        }
        4 => {
            let sub = i & (1 << 30) != 0;
            let b = shift(reg(c, rm), bits(i, 22, 2), op.imm as u32, width);
            let v = add(
                c,
                reg(c, rn),
                if sub { !b } else { b },
                sub,
                width,
                i & (1 << 29) != 0,
            );
            put(c, d, v, !sf);
            Ok(None)
        }
        5 => {
            let a = reg(c, rn);
            let mut b = shift(reg(c, rm), bits(i, 22, 2), op.imm as u32, width);
            if i & (1 << 21) != 0 {
                b = !b
            }
            let opc = bits(i, 29, 2);
            let v = match opc {
                0 | 3 => a & b,
                1 => a | b,
                _ => a ^ b,
            } & mask(width);
            if opc == 3 {
                c.nzcv = ((v >> (width - 1)) << 31) | (((v == 0) as u64) << 30)
            }
            put(c, d, v, !sf);
            Ok(None)
        }
        6 => {
            let size = i >> 30;
            let opc = bits(i, 22, 2);
            let addr = base(c, rn).wrapping_add(op.imm);
            if opc == 0 {
                store(c, m, addr, size, reg(c, d))?;
            } else if opc == 1 {
                let v = load(c, m, addr, size)?;
                put(c, d, v, size != 3);
            } else if size != 3 {
                let v = sext(load(c, m, addr, size)?, 8 << size);
                put(c, d, v, opc == 3);
            }
            Ok(None)
        }
        7 => {
            if cond(c, i & 15) {
                c.pc = c.pc.wrapping_add(op.imm);
                Ok(Some(Exit::Budget))
            } else {
                Ok(None)
            }
        }
        8 => {
            if ((reg(c, d) & mask(width)) != 0) == (i & (1 << 24) != 0) {
                c.pc = c.pc.wrapping_add(op.imm);
                Ok(Some(Exit::Budget))
            } else {
                Ok(None)
            }
        }
        9 => {
            let v = if sf {
                (c.pc & !4095).wrapping_add(op.imm << 12)
            } else {
                c.pc.wrapping_add(op.imm)
            };
            put(c, d, v, false);
            Ok(None)
        }
        10 => {
            let v = if cond(c, bits(i, 12, 4)) {
                reg(c, rn)
            } else {
                let b = reg(c, rm);
                match (bits(i, 30, 1), bits(i, 10, 1)) {
                    (0, 0) => b,
                    (0, 1) => b.wrapping_add(1),
                    (1, 0) => !b,
                    _ => b.wrapping_neg(),
                }
            };
            put(c, d, v, !sf);
            Ok(None)
        }
        11 => {
            let option = bits(i, 13, 3);
            let n = 8u32 << (option & 3);
            let mut b = reg(c, rm) & mask(n);
            if option & 4 != 0 {
                b = sext(b, n)
            }
            b = b.wrapping_shl(bits(i, 10, 3));
            let sub = i & (1 << 30) != 0;
            let flags = i & (1 << 29) != 0;
            let v = add(c, base(c, rn), if sub { !b } else { b }, sub, width, flags);
            if flags {
                put(c, d, v, !sf)
            } else {
                putsp(c, d, v, !sf)
            }
            Ok(None)
        }
        12 => {
            ldst(c, m, i)?;
            Ok(None)
        }
        13 => {
            let sub = i & (1 << 30) != 0;
            let b = reg(c, rm);
            let v = add(
                c,
                reg(c, rn),
                if sub { !b } else { b },
                c.nzcv & (1 << 29) != 0,
                width,
                i & (1 << 29) != 0,
            );
            put(c, d, v, !sf);
            Ok(None)
        }
        14 => {
            if cond(c, bits(i, 12, 4)) {
                let b = if i & (1 << 11) != 0 {
                    rm as u64
                } else {
                    reg(c, rm)
                };
                let sub = i & (1 << 30) != 0;
                add(c, reg(c, rn), if sub { !b } else { b }, sub, width, true);
            } else {
                c.nzcv = (bits(i, 0, 4) as u64) << 28;
            }
            Ok(None)
        }
        15 => {
            let bit = bits(i, 19, 5) | (bits(i, 31, 1) << 5);
            if ((reg(c, d) >> bit) & 1 != 0) == (i & (1 << 24) != 0) {
                c.pc = c.pc.wrapping_add(op.imm);
                Ok(Some(Exit::Budget))
            } else {
                Ok(None)
            }
        }
        16 => {
            let pair_op = i >> 30;
            let sz = if pair_op == 2 { 3 } else { 2 };
            let mode = bits(i, 23, 2);
            let b = base(c, rn);
            let addr = if mode == 1 { b } else { b.wrapping_add(op.imm) };
            let rt2 = bits(i, 10, 5);
            if i & (1 << 22) != 0 {
                let x = load(c, m, addr, sz)?;
                let y = load(c, m, addr.wrapping_add(1 << sz), sz)?;
                put(
                    c,
                    d,
                    if pair_op == 1 { sext(x, 32) } else { x },
                    pair_op == 0,
                );
                put(
                    c,
                    rt2,
                    if pair_op == 1 { sext(y, 32) } else { y },
                    pair_op == 0,
                );
            } else {
                store(c, m, addr, sz, reg(c, d))?;
                store(c, m, addr.wrapping_add(1 << sz), sz, reg(c, rt2))?;
            }
            if mode == 1 || mode == 3 {
                putsp(c, rn, b.wrapping_add(op.imm), false);
            }
            Ok(None)
        }
        _ => execute(c, m, i),
    }
}
/// Execute a recognized nonfaulting SIMD operation without changing PC or retirement.
#[inline(always)]
pub fn aot_simd_unretired(cpu: &mut Cpu, insn: u32) {
    let d = (insn & 31) as usize;
    let n = ((insn >> 5) & 31) as usize;
    let m = ((insn >> 16) & 31) as usize;
    let key = insn & 0xffff_fc00;
    if matches!(key, 0x4e28_4800 | 0x4e28_5800 | 0x4e28_6800 | 0x4e28_7800) {
        if let Some(value) = crate::base_crypto::aes(key, cpu.v[d], cpu.v[n]) {
            cpu.v[d] = value;
        } else {
            assert!(crate::simd_crypto::exec(cpu, insn));
        }
    } else {
        let value = if insn & 0x2000_0000 != 0 {
            cpu.v[n] ^ cpu.v[m]
        } else {
            cpu.v[n] | cpu.v[m]
        };
        cpu.v[d] = if insn & (1 << 30) != 0 {
            value
        } else {
            value as u64 as u128
        };
    }
}

/// Execute one recognized SIMD operation with architectural bookkeeping.
#[inline(always)]
pub fn aot_simd_simple(cpu: &mut Cpu, insn: u32) {
    aot_simd_unretired(cpu, insn);
    cpu.pc = cpu.pc.wrapping_add(4);
    cpu.retired += 1;
}

/// Execute a compile-time A64 word from an AOT unit. The caller must have
/// validated the executable page and must enforce the instruction budget.
#[inline(always)]
pub fn aot_step(cpu: &mut Cpu, mem: &dyn AddressSpace, insn: u32) -> Option<Exit> {
    let pc = cpu.pc;
    let op = crate::base_cache::Op::decode(insn);
    match execute_cached(cpu, mem, op) {
        Ok(exit) => {
            cpu.retired += 1;
            match exit {
                None => {
                    cpu.pc = pc.wrapping_add(4);
                    None
                }
                Some(Exit::Budget) => None,
                Some(e) => Some(e),
            }
        }
        Err(e) => Some(e),
    }
}
pub(crate) fn run<A: AddressSpace + ?Sized>(cpu: &mut Cpu, mem: &A, budget: u64) -> Exit {
    // Retain the original executor for comparative profiling and debugging.
    static DISABLE_CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *DISABLE_CACHE.get_or_init(|| std::env::var_os("INDROID_A64_DISABLE_CACHE").is_some()) {
        return run_uncached(cpu, mem, budget);
    }
    let mut cache = core::mem::take(&mut cpu.tlb.block_cache);
    let result = run_cached(cpu, mem, budget, &mut cache);
    cpu.tlb.block_cache = cache;
    result
}
fn run_uncached<A: AddressSpace + ?Sized>(cpu: &mut Cpu, mem: &A, budget: u64) -> Exit {
    let end = cpu.retired.saturating_add(budget);
    while cpu.retired < end {
        let pc = cpu.pc;
        let Some(p) = cpu.tlb.lookup(mem, pc, Access::Exec) else {
            return Exit::Fault {
                addr: pc,
                access: Access::Exec,
            };
        };
        let insn = unsafe { (p as *const u32).read_unaligned() };
        match execute(cpu, mem, insn) {
            Ok(exit) => {
                cpu.retired += 1;
                match exit {
                    None => cpu.pc = pc.wrapping_add(4),
                    Some(Exit::Budget) => {}
                    Some(e) => return e,
                }
            }
            Err(e) => return e,
        }
    }
    Exit::Budget
}
// Erases a possibly-unsized AddressSpace without changing Cpu::run's API.
struct AotMemory<'a, A: AddressSpace + ?Sized>(&'a A);
impl<A: AddressSpace + ?Sized> AddressSpace for AotMemory<'_, A> {
    fn translate(&self, page: u64, access: Access) -> Option<*mut u8> {
        self.0.translate(page, access)
    }
    fn generation(&self) -> u64 {
        self.0.generation()
    }
}
fn run_cached<A: AddressSpace + ?Sized>(
    cpu: &mut Cpu,
    mem: &A,
    budget: u64,
    cache: &mut crate::base_cache::BlockCache,
) -> Exit {
    let end = cpu.retired.saturating_add(budget);
    let mut retired = cpu.retired;
    let result = (|| {
        let mut successor = None;
        while retired < end {
            cache.sync(mem, cpu);
            let pc = cpu.pc;
            if pc & 3 != 0 {
                let Some(p) = cpu.tlb.lookup(mem, pc, Access::Exec) else {
                    return Exit::Fault {
                        addr: pc,
                        access: Access::Exec,
                    };
                };
                let insn = unsafe { (p as *const u32).read_unaligned() };
                match execute(cpu, mem, insn) {
                    Ok(exit) => {
                        retired += 1;
                        match exit {
                            None => cpu.pc = pc.wrapping_add(4),
                            Some(Exit::Budget) => {}
                            Some(e) => return e,
                        }
                    }
                    Err(e) => return e,
                };
                continue;
            }
            let block = match cache.get(cpu, mem, pc, successor.take()) {
                Ok(b) => b,
                Err(e) => return e,
            };
            if let Some(unit) = block.aot {
                static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
                static REPORTED: std::sync::Once = std::sync::Once::new();
                if *TRACE.get_or_init(|| std::env::var_os("INDROID_AOT_TRACE").is_some()) {
                    REPORTED.call_once(|| eprintln!("AOT unit entered at {:#x}", cpu.pc));
                }
                cpu.retired = retired;
                let exit = (unit.execute)(cpu, &AotMemory(mem), end - retired);
                retired = cpu.retired;
                if let Some(exit) = exit {
                    return exit;
                }
                continue;
            }
            for op in &block.ops {
                if retired >= end {
                    return Exit::Budget;
                }
                let pc = cpu.pc;
                match execute_cached(cpu, mem, *op) {
                    Ok(exit) => {
                        retired += 1;
                        match exit {
                            None => cpu.pc = pc.wrapping_add(4),
                            Some(Exit::Budget) => {
                                if op.kind == 3 {
                                    successor = block.successor;
                                }
                                break;
                            }
                            Some(e) => return e,
                        }
                    }
                    Err(e) => return e,
                }
            }
        }
        Exit::Budget
    })();
    cpu.retired = retired;
    result
}
