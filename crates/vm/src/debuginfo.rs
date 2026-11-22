//! Line numbers from DEX debug_info (only decoded when a stack trace is built).

use indroid_dex::{read_sleb128, read_uleb128, Dex};

use crate::{MKind, MethodIdx, Vm};

/// Source line for `pc` in method `m`, or -1.
pub fn line_for(vm: &Vm, m: MethodIdx, pc: u32) -> i32 {
    let meth = &vm.methods[m as usize];
    let MKind::Code(code) = &meth.kind else {
        return -1;
    };
    if code.debug_info_off == 0 {
        return -1;
    }
    let bytes = &vm.dexes[code.dex as usize].bytes;
    let Ok(_dex) = Dex::parse(bytes) else {
        return -1;
    };
    decode(bytes, code.debug_info_off as usize, pc).unwrap_or(-1)
}

fn decode(d: &[u8], off: usize, target: u32) -> Option<i32> {
    let mut p = off;
    let mut line = read_uleb128(d, &mut p).ok()? as i32;
    let params = read_uleb128(d, &mut p).ok()?;
    for _ in 0..params {
        read_uleb128(d, &mut p).ok()?;
    }
    let mut addr: u32 = 0;
    let mut best = -1;
    loop {
        let op = *d.get(p)?;
        p += 1;
        match op {
            0x00 => break,
            0x01 => addr += read_uleb128(d, &mut p).ok()?,
            0x02 => line += read_sleb128(d, &mut p).ok()?,
            0x03 => {
                read_uleb128(d, &mut p).ok()?;
                read_uleb128(d, &mut p).ok()?;
                read_uleb128(d, &mut p).ok()?;
            }
            0x04 => {
                for _ in 0..4 {
                    read_uleb128(d, &mut p).ok()?;
                }
            }
            0x05 | 0x06 => {
                read_uleb128(d, &mut p).ok()?;
            }
            0x07 | 0x08 => {}
            0x09 => {
                read_uleb128(d, &mut p).ok()?;
            }
            _ => {
                let adj = op as i32 - 0x0a;
                addr += (adj / 15) as u32;
                line += -4 + adj % 15;
                if addr > target {
                    break;
                }
                best = line;
            }
        }
    }
    Some(best)
}
