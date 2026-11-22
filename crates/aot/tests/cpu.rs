//! Run static ELFs through generated units and compare with QEMU.
#[path = "../../a64/tests/support/mod.rs"]
mod support;
use indroid_a64::{AddressSpace, Cpu, Exit};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
static UNIT: Mutex<Option<indroid_a64::mem::AotUnit>> = Mutex::new(None);
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn counted(c: &mut Cpu, m: &dyn AddressSpace, b: u64) -> Option<Exit> {
    CALLS.fetch_add(1, Ordering::Relaxed);
    (UNIT.lock().unwrap().unwrap().execute)(c, m, b)
}
#[test]
fn generated_static_elf_matches_qemu() {
    let Some(root) = std::env::var_os("INDROID_AOT_ROM") else {
        eprintln!("no generated test profile");
        return;
    };
    let mut tested = 0;
    for name in ["hello", "simd_crypto"] {
        let guest_path = format!("/bin/{name}");
        let Some(unit) = indroid_aot::lookup(&guest_path, 0) else {
            continue;
        };
        *UNIT.lock().unwrap() = Some(unit);
        let before = CALLS.load(Ordering::Relaxed);
        let path = std::path::Path::new(&root).join("bin").join(name);
        let elf = std::fs::read(&path).unwrap();
        let qemu = std::process::Command::new("qemu-aarch64-static")
            .arg(&path)
            .output()
            .unwrap();
        let o = support::run_elf_with_aot(&elf, &[name], 20_000_000_000, |offset| {
            if offset == 0 {
                Some(indroid_a64::mem::AotUnit {
                    execute: counted,
                    ..unit
                })
            } else {
                None
            }
        });
        assert_eq!(o.stdout, qemu.stdout);
        assert_eq!(Some(o.exit_code), qemu.status.code());
        assert!(
            CALLS.load(Ordering::Relaxed) > before,
            "generated {name} unit was not entered"
        );
        tested += 1;
    }
    assert!(tested > 0, "test profile did not select a CPU test ELF");
}

#[test]
fn generated_simd_batch_preserves_budgets() {
    use indroid_a64::{Cpu, Exit};
    let Some(root) = std::env::var_os("INDROID_AOT_ROM") else {
        return;
    };
    let Some(unit) = indroid_aot::lookup("/bin/batch", 0) else {
        return;
    };
    let bytes = std::fs::read(std::path::Path::new(&root).join("bin/batch")).unwrap();
    assert_eq!(bytes.len(), 4096);
    let plain = support::Pages::new();
    let aot = support::Pages::new();
    plain.map(0x1000, 4096, 7);
    aot.map(0x1000, 4096, 7);
    plain.write(0x1000, &bytes);
    aot.write(0x1000, &bytes);
    aot.set_aot(1, unit);
    for start in [0, 4, 8] {
        for budget in 1..=4 {
            let mut a = Cpu::new();
            a.pc = 0x1000 + start;
            for (i, v) in a.v.iter_mut().enumerate() {
                *v = 0x7766554433221100ffeeddccbbaa9988u128.rotate_left(i as u32);
            }
            let mut b = a.clone();
            let x = a.run(&plain, budget);
            let y = b.run(&aot, budget);
            assert_eq!(x, y, "start={start} budget={budget}");
            assert_eq!(a.pc, b.pc);
            assert_eq!(a.retired, b.retired);
            assert_eq!(a.v, b.v);
            assert!(matches!(y, Exit::Budget | Exit::Brk(0)));
        }
    }
}
