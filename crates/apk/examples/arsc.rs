fn main() {
    let p = std::env::args().nth(1).unwrap();
    let apk = indroid_apk::Apk::open(std::fs::read(p).unwrap()).unwrap();
    match apk.resources() {
        Ok(r) => {
            for id in [
                0x7f010000u32,
                0x7f020000,
                0x7f030000,
                0x7f030001,
                0x7f040000,
                0x7f040001,
                0x7f050000,
                0x7f050001,
            ] {
                println!(
                    "{id:#x} {:?} {:?} {:?}",
                    r.name(id),
                    r.get(id),
                    r.file_path(id)
                );
            }
        }
        Err(e) => println!("resources error: {e:?}"),
    }
}
