//! Runs static aarch64 test programs on the neo kernel (no ROM needed: the
//! binary is placed in a temporary root).

use std::process::Command;
use std::sync::{Arc, Mutex};

fn run(name: &str) -> Option<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/c");
    let tmp = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("neo-{name}"));
    let _ = std::fs::remove_dir_all(&tmp);
    let rom = tmp.join("rom");
    std::fs::create_dir_all(rom.join("bin")).unwrap();
    let bin = rom.join("bin").join(name);
    let ok = Command::new("aarch64-linux-gnu-gcc")
        .args(["-O2", "-static", "-pthread", "-o"])
        .arg(&bin)
        .arg(dir.join(format!("{name}.c")))
        .status();
    match ok {
        Ok(s) if s.success() => {}
        Ok(_) => panic!("compile {name} failed"),
        Err(_) => {
            eprintln!("skipping: no aarch64-linux-gnu-gcc");
            return None;
        }
    }
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let o2 = out.clone();
    let k = indroid_kernel::Kernel::new(indroid_kernel::Config {
        rom: rom.clone(),
        state: tmp.join("state"),
        overlays: vec![],
        console: Arc::new(move |b: &[u8]| o2.lock().unwrap().extend_from_slice(b)),
    });
    let path = format!("/bin/{name}");
    k.spawn(&path, &[&path], &[]).expect("spawn");
    assert!(
        k.wait_idle(Some(std::time::Duration::from_secs(60))),
        "{name} timed out"
    );
    let s = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    Some(s)
}

#[test]
fn threads() {
    let Some(o) = run("threads") else { return };
    assert!(o.contains("counter=80000 atomic=80000 sum=6"), "{o}");
}

#[test]
fn procs() {
    let Some(o) = run("procs") else { return };
    for want in [
        "read 13 'child says hi' exit=7",
        "handler got 10",
        "shared=42 private=1",
        "mremap kept 5",
        "exec fail exit=3",
        "segv child signaled=1 sig=11",
    ] {
        assert!(o.contains(want), "missing {want:?} in:\n{o}");
    }
}
