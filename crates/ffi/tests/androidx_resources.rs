use indroid_ffi::App;

#[test]
fn androidx_theme_contains_app_and_framework_attributes() {
    let apk = std::fs::read(format!(
        "{}/../../tests/fixtures/androidx-hello-release.apk",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let archive = indroid_apk::Apk::open(apk.clone()).unwrap();
    let app = App::load(apk, false).unwrap();
    let theme = *app.vm.manifest_themes.get("").expect("application theme");
    let resources = app.vm.resources.as_ref().unwrap();
    let primary = resources.identifier("colorPrimary", "attr").unwrap();
    let window = resources
        .identifier("windowBackground", "android:attr")
        .unwrap();
    let xml = resources
        .xml("res/color/abc_hint_foreground_material_light.xml")
        .unwrap();
    assert!(xml.starts_with("\u{1}selector"));
    assert!(xml.contains("\u{5}1684"));
    assert!(
        archive
            .entries()
            .iter()
            .filter(|path| path.starts_with("res/") && path.ends_with(".xml"))
            .filter_map(|path| resources.xml(path))
            .any(|xml| xml.contains("\u{4}a:")),
        "missing XML theme attribute"
    );
    let style = resources.style(theme).expect("theme bag");
    assert!(
        style.iter().any(|(id, _)| *id == primary),
        "missing colorPrimary"
    );
    assert!(
        style.iter().any(|(id, _)| *id == window),
        "missing windowBackground"
    );
}
