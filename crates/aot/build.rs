#[path = "src/generator.rs"]
mod generator;
use std::path::PathBuf;
fn main() {
    println!("cargo:rerun-if-env-changed=INDROID_AOT_PROFILE");
    println!("cargo:rerun-if-env-changed=INDROID_AOT_ROM");
    println!("cargo:rerun-if-env-changed=INDROID_AOT_LIMIT");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("units.rs");
    let mut file = std::fs::File::create(out).unwrap();
    if let (Some(profile), Some(rom)) = (
        std::env::var_os("INDROID_AOT_PROFILE"),
        std::env::var_os("INDROID_AOT_ROM"),
    ) {
        let profile = PathBuf::from(profile);
        println!("cargo:rerun-if-changed={}", profile.display());
        let limit = std::env::var("INDROID_AOT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let rom = PathBuf::from(rom);
        if let Ok(lines) = std::fs::read_to_string(&profile) {
            for line in lines.lines() {
                if let Some(path) = line.split_whitespace().nth(1) {
                    if path.starts_with('/') {
                        println!(
                            "cargo:rerun-if-changed={}",
                            rom.join(path.trim_start_matches('/')).display()
                        );
                    }
                }
            }
        }
        let n = generator::generate(&profile, &rom, limit, &mut file).expect("generate AOT pages");
        println!("cargo:warning=compiled {n} AOT pages");
    } else {
        use std::io::Write;
        writeln!(file,"pub const HAS_UNITS: bool = false; pub fn lookup(_: &str, _: u64) -> Option<indroid_a64::mem::AotUnit> {{ None }}").unwrap();
    }
}
