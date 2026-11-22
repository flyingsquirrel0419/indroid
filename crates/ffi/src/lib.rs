//! Host bridge: loads an APK into the VM, drives the Activity lifecycle, and
//! exposes a C ABI (`include/indroid.h`) for the iOS app.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicPtr, Ordering};

mod image_size;
mod manifest_themes;
pub mod neo;

type MeasureTextFn = unsafe extern "C" fn(*const c_char, f32, i32, f32, *mut f32);
static TEXT_MEASURER: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Set or clear the process-wide text measurement callback.
#[no_mangle]
pub extern "C" fn indroid_set_text_measurer(callback: Option<MeasureTextFn>) {
    TEXT_MEASURER.store(
        callback.map_or(std::ptr::null_mut(), |f| f as *mut ()),
        Ordering::Release,
    );
}

use indroid_apk::axml::{Document, Element};
use indroid_apk::resources::{DimensionUnit, ResValue};
use indroid_apk::{Apk, Manifest, Resources};
use indroid_vm::{ResourceProvider, Throw, Vm};

/// Resources backed by the APK.
pub struct ApkResources {
    apk: Rc<Apk>,
    res: Option<Rc<Resources>>,
    package: String,
}

fn unit(u: DimensionUnit) -> &'static str {
    match u {
        DimensionUnit::Px => "px",
        DimensionUnit::Dp => "dp",
        DimensionUnit::Sp => "sp",
        DimensionUnit::Pt => "pt",
        DimensionUnit::In => "in",
        DimensionUnit::Mm => "mm",
        DimensionUnit::Unknown(_) => "px",
    }
}

fn dimen_px(value: f32, u: DimensionUnit) -> f32 {
    match u {
        DimensionUnit::Pt => value * 160.0 / 72.0,
        DimensionUnit::In => value * 160.0,
        DimensionUnit::Mm => value * 160.0 / 25.4,
        _ => value,
    }
}

/// Attributes whose references are identities, not values to resolve.
const REF_ATTRS: &[&str] = &[
    "id",
    "layout",
    "src",
    "srcCompat",
    "style",
    "theme",
    "layout_below",
    "layout_above",
    "layout_toLeftOf",
    "layout_toRightOf",
    "layout_toStartOf",
    "layout_toEndOf",
    "layout_alignTop",
    "layout_alignBottom",
    "layout_alignLeft",
    "layout_alignRight",
    "layout_alignStart",
    "layout_alignEnd",
    "background",
    "foreground",
    "button",
    "thumb",
    "track",
    "progressDrawable",
    "divider",
    "listSelector",
    "drawableLeft",
    "drawableRight",
    "drawableTop",
    "drawableBottom",
    "drawableStart",
    "drawableEnd",
    "layout_alignBaseline",
    "labelFor",
    "nextFocusDown",
    "entries",
    "menu",
];

impl ApkResources {
    fn resolve(&self, v: &ResValue) -> ResValue {
        match (v, &self.res) {
            (ResValue::Reference(id), Some(r)) => r.resolve(*id).unwrap_or_else(|| v.clone()),
            _ => v.clone(),
        }
    }

