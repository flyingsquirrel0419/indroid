use indroid_apk::{axml::Document, resources::DimensionUnit, Apk, ResValue};
use std::io::{Cursor, Write};

#[test]
fn sample_apk() {
    let apk = Apk::open(include_bytes!("../../../tests/fixtures/sample.apk").to_vec()).unwrap();
    assert!(!apk.dex_files().is_empty());
    let manifest = apk.manifest().unwrap();
    assert_eq!(manifest.package, "dev.indroid.sample");
    assert_eq!(
        manifest.main_activity.as_deref(),
        Some("dev.indroid.sample.MainActivity")
    );
    assert_eq!(manifest.min_sdk, Some(21));
    let resources = apk.resources().unwrap();
    let label = match manifest.label.unwrap() {
        ResValue::Reference(id) => id,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(resources.string(label).as_deref(), Some("Indroid Sample"));
    let id = resources.id_by_name("layout", "activity_main").unwrap();
    assert_eq!(resources.name(id), Some("layout/activity_main"));
    let path = resources.file_path(id).unwrap();
    let doc = Document::parse(&apk.read(&path).unwrap()).unwrap();
    let root = doc.root;
    assert_eq!(root.name, "LinearLayout");
    assert_eq!(
        root.attribute("orientation").unwrap().value,
        ResValue::Int(1)
    );
    assert_eq!(
        root.attribute("padding").unwrap().value,
        ResValue::Dimension {
            value: 16.0,
            unit: DimensionUnit::Dp
        }
    );
    assert_eq!(
        root.children
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["TextView", "Button", "EditText"]
    );
    let title = &root.children[0];
    assert!(
        matches!(title.attribute("id").unwrap().value,ResValue::Reference(id) if id >> 24 == 0x7f)
    );
    assert_eq!(
        title.attribute("textSize").unwrap().value,
        ResValue::Dimension {
            value: 24.0,
            unit: DimensionUnit::Sp
        }
    );
    let text = match title.attribute("text").unwrap().value {
        ResValue::Reference(id) => id,
        _ => panic!(),
    };
    assert_eq!(
        resources.string(text).as_deref(),
        Some("Hello from Android")
    );
    assert_eq!(
        root.children[1].attribute("text").unwrap().value,
        ResValue::String("Tap".into())
    );
    assert_eq!(
        root.children[2].attribute("hint").unwrap().value,
        ResValue::String("Your name".into())
    );
}
#[test]
fn dex_order_and_invalid_inputs() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in [
        "classes10.dex",
        "classes2.dex",
        "classes.dex",
        "classes1.dex",
        "classes02.dex",
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(name.as_bytes()).unwrap();
    }
    let apk = Apk::open(zip.finish().unwrap().into_inner()).unwrap();
    assert_eq!(
        apk.dex_files(),
        [
            b"classes.dex".to_vec(),
            b"classes2.dex".to_vec(),
            b"classes10.dex".to_vec()
        ]
    );
    for n in 0..32 {
        assert!(Document::parse(&vec![0; n]).is_err());
        assert!(indroid_apk::Resources::parse(&vec![0; n]).is_err());
    }
}

#[test]
fn truncated_real_files_are_rejected_without_panicking() {
    let apk = Apk::open(include_bytes!("../../../tests/fixtures/sample.apk").to_vec()).unwrap();
    for name in ["AndroidManifest.xml", "resources.arsc"] {
        let bytes = apk.read(name).unwrap();
        for n in 0..bytes.len() {
            let result = std::panic::catch_unwind(|| {
                if name == "AndroidManifest.xml" {
                    assert!(Document::parse(&bytes[..n]).is_err());
                } else {
                    assert!(indroid_apk::Resources::parse(&bytes[..n]).is_err());
                }
            });
            assert!(result.is_ok(), "panic at {name} length {n}");
        }
    }
}
