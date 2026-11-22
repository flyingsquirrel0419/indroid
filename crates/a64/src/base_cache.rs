//! Per-vCPU decoded basic blocks. Entries are indexed by guest virtual PC;
//! the AddressSpace interface exposes translations, not physical page IDs.
use crate::cpu::Cpu;
use crate::mem::{Access, AddressSpace, PAGE_SIZE};

#[derive(Clone, Copy, Default)]
pub(crate) struct Op {
    pub raw: u32,
    pub kind: u8,
    pub rd: u8,
    pub rn: u8,
    pub rm: u8,
    pub imm: u64,
}
impl Op {
    #[inline(always)]
    pub(crate) fn decode(i: u32) -> Self {
        let mut o = Self {
            raw: i,
            rd: (i & 31) as u8,
            rn: ((i >> 5) & 31) as u8,
            rm: ((i >> 16) & 31) as u8,
            ..Self::default()
        };
        if i & 0x1f00_0000 == 0x1100_0000 {
            o.kind = 1;
            o.imm = (((i >> 10) & 4095) as u64) << if i & (1 << 22) != 0 { 12 } else { 0 };
        } else if i & 0x1f80_0000 == 0x1280_0000
            && ((i >> 21) & 3) * 16 < if i >> 31 != 0 { 64 } else { 32 }
            && (i >> 29) & 3 != 1
        {
            o.kind = 2;
            o.imm = (((i >> 5) & 65535) as u64) << (((i >> 21) & 3) * 16);
        } else if i & 0x7c00_0000 == 0x1400_0000 {
            o.kind = 3;
            o.imm = (((i as u64 & 0x3ff_ffff) << 38) as i64 >> 36) as u64;
        } else if i & 0x3f00_0000 == 0x3900_0000 {
            // Integer unsigned-offset load/store; SIMD/FP transfers keep the slow path.
            o.kind = 6;
            o.imm = (((i >> 10) & 4095) as u64) << (i >> 30);
        } else if i & 0xff00_0010 == 0x5400_0000 {
            o.kind = 7;
            o.imm = ((((i >> 5) & 0x7ffff) as u64) << 45) as i64 as u64;
            o.imm = ((o.imm as i64) >> 43) as u64;
        } else if i & 0x7e00_0000 == 0x3400_0000 {
            o.kind = 8;
            o.imm = (((((i >> 5) & 0x7ffff) as u64) << 45) as i64 >> 43) as u64;
        } else if i & 0x1f00_0000 == 0x1000_0000 {
            o.kind = 9;
            let imm = (((i >> 5) & 0x7ffff) << 2) | ((i >> 29) & 3);
            o.imm = ((imm as i64) << 43 >> 43) as u64;
        } else if i & 0x1fe0_0000 == 0x1a80_0000 {
            o.kind = 10;
        } else if i & 0x1f20_0000 == 0x0b20_0000 && ((i >> 10) & 7) <= 4 {
            o.kind = 11;
        } else if i & 0x3e00_0000 == 0x2800_0000 && i >> 30 != 3 {
            o.kind = 16;
            let sz = if i >> 30 == 2 { 3 } else { 2 };
            let off = (((i >> 15) & 0x7f) as u64) << sz;
            o.imm = ((off << (57 - sz)) as i64 >> (57 - sz)) as u64;
        } else if ((i >> 25) & 5) == 4 && i & (1 << 26) == 0 {
            o.kind = 12;
        } else if i & 0x1fe0_fc00 == 0x1a00_0000 {
            o.kind = 13;
        } else if i & 0x1fe0_0000 == 0x1a40_0000 {
            o.kind = 14;
        } else if i & 0x7e00_0000 == 0x3600_0000 {
            o.kind = 15;
            o.imm = (((((i >> 5) & 0x3fff) as u64) << 50) as i64 >> 48) as u64;
        } else if i & 0x1f20_0000 == 0x0b00_0000
            && ((i >> 10) & 63) < if i >> 31 != 0 { 64 } else { 32 }
            && (i >> 22) & 3 != 3
        {
            o.kind = 4;
            o.imm = ((i >> 10) & 63) as u64;
        } else if i & 0x1f00_0000 == 0x0a00_0000
            && ((i >> 10) & 63) < if i >> 31 != 0 { 64 } else { 32 }
        {
            o.kind = 5;
            o.imm = ((i >> 10) & 63) as u64;
        }
        o
    }
}
pub(crate) struct Block {
    pub pc: u64,
    pub ops: Vec<Op>,
    pub successor: Option<(u64, usize)>,
    pub aot: Option<crate::mem::AotUnit>,
}
#[derive(Default)]
pub(crate) struct BlockCache {
    generation: u64,
    space: usize,
    slots: Vec<Option<Block>>,
}
impl Clone for BlockCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}
/// Bumped by IC IVAU / IC IALLU on any guest thread.
pub static CODE_EPOCH: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