    fn encode(&self, name: &str, v: &ResValue) -> Option<String> {
        if let ResValue::Reference(id) = v {
            if REF_ATTRS.contains(&name) {
                return Some(format!("r:{}", *id as i32));
            }
            // References to ids, styles, layouts, arrays, etc. are identities.
            let ty = (*id >> 16) & 0xff;
            let kind = self.res.as_ref().and_then(|r| r.name(*id)).map(|n| {
                let n = n.rsplit(':').next().unwrap_or(n);
                n.split('/').next().unwrap_or("").to_string()
            });
            if matches!(
                kind.as_deref(),
                Some(
                    "id" | "style"
                        | "layout"
                        | "array"
                        | "menu"
                        | "xml"
                        | "anim"
                        | "animator"
                        | "transition"
                        | "navigation"
                        | "raw"
                        | "font"
                )
            ) || (kind.is_none() && ty == 0)
            {
                return Some(format!("r:{}", *id as i32));
            }
        }
        let rv = self.resolve(v);
        if let (ResValue::Reference(id), ResValue::String(path)) = (v, &rv) {
            if path.starts_with("res/") {
                return Some(format!("r:{}", *id as i32));
            }
        }
        Some(match rv {
            ResValue::Null => return None,
            ResValue::Attribute(id) => format!("a:{}", id as i32),
            ResValue::String(s) => {
                format!(
                    "s:{}",
                    s.replace(['\u{1}', '\u{2}', '\u{3}', '\u{4}', '\u{5}'], "")
                )
            }
            ResValue::Reference(id) => format!("r:{}", id as i32),
            ResValue::Int(i) => format!("i:{i}"),
            ResValue::Hex(h) => format!("i:{}", h as i32),
            ResValue::Bool(b) => format!("b:{b}"),
            ResValue::Dimension { value, unit: u } => {
                format!("d:{}{}", value, unit(u))
            }
            ResValue::Fraction { value, unit } => format!(
                "q:{value}{}",
                if matches!(unit, indroid_apk::resources::FractionUnit::Parent) {
                    "p"
                } else {
                    ""
                }
            ),
            ResValue::Color(c) => format!("c:{}", c as i32),
            ResValue::Float(f) => format!("f:{f}"),
            ResValue::Unknown { data, .. } => format!("i:{}", data as i32),
        })
    }

    fn serialize(&self, e: &Element, out: &mut String) {
        out.push('\u{1}');
        out.push_str(&e.name);
        for a in &e.attributes {
            if a.name.is_empty() {
                continue;
            }
            // Only android:* and app:* (custom view) attributes plus un-namespaced ones (style, class, layout)
            if let Some(v) = self.encode(&a.name, &a.value) {
                out.push('\u{2}');
                out.push_str(&a.name);
                if let Some(id) = a.resource_id {
                    out.push('\u{5}');
                    out.push_str(&(id as i32).to_string());
                }
                out.push('\u{4}');
                out.push_str(&v);
            }
        }
        for c in &e.children {
            self.serialize(c, out);
        }
        out.push('\u{3}');
    }
}

impl ResourceProvider for ApkResources {
    fn string(&self, id: u32) -> Option<String> {
        match self.resolve(&ResValue::Reference(id)) {
            ResValue::String(s) => Some(s),
            ResValue::Int(i) => Some(i.to_string()),
            _ => self.res.as_ref()?.string(id),
        }
    }
    fn layout(&self, id: u32) -> Option<String> {
        let path = self.res.as_ref()?.file_path(id)?;
        let bytes = self.apk.read(&path)?;
        let doc = Document::parse(&bytes).ok()?;
        let mut out = String::with_capacity(bytes.len() / 2);
        self.serialize(&doc.root, &mut out);
        Some(out)
    }
    fn name(&self, id: u32) -> Option<String> {
        let n = self.res.as_ref()?.name(id)?;
        Some(if id >> 24 == 1 {
            format!("android:{n}")
        } else if n.contains(':') {
            n.to_string()
        } else {
            format!("{}:{}", self.package, n)
        })
    }
    fn int(&self, id: u32, _kind: i32) -> Option<i32> {
        match self.resolve(&ResValue::Reference(id)) {
            ResValue::Color(c) => Some(c as i32),
            ResValue::Int(i) => Some(i),
            ResValue::Hex(h) => Some(h as i32),
            ResValue::Bool(b) => Some(b as i32),
            _ => None,
        }
    }
    fn dimen(&self, id: u32) -> Option<f32> {
        match self.resolve(&ResValue::Reference(id)) {
            ResValue::Dimension { value, unit } => Some(dimen_px(value, unit)),
            ResValue::Float(f) => Some(f),
            ResValue::Int(i) => Some(i as f32),
            _ => None,
        }
    }
    fn identifier(&self, name: &str, ty: &str) -> Option<u32> {
        self.res.as_ref()?.id_by_name(ty, name)
    }
    fn string_array(&self, id: u32) -> Option<Vec<String>> {
        self.res.as_ref()?.string_array(id)
    }
    fn style(&self, id: u32) -> Option<Vec<(u32, String)>> {
        let res = self.res.as_ref()?;
        res.bag(id)?;
        Some(
            res.style(id)
                .into_iter()
                .filter_map(|(id, v)| self.encode("", &v).map(|v| (id, v)))
                .collect(),
        )
    }
    fn value(&self, id: u32) -> Option<String> {
        let res = self.res.as_ref()?;
        if res.bag(id).is_some() {
            return Some(format!("r:{}", id as i32));
        }
        let v = res.get(id)?;
        let resolved = self.resolve(&v);
        if let ResValue::String(s) = &resolved {
            if s.starts_with("res/") {
                return Some(format!("p:{s}"));
            }
        }
        self.encode("", &v)
    }
    fn xml(&self, path: &str) -> Option<String> {
        let bytes = self.apk.read(path)?;
        let doc = Document::parse(&bytes).ok()?;
        let mut out = String::new();
        self.serialize(&doc.root, &mut out);
        Some(out)
    }
    fn attr_name(&self, id: u32) -> Option<String> {
        let n = self.res.as_ref()?.name(id)?.strip_prefix("attr/")?;
        Some(if id >> 24 == 1 {
            format!("android:{n}")
        } else {
            n.to_owned()
        })
    }
    fn bag_parent(&self, id: u32) -> Option<u32> {
        Some(self.res.as_ref()?.bag(id)?.parent)
    }
}

