use indroid_ffi::{indroid_set_text_measurer, App};
use std::ffi::{c_char, CStr};

unsafe extern "C" fn measure(
    text: *const c_char,
    size: f32,
    flags: i32,
    max_width: f32,
    out: *mut f32,
) {
    assert_eq!(CStr::from_ptr(text).to_str().unwrap(), "test");
    *out = size + flags as f32;
    *out.add(1) = max_width;
}

#[test]
fn layout_hooks_work_for_loaded_apps() {
    let bytes = std::fs::read(format!(
        "{}/../../tests/fixtures/sample.apk",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let mut app = App::load(bytes, false).unwrap();
    let fallback = app.vm.text_measurer.as_ref().unwrap()("test", 12.0, 0, 100.0);
    indroid_set_text_measurer(Some(measure));
    assert_eq!(
        app.vm.text_measurer.as_ref().unwrap()("test", 12.0, 3, 100.0),
        (15.0, 100.0)
    );
    indroid_set_text_measurer(None);
    assert_eq!(
        app.vm.text_measurer.as_ref().unwrap()("test", 12.0, 0, 100.0),
        fallback
    );
    let _: serde_json::Value = serde_json::from_str(&app.launch()).unwrap();
    let ui: serde_json::Value = serde_json::from_str(&app.set_window_size(250, 400)).unwrap();
    assert!(ui["error"].is_null(), "{ui}");
    assert_eq!(ui["root"]["frame"], serde_json::json!([0, 0, 250, 400]));
}
