//! Runs tests/oracle.sh (JVM vs indroid differential tests) when a JDK and the
//! Android build tools are available.

#[test]
fn oracle() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let have_java = std::process::Command::new("javac")
        .arg("-version")
        .output()
        .is_ok();
    let sdk = std::env::var("ANDROID_HOME")
        .unwrap_or_else(|_| format!("{}/android-sdk", std::env::var("HOME").unwrap_or_default()));
    if !have_java || !std::path::Path::new(&sdk).join("build-tools").exists() {
        eprintln!("skipping oracle tests: need javac and Android build-tools");
        return;
    }
    let st = std::process::Command::new("bash")
        .arg(format!("{root}/tests/oracle.sh"))
        .status()
        .expect("run oracle.sh");
    assert!(st.success(), "oracle tests failed");
}

/// Runs tests/kotlin.sh (Kotlin programs + kotlin-stdlib vs the JVM) when
/// kotlinc is available (on PATH, $KOTLINC, or ~/tools/kotlinc).
#[test]
fn kotlin_oracle() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let home = std::env::var("HOME").unwrap_or_default();
    let have_kotlinc = std::env::var("KOTLINC").is_ok()
        || std::process::Command::new("kotlinc")
            .arg("-version")
            .output()
            .is_ok()
        || std::path::Path::new(&format!("{home}/tools/kotlinc/bin/kotlinc")).exists();
    let sdk = std::env::var("ANDROID_HOME").unwrap_or_else(|_| format!("{home}/android-sdk"));
    if !have_kotlinc || !std::path::Path::new(&sdk).join("build-tools").exists() {
        eprintln!("skipping kotlin tests: need kotlinc and Android build-tools");
        return;
    }
    let st = std::process::Command::new("bash")
        .arg(format!("{root}/tests/kotlin.sh"))
        .status()
        .expect("run kotlin.sh");
    assert!(st.success(), "kotlin tests failed");
}