/// Files under `<root>/<package>/` (paths from the VM are sanitized).
pub struct DirStorage {
    root: std::path::PathBuf,
}

impl DirStorage {
    pub fn new(root: impl Into<std::path::PathBuf>) -> DirStorage {
        DirStorage { root: root.into() }
    }
    fn path(&self, rel: &str) -> Option<std::path::PathBuf> {
        let mut p = self.root.clone();
        for part in rel.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
                return None;
            }
            p.push(part);
        }
        Some(p)
    }
}

impl indroid_vm::Storage for DirStorage {
    fn load(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.path(path)?).ok()
    }
    fn save(&self, path: &str, data: &str) -> bool {
        let Some(p) = self.path(path) else {
            return false;
        };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, data)
            .and_then(|_| std::fs::rename(&tmp, &p))
            .is_ok()
    }
}

/// A loaded Android app.
pub struct App {
    pub vm: Vm,
    apk: Rc<Apk>,
    res_index: Option<Rc<Resources>>,
    pub manifest: Manifest,
    pub label: String,
    log: Rc<RefCell<String>>,
    launched: bool,
}

impl App {
    /// Loads an APK. `echo` also forwards log output to stdout/stderr.
    pub fn load(bytes: Vec<u8>, echo: bool) -> Result<App, String> {
        let t0 = std::time::Instant::now();
        let apk = Rc::new(Apk::open(bytes).map_err(|e| format!("bad apk: {e:?}"))?);
        let manifest = apk.manifest().map_err(|e| format!("bad manifest: {e:?}"))?;
        let res = apk.resources().ok().map(Rc::new);
        let res_index = res.clone();
        let provider = ApkResources {
            apk: apk.clone(),
            res,
            package: manifest.package.clone(),
        };
        let label = match &manifest.label {
            Some(ResValue::String(s)) => s.clone(),
            Some(v) => match provider.resolve(v) {
                ResValue::String(s) => s,
                _ => manifest.package.clone(),
            },
            None => manifest.package.clone(),
        };
        let mut vm = Vm::new();
        for d in apk.dex_files() {
            vm.add_dex(Rc::from(d))?;
        }
        vm.resources = Some(Box::new(provider));
        if let Some(m) = apk.read("AndroidManifest.xml") {
            vm.manifest_themes = manifest_themes::read(&m, &manifest.package);
        }
        vm.text_measurer = Some(Box::new(|text, size, flags, max_width| {
            let ptr = TEXT_MEASURER.load(Ordering::Acquire);
            if ptr.is_null() {
                return indroid_vm::metrics::approximate(text, size, flags, max_width);
            }
            let text = CString::new(text.replace('\0', "\u{fffd}")).unwrap();
            let mut out = [0.0; 2];
            let callback: MeasureTextFn = unsafe { std::mem::transmute(ptr) };
            unsafe { callback(text.as_ptr(), size, flags, max_width, out.as_mut_ptr()) };
            (out[0], out[1])
        }));
        let image_apk = apk.clone();
        let image_resources = res_index.clone();
        vm.image_sizer = Some(Box::new(move |id| {
            let path = image_resources.as_ref()?.file_path(id)?;
            let bytes = image_apk.read(&path)?;
            image_size::size_dp(&path, &bytes)
        }));
        let log = Rc::new(RefCell::new(String::new()));
        let sink = log.clone();
        vm.out = Box::new(move |fd, s| {
            let mut l = sink.borrow_mut();
            if fd == 2 && !s.is_empty() {
                l.push_str("E/System.err: ");
            } else if fd == 1 && !s.is_empty() {
                l.push_str("I/System.out: ");
            }
            l.push_str(s);
            if echo {
                if fd == 1 {
                    print!("{s}");
                } else {
                    eprint!("{s}");
                }
            }
        });
        if echo {
            eprintln!(
                "[indroid] loaded {} in {:?}",
                manifest.package,
                t0.elapsed()
            );
        }
        Ok(App {
            vm,
            apk,
            res_index,
            manifest,
            label,
            log,
            launched: false,
        })
    }