impl BlockCache {
    pub fn clear(&mut self) {
        self.slots.clear();
    }
    pub fn sync<A: AddressSpace + ?Sized>(&mut self, mem: &A, cpu: &mut Cpu) {
        // Self-modifying code (ART JIT, linkers patching PLTs) announces new
        // instructions with IC IVAU; any thread's IC bumps the global epoch.
        let generation = mem
            .generation()
            .wrapping_add(CODE_EPOCH.load(core::sync::atomic::Ordering::Acquire) << 40);
        let space = mem as *const A as *const () as usize;
        if self.slots.is_empty() {
            self.slots.resize_with(4096, || None);
            self.generation = generation;
            self.space = space;
        } else if self.generation != generation || self.space != space {
            if self.space != space {
                cpu.tlb.flush();
            }
            cpu.tlb.sync(mem);
            self.slots.iter_mut().for_each(|x| *x = None);
            self.generation = generation;
            self.space = space;
        }
    }
    pub fn get<A: AddressSpace + ?Sized>(
        &mut self,
        c: &mut Cpu,
        mem: &A,
        pc: u64,
        hint: Option<(u64, usize)>,
    ) -> Result<&Block, crate::cpu::Exit> {
        let index = hint.filter(|(target, _)| *target == pc).map_or_else(
            || ((pc >> 2) as usize) & (self.slots.len() - 1),
            |(_, index)| index,
        );
        if self.slots[index].as_ref().is_none_or(|b| b.pc != pc) {
            let p = c
                .tlb
                .lookup(mem, pc, Access::Exec)
                .ok_or(crate::cpu::Exit::Fault {
                    addr: pc,
                    access: Access::Exec,
                })?;
            static DISABLE_AOT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if !*DISABLE_AOT.get_or_init(|| std::env::var_os("INDROID_DISABLE_AOT").is_some()) {
                if let Some(unit) = c.tlb.executable_unit(mem, pc) {
                    self.slots[index] = Some(Block {
                        pc,
                        ops: Vec::new(),
                        successor: None,
                        aot: Some(unit),
                    });
                    return Ok(self.slots[index].as_ref().unwrap());
                }
            }
            let limit = ((PAGE_SIZE - (pc & (PAGE_SIZE - 1))) / 4).min(32) as usize;
            // Misaligned PCs are fetched by the ordinary path, which preserves its faults.
            let mut ops = Vec::with_capacity(limit.max(1));
            for n in 0..limit.max(1) {
                let i = unsafe { (p.add(n * 4) as *const u32).read_unaligned() };
                ops.push(Op::decode(i));
                if ((i >> 25) & 15) & 0xe == 0xa {
                    break;
                }
            }
            // Cache a checked slot handle for direct branches. Slot handles remain
            // valid across replacement; the destination PC is checked on entry.
            let successor = ops.last().filter(|op| op.kind == 3).map(|op| {
                let branch_pc = pc.wrapping_add((ops.len() as u64 - 1) * 4);
                let target = branch_pc.wrapping_add(op.imm);
                (target, ((target >> 2) as usize) & (self.slots.len() - 1))
            });
            self.slots[index] = Some(Block {
                pc,
                ops,
                successor,
                aot: None,
            });
        }
        Ok(self.slots[index].as_ref().unwrap())
    }
}
