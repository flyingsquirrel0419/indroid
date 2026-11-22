fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let data = std::fs::read(&args[1]).expect("read resources.arsc");
    let resources = indroid_apk::Resources::parse(&data).expect("parse framework resources");
    std::fs::write(&args[2], resources.framework_snapshot()).expect("write snapshot");
}