    fn call(&mut self, name_sig: &str, args: &[CallArg]) -> String {
        let mut raw = Vec::with_capacity(args.len());
        let pins = self.vm.pins.len();
        for a in args {
            match a {
                CallArg::Int(i) => raw.push(*i as u32),
                CallArg::Float(f) => raw.push(f.to_bits()),
                CallArg::Str(s) => match s {
                    Some(s) => {
                        let r = self.vm.new_string(s);
                        self.vm.pins.push(r);
                        raw.push(r);
                    }
                    None => raw.push(0),
                },
            }
        }
        let r = self
            .vm
            .call_static("Landroid/app/ActivityThread;", name_sig, &raw);
        self.vm.pins.truncate(pins);
        match r {
            Ok(v) => self
                .vm
                .rust_string(v as u32)
                .unwrap_or_else(|| error_json("no UI")),
            Err(Throw) => {
                let e = self.vm.describe_exception();
                self.log.borrow_mut().push_str(&format!("E/indroid: {e}\n"));
                error_json(&e)
            }
        }
    }

    pub fn launch(&mut self) -> String {
        let Some(activity) = self.manifest.main_activity.clone() else {
            return error_json("no launcher activity in AndroidManifest.xml");
        };
        self.launched = true;
        let pkg = self.manifest.package.clone();
        let label = self.label.clone();
        let app_class = self.manifest.application_class.clone();
        self.call(
            "launch(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[CallArg::Str(Some(pkg)), CallArg::Str(Some(label)), CallArg::Str(Some(activity)), CallArg::Str(app_class)],
        )
    }
    pub fn set_size(&mut self, id: i32, w: i32, h: i32) -> String {
        self.call(
            "setSize(III)Ljava/lang/String;",
            &[CallArg::Int(id), CallArg::Int(w), CallArg::Int(h)],
        )
    }
    pub fn set_window_size(&mut self, w: i32, h: i32) -> String {
        self.call(
            "setWindowSize(II)Ljava/lang/String;",
            &[CallArg::Int(w), CallArg::Int(h)],
        )
    }
    pub fn touch(&mut self, id: i32, action: i32, x: f32, y: f32) -> String {
        self.call(
            "touch(IIFF)Ljava/lang/String;",
            &[
                CallArg::Int(id),
                CallArg::Int(action),
                CallArg::Float(x),
                CallArg::Float(y),
            ],
        )
    }
    /// Touch in window coordinates, dispatched through the Java view tree.
    pub fn touch_window(&mut self, action: i32, x: f32, y: f32) -> String {
        self.call(
            "touchWindow(IFF)Ljava/lang/String;",
            &[CallArg::Int(action), CallArg::Float(x), CallArg::Float(y)],
        )
    }
    /// Native scroll view moved: sync the Java view's scroll offset.
    pub fn scroll(&mut self, id: i32, x: i32, y: i32) -> String {
        self.call(
            "scroll(III)Ljava/lang/String;",
            &[CallArg::Int(id), CallArg::Int(x), CallArg::Int(y)],
        )
    }
    /// Down + up at a window point (a tap).
    pub fn tap(&mut self, x: f32, y: f32) -> String {
        self.touch_window(0, x, y);
        self.touch_window(1, x, y)
    }
    pub fn click(&mut self, id: i32) -> String {
        self.call("click(I)Ljava/lang/String;", &[CallArg::Int(id)])
    }
    pub fn long_click(&mut self, id: i32) -> String {
        self.call("longClick(I)Ljava/lang/String;", &[CallArg::Int(id)])
    }
    pub fn set_text(&mut self, id: i32, text: &str) -> String {
        self.call(
            "setText(ILjava/lang/String;)Ljava/lang/String;",
            &[CallArg::Int(id), CallArg::Str(Some(text.into()))],
        )
    }
    pub fn editor_action(&mut self, id: i32) -> String {
        self.call("editorAction(I)Ljava/lang/String;", &[CallArg::Int(id)])
    }
    pub fn set_progress(&mut self, id: i32, p: i32) -> String {
        self.call(
            "setProgress(II)Ljava/lang/String;",
            &[CallArg::Int(id), CallArg::Int(p)],
        )
    }
    pub fn back(&mut self) -> String {
        self.call("back()Ljava/lang/String;", &[])
    }
    pub fn tick(&mut self) -> String {
        self.call("tick()Ljava/lang/String;", &[])
    }
    pub fn ui(&mut self) -> String {
        self.call("ui()Ljava/lang/String;", &[])
    }
    /// Raw bytes of a file resource (e.g. a PNG/WebP drawable), if it is a bitmap.
    pub fn resource_bytes(&self, id: u32) -> Option<Vec<u8>> {
        let mut id = id;
        let res = self.res_index.as_ref()?;
        for _ in 0..8 {
            match res.get(id)? {
                ResValue::Reference(next) => id = next,
                _ => break,
            }
        }
        let path = res.file_path(id)?;
        let lower = path.to_ascii_lowercase();
        if !(lower.ends_with(".png")
            || lower.ends_with(".webp")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".gif"))
        {
            return None;
        }
        self.apk.read(&path)
    }

    /// Persist app data under `<dir>/<package>/`.
    pub fn set_data_dir(&mut self, dir: &str) {
        let root = std::path::Path::new(dir).join(&self.manifest.package);
        self.vm.storage = Some(Box::new(DirStorage::new(root)));
    }
    pub fn take_log(&mut self) -> String {
        std::mem::take(&mut *self.log.borrow_mut())
    }
    pub fn info_json(&self) -> String {
        format!(
            "{{\"package\":{},\"label\":{},\"activity\":{}}}",
            json_str(&self.manifest.package),
            json_str(&self.label),
            self.manifest
                .main_activity
                .as_deref()
                .map(json_str)
                .unwrap_or_else(|| "null".into())
        )
    }
}

