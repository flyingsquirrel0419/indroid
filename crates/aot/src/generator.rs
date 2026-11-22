use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path};

/// Generate Rust units from the profiler's hottest file-backed pages.
pub fn generate(
    profile: &Path,
    rom: &Path,
    limit: usize,
    mut out: impl Write,
) -> io::Result<usize> {
    let text = std::fs::read_to_string(profile)?;
    writeln!(out, "use indroid_a64::{{Cpu, Exit, AddressSpace}};")?;
    let mut pages = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        if pages.len() >= limit {
            break;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 3 || fields[1].starts_with('[') {
            continue;
        }
        let path = Path::new(fields[1].trim_start_matches('/'));
        if path
            .components()
            .any(|x| !matches!(x, Component::Normal(_)))
        {
            continue;
        }
        let offset = match u64::from_str_radix(fields[2].trim_start_matches("0x"), 16) {
            Ok(v) if v % 4096 == 0 => v,
            _ => continue,
        };
        if pages
            .iter()
            .any(|(p, o, _, _)| p == fields[1] && *o == offset)
        {
            continue;
        }
        let mut image = match std::fs::File::open(rom.join(path)) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut page = [0u8; 4096];
        if image
            .seek(SeekFrom::Start(offset))
            .and_then(|_| image.read_exact(&mut page))
            .is_err()
        {
            continue;
        }
        let bytes = &page[..];
        let mut hash = 0xcbf29ce484222325u64;
        for b in bytes {
            hash = (hash ^ *b as u64).wrapping_mul(0x100000001b3);
        }
        let n = pages.len();
        writeln!(out, "static BYTES_{n}: [u8; 4096] = {:?};", bytes)?;
        writeln!(
            out,
            "fn unit_{n}(cpu: &mut Cpu, mem: &dyn AddressSpace, budget: u64) -> Option<Exit> {{"
        )?;
        writeln!(out,"let base = cpu.pc & !4095; let end = cpu.retired.saturating_add(budget); let generation = mem.generation();")?;
        writeln!(out,"while cpu.retired < end && cpu.pc & !4095 == base && cpu.pc & 3 == 0 && mem.generation() == generation {{")?;
        writeln!(
            out,
            "let entry = ((cpu.pc & 4095) >> 2) as usize; match entry / 8 {{"
        )?;
        for (group, chunk) in bytes.as_chunks::<32>().0.iter().enumerate() {
            writeln!(out, "{group} => {{")?;
            let words: Vec<u32> = chunk
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| u32::from_le_bytes(*word))
                .collect();
            let is_simple = |word: u32| {
                matches!(
                    word & 0xffff_fc00,
                    0x4e28_4800 | 0x4e28_5800 | 0x4e28_6800 | 0x4e28_7800
                ) || matches!(word & 0xbfe0_fc00, 0x2e20_1c00 | 0x0ea0_1c00)
            };
            let mut j = 0;
            while j < words.len() {
                let start = j;
                if is_simple(words[j]) {
                    while j < words.len() && is_simple(words[j]) {
                        j += 1;
                    }
                } else {
                    j += 1;
                }
                let first = group * 8 + start;
                let last = group * 8 + j - 1;
                if j - start >= 2 {
                    // SIMD register-only operations cannot fault or branch. When
                    // budget permits, execute the suffix and retire it as a unit.
                    writeln!(out, "if entry <= {last} {{ let first = entry.max({first}); let count = ({last} + 1 - first) as u64; if end - cpu.retired >= count {{")?;
                    for (k, word) in words[start..j].iter().enumerate() {
                        let index = first + k;
                        writeln!(out,"if entry <= {index} {{ indroid_a64::aot::aot_simd_unretired(cpu,0x{word:08x}); }}")?;
                    }
                    writeln!(
                        out,
                        "cpu.pc=base.wrapping_add({}); cpu.retired+=count; if cpu.retired >= end {{ continue; }} }} else {{",
                        (last + 1) * 4
                    )?;
                    for (k, word) in words[start..j].iter().enumerate() {
                        let index = first + k;
                        writeln!(out,"if entry <= {index} {{ if cpu.retired >= end {{ continue; }} indroid_a64::aot::aot_simd_simple(cpu,0x{word:08x}); }}")?;
                    }
                    writeln!(out, "if cpu.retired >= end {{ continue; }} }} }}")?;
                } else {
                    let word = words[start];
                    let next = (first + 1) * 4;
                    if word == 0 {
                        writeln!(
                            out,
                            "if entry <= {first} {{ return Some(Exit::Undefined(0)); }}"
                        )?;
                    } else if is_simple(word) {
                        writeln!(out,"if entry <= {first} {{ indroid_a64::aot::aot_simd_simple(cpu,0x{word:08x}); if cpu.pc != base.wrapping_add({next}) || cpu.retired >= end {{ continue; }} }}")?;
                    } else {
                        writeln!(out,"if entry <= {first} {{ if let Some(e)=indroid_a64::aot::aot_step(cpu,mem,0x{word:08x}) {{ return Some(e); }} if cpu.pc != base.wrapping_add({next}) || cpu.retired >= end || mem.generation() != generation {{ continue; }} }}")?;
                    }
                }
            }
            writeln!(out, "}},")?;
        }
        writeln!(out, "_ => return None, }} }}")?;
        writeln!(
            out,
            "if cpu.retired >= end {{ Some(Exit::Budget) }} else {{ None }} }}"
        )?;
        pages.push((fields[1].to_string(), offset, hash, n));
    }
    writeln!(out, "pub const HAS_UNITS: bool = {};", !pages.is_empty())?;
    writeln!(out,"pub fn lookup(path: &str, offset: u64) -> Option<indroid_a64::mem::AotUnit> {{ match (path, offset) {{")?;
    for (path, offset, hash, n) in &pages {
        writeln!(out,"({path:?}, {offset}) => Some(indroid_a64::mem::AotUnit {{ fingerprint: {hash}, bytes: &BYTES_{n}, execute: unit_{n} }}),")?;
    }
    writeln!(out, "_ => None, }} }}")?;
    Ok(pages.len())
}