enum CallArg {
    Int(i32),
    Float(f32),
    Str(Option<String>),
}

pub fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn error_json(msg: &str) -> String {
    format!(
        "{{\"title\":\"indroid\",\"finished\":false,\"error\":{},\"next\":-1}}",
        json_str(msg)
    )
}

// ----------------------------------------------------------------------
// C ABI
// ----------------------------------------------------------------------

/// Opaque handle for C.
pub struct IndroidVm(App);

fn to_c(s: String) -> *mut c_char {
    CString::new(s.replace('\0', ""))
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn guard(f: impl FnOnce() -> String) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(s) => to_c(s),
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "VM panic".into());
            to_c(error_json(&format!("internal error: {msg}")))
        }
    }
}

/// # Safety
/// `apk` must point to `len` readable bytes; `err` may be null.
#[no_mangle]
pub unsafe extern "C" fn indroid_vm_new(
    apk: *const u8,
    len: usize,
    err: *mut *mut c_char,
) -> *mut IndroidVm {
    if apk.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(apk, len).to_vec();
    match catch_unwind(AssertUnwindSafe(|| App::load(bytes, false))) {
        Ok(Ok(app)) => Box::into_raw(Box::new(IndroidVm(app))),
        Ok(Err(e)) => {
            if !err.is_null() {
                *err = to_c(e);
            }
            std::ptr::null_mut()
        }
        Err(_) => {
            if !err.is_null() {
                *err = to_c("internal error while loading".into());
            }
            std::ptr::null_mut()
        }
    }
}

macro_rules! vm_fn {
    ($(#[$m:meta])* $name:ident ($($arg:ident : $ty:ty),*) => |$app:ident| $body:expr) => {
        $(#[$m])*
        /// # Safety
        /// `vm` must come from `indroid_vm_new` and be used from one thread.
        #[no_mangle]
        pub unsafe extern "C" fn $name(vm: *mut IndroidVm $(, $arg: $ty)*) -> *mut c_char {
            let Some(IndroidVm($app)) = vm.as_mut() else { return std::ptr::null_mut() };
            guard(|| $body)
        }
    };
}

vm_fn!(indroid_vm_launch() => |app| app.launch());
vm_fn!(indroid_vm_set_window_size(w: i32, h: i32) => |app| app.set_window_size(w, h));
vm_fn!(indroid_vm_set_size(view_id: i32, w: i32, h: i32) => |app| app.set_size(view_id, w, h));
vm_fn!(indroid_vm_touch(view_id: i32, action: i32, x: f32, y: f32) => |app| app.touch(view_id, action, x, y));
vm_fn!(indroid_vm_touch_window(action: i32, x: f32, y: f32) => |app| app.touch_window(action, x, y));
vm_fn!(indroid_vm_scroll(view_id: i32, x: i32, y: i32) => |app| app.scroll(view_id, x, y));
vm_fn!(indroid_vm_click(view_id: i32) => |app| app.click(view_id));
vm_fn!(indroid_vm_long_click(view_id: i32) => |app| app.long_click(view_id));
vm_fn!(indroid_vm_editor_action(view_id: i32) => |app| app.editor_action(view_id));
vm_fn!(indroid_vm_set_progress(view_id: i32, progress: i32) => |app| app.set_progress(view_id, progress));
vm_fn!(indroid_vm_back() => |app| app.back());
vm_fn!(indroid_vm_tick() => |app| app.tick());
vm_fn!(indroid_vm_take_log() => |app| app.take_log());
vm_fn!(indroid_vm_info() => |app| app.info_json());
vm_fn!(indroid_vm_set_data_dir(path: *const c_char) => |app| {
    if !path.is_null() {
        let p = CStr::from_ptr(path).to_string_lossy().into_owned();
        app.set_data_dir(&p);
    }
    String::from("{}")
});
vm_fn!(indroid_vm_set_text(view_id: i32, utf8: *const c_char) => |app| {
    let text = if utf8.is_null() { String::new() } else { CStr::from_ptr(utf8).to_string_lossy().into_owned() };
    app.set_text(view_id, &text)
});

/// Bitmap bytes for a drawable resource id; NULL if absent or not a bitmap.
/// Free with `indroid_bytes_free(ptr, len)`.
///
/// # Safety
/// `vm` must come from `indroid_vm_new`; `len` must be writable.
#[no_mangle]
pub unsafe extern "C" fn indroid_vm_resource_bytes(
    vm: *mut IndroidVm,
    id: i32,
    len: *mut usize,
) -> *mut u8 {
    let Some(IndroidVm(app)) = vm.as_mut() else {
        return std::ptr::null_mut();
    };
    match catch_unwind(AssertUnwindSafe(|| app.resource_bytes(id as u32))) {
        Ok(Some(bytes)) => {
            let mut b = bytes.into_boxed_slice();
            if !len.is_null() {
                *len = b.len();
            }
            let p = b.as_mut_ptr();
            std::mem::forget(b);
            p
        }
        _ => std::ptr::null_mut(),
    }
}

/// # Safety
/// `ptr`/`len` must come from `indroid_vm_resource_bytes`.
#[no_mangle]
pub unsafe extern "C" fn indroid_bytes_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }
}

/// # Safety
/// `s` must come from an indroid function (or be null).
#[no_mangle]
pub unsafe extern "C" fn indroid_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// # Safety
/// `vm` must come from `indroid_vm_new` (or be null); it is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn indroid_vm_free(vm: *mut IndroidVm) {
    if !vm.is_null() {
        drop(Box::from_raw(vm));
    }
}
