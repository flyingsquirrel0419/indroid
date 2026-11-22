//! indroid virtual machine: a JIT-free Dalvik bytecode interpreter.

use std::collections::HashMap;
use std::rc::Rc;

use indroid_dex::{access, Dex, EncodedValue, NO_INDEX};

mod debuginfo;
pub mod heap;
mod interp;
mod lambda;
pub mod metrics;
mod natives;
mod natives_android;
mod reflect;
pub mod regex;
pub mod text;
mod verify;

pub use heap::{JStr, Obj, ObjData, Ref};

/// The bundled class library (java.*, android.* shim).
pub static CORE_DEX: &[u8] = include_bytes!("../core.dex");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(pub u32);

pub type MethodIdx = u32;

/// Marker: a Java exception is pending in `Vm::exception`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Throw;

pub type JResult<T> = Result<T, Throw>;
pub type NativeFn = fn(&mut Vm, &[u32]) -> JResult<u64>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassState {
    Loaded,
    Initializing,
    Initialized,
    Failed,
}

#[derive(Debug, Clone)]
pub enum ClassSrc {
    Dex {
        dex: u16,
        def: u32,
    },
    Array {
        component: ClassId,
    },
    Primitive(u8),
    /// Generated at run time (lambda classes).
    Synthetic,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: Rc<str>,
    pub desc: Rc<str>,
    pub slot: u32,
    pub flags: u32,
}

#[derive(Debug)]
pub struct Class {
    pub name: Rc<str>,
    pub super_: Option<ClassId>,
    pub interfaces: Vec<ClassId>,
    pub flags: u32,
    pub src: ClassSrc,
    pub state: ClassState,
    pub ifields: Vec<FieldDef>,
    pub sfields: Vec<FieldDef>,
    pub statics: Vec<u32>,
    pub nslots: u32,
    pub ref_slots: Box<[u32]>,
    pub methods: Vec<MethodIdx>,
    pub vcache: HashMap<u32, Option<MethodIdx>>,
    pub mirror: Ref,
    pub source_file: Option<Rc<str>>,
}

impl Class {
    pub fn is_interface(&self) -> bool {
        self.flags & access::INTERFACE != 0
    }
    pub fn is_array(&self) -> bool {
        matches!(self.src, ClassSrc::Array { .. })
    }
    /// "java.lang.String" style name.
    pub fn java_name(&self) -> String {
        descriptor_to_java(&self.name)
    }
}

#[derive(Debug)]
pub struct TryEntry {
    pub start: u32,
    pub end: u32,
    /// (type_idx or NO_INDEX for catch-all, handler address)
    pub handlers: Box<[(u32, u32)]>,
}

#[derive(Debug)]
pub struct Code {
    pub regs: u16,
    pub ins: u16,
    pub insns: Box<[u16]>,
    pub tries: Box<[TryEntry]>,
    pub dex: u16,
    pub debug_info_off: u32,
}

#[derive(Clone)]
pub enum MKind {
    Code(Rc<Code>),
    Native(NativeFn),
    Abstract,
    /// Declared native with no implementation, or code_off missing.
    Missing,
    /// Failed structural verification.
    Invalid(Rc<str>),
    /// Functional-interface method of a lambda class (see lambda.rs).
    Lambda(Rc<lambda::Lambda>),
}

#[derive(Clone)]
pub struct Method {
    pub class: ClassId,
    pub name: Rc<str>,
    pub sig: Rc<str>,
    pub sig_id: u32,
    pub flags: u32,
    pub kind: MKind,
    /// Argument slots including `this`.
    pub arg_slots: u16,
    /// Return type descriptor first char.
    pub ret: u8,
    /// For lazy code loading: (dex, code_off).
    pub code_off: (u16, u32),
}

impl Method {
    pub fn is_static(&self) -> bool {
        self.flags & access::STATIC != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FieldRef {
    pub owner: ClassId,
    pub slot: u32,
    pub kind: u8,
    pub is_static: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MethodRef {
    pub class: ClassId,
    pub sig_id: u32,
    /// Direct target if resolvable statically (static/direct/super lookups).
    pub direct: Option<MethodIdx>,
}

pub struct DexFile {
    pub bytes: Rc<[u8]>,
    strings: Vec<Ref>,
    types: Vec<u32>,
    fields: Vec<Option<FieldRef>>,
    pub(crate) methods: Vec<Option<MethodRef>>,
    /// Monomorphic inline cache per method ref: (receiver class + 1, target).
    ic: Vec<(u32, MethodIdx)>,
}

#[derive(Debug)]
pub struct Frame {
    pub method: MethodIdx,
    pub code: Rc<Code>,
    /// Address of the instruction currently executing (call site when suspended).
    pub pc: usize,
    pub fp: usize,
}

/// App resources (resources.arsc + layouts) supplied by the host crate.
pub trait ResourceProvider {
    fn string(&self, id: u32) -> Option<String>;
    /// Layout serialized for android.view.LayoutInflater.
    fn layout(&self, id: u32) -> Option<String>;
    /// "package:type/entry"
    fn name(&self, id: u32) -> Option<String>;
    /// kind: 0 color, 1 integer, 2 bool
    fn int(&self, id: u32, kind: i32) -> Option<i32>;
    fn dimen(&self, id: u32) -> Option<f32>;
    fn identifier(&self, name: &str, ty: &str) -> Option<u32>;
    fn string_array(&self, id: u32) -> Option<Vec<String>>;
    fn style(&self, _id: u32) -> Option<Vec<(u32, String)>> {
        None
    }
    fn value(&self, _id: u32) -> Option<String> {
        None
    }
    fn xml(&self, _path: &str) -> Option<String> {
        None
    }
    fn attr_name(&self, _id: u32) -> Option<String> {
        None
    }
    fn bag_parent(&self, _id: u32) -> Option<u32> {
        None
    }
}

/// App-private storage supplied by the host (None = in-memory only).
pub trait Storage {
    fn load(&self, path: &str) -> Option<String>;
    fn save(&self, path: &str, data: &str) -> bool;
}

/// Output sink: (fd, text). fd 1 = stdout, 2 = stderr, 3 = logcat.
pub type OutputFn = Box<dyn FnMut(i32, &str)>;

pub struct Vm {
    pub heap: heap::Heap,
    pub classes: Vec<Class>,
    pub methods: Vec<Method>,
    pub dexes: Vec<DexFile>,
    class_by_name: HashMap<Rc<str>, ClassId>,
    class_index: HashMap<Rc<str>, (u16, u32)>,
    sigs: HashMap<Rc<str>, u32>,
    interned: HashMap<Box<[u16]>, Ref>,
    natives: HashMap<&'static str, NativeFn>,
    pub stack: Vec<u32>,
    pub sp: usize,
    pub frames: Vec<Frame>,
    pub exception: Ref,
    pub ret: u64,
    /// Extra GC roots held by Rust code (host API, natives calling back into Java).
    pub pins: Vec<Ref>,
    pub out: OutputFn,
    pub exit_code: Option<i32>,
    pub max_depth: usize,
    pub start_time: std::time::Instant,
    pub instructions: u64,
    pub host: Box<dyn std::any::Any>,
    pub resources: Option<Box<dyn ResourceProvider>>,
    pub storage: Option<Box<dyn Storage>>,
    well_known: WellKnown,
    lambdas: lambda::Lambdas,
    /// Last descriptor that was not found (explains linkage failures).
    missing: Option<Rc<str>>,
    /// Host font metrics (see metrics.rs); None = built-in approximation.
    pub text_measurer: Option<metrics::TextMeasurer>,
    /// Bitmap resource intrinsic sizes, supplied by the host crate.
    pub image_sizer: Option<metrics::ImageSizer>,
    /// android:theme per activity class ("" = application), from the manifest.
    pub manifest_themes: HashMap<String, u32>,
}

#[derive(Default, Clone, Copy)]
struct WellKnown {
    object: Option<ClassId>,
    string: Option<ClassId>,
    class: Option<ClassId>,
}

pub fn descriptor_to_java(desc: &str) -> String {
    if let Some(inner) = desc.strip_prefix('L').and_then(|d| d.strip_suffix(';')) {
        inner.replace('/', ".")
    } else if desc.starts_with('[') {
        desc.replace('/', ".")
    } else {
        match desc {
            "I" => "int",
            "J" => "long",
            "Z" => "boolean",
            "B" => "byte",
            "C" => "char",
            "S" => "short",
            "F" => "float",
            "D" => "double",
            "V" => "void",
            _ => desc,
        }
        .to_string()
    }
}

pub fn java_to_descriptor(name: &str) -> String {
    if name.starts_with('[') {
        name.replace('.', "/")
    } else {
        format!("L{};", name.replace('.', "/"))
    }
}

impl Vm {
    /// New VM with the bundled core library loaded.
    pub fn new() -> Vm {
        let mut vm = Vm {
            heap: heap::Heap::new(),
            classes: Vec::new(),
            methods: Vec::new(),
            dexes: Vec::new(),
            class_by_name: HashMap::new(),
            class_index: HashMap::new(),
            sigs: HashMap::new(),
            interned: HashMap::new(),
            natives: natives::registry(),
            stack: vec![0; 64 * 1024],
            sp: 0,
            frames: Vec::new(),
            exception: 0,
            ret: 0,
            pins: Vec::new(),
            out: Box::new(|fd, s| {
                use std::io::Write;
                if fd == 2 {
                    let _ = std::io::stderr().write_all(s.as_bytes());
                } else {
                    let _ = std::io::stdout().write_all(s.as_bytes());
                }
            }),
            exit_code: None,
            max_depth: 4000,
            start_time: std::time::Instant::now(),
            instructions: 0,
            host: Box::new(()),
            resources: None,
            storage: None,
            well_known: WellKnown::default(),
            lambdas: lambda::Lambdas::default(),
            missing: None,
            text_measurer: None,
            image_sizer: None,
            manifest_themes: HashMap::new(),
        };
        vm.add_dex(Rc::from(CORE_DEX))
            .expect("bundled core.dex is valid");
        vm.well_known.object = vm.find_class("Ljava/lang/Object;").ok();
        vm.well_known.string = vm.find_class("Ljava/lang/String;").ok();
        vm.well_known.class = vm.find_class("Ljava/lang/Class;").ok();
        vm
    }

    /// Adds a dex file to the class path (later files never shadow earlier ones).
    pub fn add_dex(&mut self, bytes: Rc<[u8]>) -> Result<(), String> {
        let dex = Dex::parse(&bytes).map_err(|e| e.to_string())?;
        let idx = self.dexes.len() as u16;
        for i in 0..dex.class_def_count() {
            let def = dex.class_def(i).map_err(|e| e.to_string())?;
            let name: Rc<str> = Rc::from(
                dex.type_descriptor(def.class_idx)
                    .map_err(|e| e.to_string())?
                    .as_ref(),
            );
            self.class_index.entry(name).or_insert((idx, i));
        }
        let df = DexFile {
            strings: vec![0; dex.string_count() as usize],
            types: vec![0; dex.type_count() as usize],
            fields: vec![None; dex.field_count() as usize],
            methods: vec![None; dex.method_count() as usize],
            ic: Vec::new(),
            bytes: bytes.clone(),
        };
        self.dexes.push(df);
        Ok(())
    }

    pub fn object_class(&self) -> ClassId {
        self.well_known.object.expect("java.lang.Object")
    }
    pub fn string_class(&self) -> ClassId {
        self.well_known.string.expect("java.lang.String")
    }

    #[inline]
    pub fn class(&self, c: ClassId) -> &Class {
        &self.classes[c.0 as usize]
    }
    #[inline]
    pub fn class_mut(&mut self, c: ClassId) -> &mut Class {
        &mut self.classes[c.0 as usize]
    }

    pub fn sig_id(&mut self, name_sig: &str) -> u32 {
        if let Some(&id) = self.sigs.get(name_sig) {
            return id;
        }
        let id = self.sigs.len() as u32;
        self.sigs.insert(Rc::from(name_sig), id);
        id
    }

    // ------------------------------------------------------------------
    // Class loading
    // ------------------------------------------------------------------

    /// Finds (loading and linking if needed) a class by descriptor. Throws
    /// NoClassDefFoundError if missing.
    pub fn find_class(&mut self, desc: &str) -> JResult<ClassId> {
        match self.load_class(desc) {
            Ok(c) => Ok(c),
            Err(LoadErr::Thrown) => Err(Throw),
            Err(LoadErr::NotFound) => {
                let msg = match self.missing.take() {
                    Some(m) if &*m != desc => {
                        format!("Failed resolution of: {desc} (needs missing {m})")
                    }
                    _ => format!("Failed resolution of: {desc}"),
                };
                self.throw_new("Ljava/lang/NoClassDefFoundError;", &msg)
            }
        }
    }

    pub fn try_find_class(&mut self, desc: &str) -> Option<ClassId> {
        self.load_class(desc).ok()
    }

    fn load_class(&mut self, desc: &str) -> Result<ClassId, LoadErr> {
        if let Some(&c) = self.class_by_name.get(desc) {
            return Ok(c);
        }
        if let Some(comp) = desc.strip_prefix('[') {
            let component = self.load_class(comp)?;
            let obj = self.object_class();
            let mut ifaces = Vec::new();
            for i in ["Ljava/lang/Cloneable;", "Ljava/io/Serializable;"] {
                if let Ok(c) = self.load_class(i) {
                    ifaces.push(c);
                }
            }
            return Ok(self.register_class(Class {
                name: Rc::from(desc),
                super_: Some(obj),
                interfaces: ifaces,
                flags: access::PUBLIC | access::FINAL | access::ABSTRACT,
                src: ClassSrc::Array { component },
                state: ClassState::Initialized,
                ifields: Vec::new(),
                sfields: Vec::new(),
                statics: Vec::new(),
                nslots: 0,
                ref_slots: Box::new([]),
                methods: Vec::new(),
                vcache: HashMap::new(),
                mirror: 0,
                source_file: None,
            }));
        }
        if desc.len() == 1 && "IJZBCSFDV".contains(desc) {
            return Ok(self.register_class(Class {
                name: Rc::from(desc),
                super_: None,
                interfaces: Vec::new(),
                flags: access::PUBLIC | access::FINAL | access::ABSTRACT,
                src: ClassSrc::Primitive(desc.as_bytes()[0]),
                state: ClassState::Initialized,
                ifields: Vec::new(),
                sfields: Vec::new(),
                statics: Vec::new(),
                nslots: 0,
                ref_slots: Box::new([]),
                methods: Vec::new(),
                vcache: HashMap::new(),
                mirror: 0,
                source_file: None,
            }));
        }
        let Some(&(dex_idx, def_idx)) = self.class_index.get(desc) else {
            self.missing = Some(Rc::from(desc));
            return Err(LoadErr::NotFound);
        };
        self.link_dex_class(dex_idx, def_idx)
    }

    fn register_class(&mut self, c: Class) -> ClassId {
        let id = ClassId(self.classes.len() as u32);
        self.class_by_name.insert(c.name.clone(), id);
        self.classes.push(c);
        id
    }

    fn link_dex_class(&mut self, dex_idx: u16, def_idx: u32) -> Result<ClassId, LoadErr> {
        let bytes = self.dexes[dex_idx as usize].bytes.clone();
        let dex = Dex::parse(&bytes).map_err(|_| LoadErr::NotFound)?;
        let bad = |_| LoadErr::NotFound;
        let def = dex.class_def(def_idx).map_err(bad)?;
        let name: Rc<str> = Rc::from(dex.type_descriptor(def.class_idx).map_err(bad)?.as_ref());

        let super_ = if def.superclass_idx == NO_INDEX {
            None
        } else {
            let sd = dex.type_descriptor(def.superclass_idx).map_err(bad)?;
            Some(self.load_class(&sd)?)
        };
        let mut interfaces = Vec::new();
        for t in dex.type_list(def.interfaces_off).map_err(bad)? {
            let d = dex.type_descriptor(t as u32).map_err(bad)?;
            interfaces.push(self.load_class(&d)?);
        }
        // Another load may have registered us recursively (shouldn't for valid code).
        if let Some(&c) = self.class_by_name.get(&name) {
            return Ok(c);
        }
        let cd = dex.class_data(def.class_data_off).map_err(bad)?;

        let (mut nslots, mut ref_slots) = match super_ {
            Some(s) => (self.class(s).nslots, self.class(s).ref_slots.to_vec()),
            None => (0, Vec::new()),
        };
        let mut ifields = Vec::with_capacity(cd.instance_fields.len());
        for f in &cd.instance_fields {
            let fid = dex.field_id(f.field_idx).map_err(bad)?;
            let desc: Rc<str> = Rc::from(
                dex.type_descriptor(fid.type_idx as u32)
                    .map_err(bad)?
                    .as_ref(),
            );
            let fname: Rc<str> = Rc::from(dex.string(fid.name_idx).map_err(bad)?.as_ref());
            let wide = matches!(desc.as_bytes()[0], b'J' | b'D');
            if matches!(desc.as_bytes()[0], b'L' | b'[') {
                ref_slots.push(nslots);
            }
            ifields.push(FieldDef {
                name: fname,
                desc,
                slot: nslots,
                flags: f.access_flags,
            });
            nslots += if wide { 2 } else { 1 };
        }
        let mut sfields = Vec::with_capacity(cd.static_fields.len());
        let mut sslots = 0u32;
        for f in &cd.static_fields {
            let fid = dex.field_id(f.field_idx).map_err(bad)?;
            let desc: Rc<str> = Rc::from(
                dex.type_descriptor(fid.type_idx as u32)
                    .map_err(bad)?
                    .as_ref(),
            );
            let fname: Rc<str> = Rc::from(dex.string(fid.name_idx).map_err(bad)?.as_ref());
            let wide = matches!(desc.as_bytes()[0], b'J' | b'D');
            sfields.push(FieldDef {
                name: fname,
                desc,
                slot: sslots,
                flags: f.access_flags,
            });
            sslots += if wide { 2 } else { 1 };
        }

        let id = ClassId(self.classes.len() as u32);
        let mut methods = Vec::new();
        for m in cd.direct_methods.iter().chain(cd.virtual_methods.iter()) {
            let mid = dex.method_id(m.method_idx).map_err(bad)?;
            let mname: Rc<str> = Rc::from(dex.string(mid.name_idx).map_err(bad)?.as_ref());
            let sig: Rc<str> = Rc::from(dex.proto_signature(mid.proto_idx as u32).map_err(bad)?);
            let sig_id = self.sig_id(&format!("{mname}{sig}"));
            let is_static = m.access_flags & access::STATIC != 0;
            let mut arg_slots: u16 = if is_static { 0 } else { 1 };
            for t in dex.proto_params(mid.proto_idx as u32).map_err(bad)? {
                let d = dex.type_descriptor(t as u32).map_err(bad)?;
                arg_slots += indroid_dex::descriptor_slots(&d) as u16;
            }
            let ret = sig.as_bytes()[sig.find(')').unwrap() + 1];
            let kind = if m.access_flags & access::NATIVE != 0 {
                let key = format!("{name}->{mname}{sig}");
                match self.natives.get(key.as_str()) {
                    Some(f) => MKind::Native(*f),
                    None => MKind::Missing,
                }
            } else if m.code_off == 0 {
                MKind::Abstract
            } else {
                MKind::Missing // loaded lazily, see method_code()
            };
            let midx = self.methods.len() as MethodIdx;
            self.methods.push(Method {
                class: id,
                name: mname,
                sig,
                sig_id,
                flags: m.access_flags,
                kind,
                arg_slots,
                ret,
                code_off: (dex_idx, m.code_off),
            });
            methods.push(midx);
        }
        let source_file = if def.source_file_idx != NO_INDEX {
            dex.string(def.source_file_idx)
                .ok()
                .map(|s| Rc::from(s.as_ref()))
        } else {
            None
        };
        self.register_class(Class {
            name,
            super_,
            interfaces,
            flags: def.access_flags,
            src: ClassSrc::Dex {
                dex: dex_idx,
                def: def_idx,
            },
            state: ClassState::Loaded,
            ifields,
            sfields,
            statics: vec![0; sslots as usize],
            nslots,
            ref_slots: ref_slots.into_boxed_slice(),
            methods,
            vcache: HashMap::new(),
            mirror: 0,
            source_file,
        });
        Ok(id)
    }

    /// Lazily decodes a method's bytecode.
    pub fn method_code(&mut self, m: MethodIdx) -> Option<Rc<Code>> {
        let meth = &self.methods[m as usize];
        if let MKind::Code(c) = &meth.kind {
            return Some(c.clone());
        }
        if !matches!(meth.kind, MKind::Missing) || meth.flags & access::NATIVE != 0 {
            return None;
        }
        let (d, off) = meth.code_off;
        if off == 0 {
            return None;
        }
        let bytes = self.dexes[d as usize].bytes.clone();
        let dex = Dex::parse(&bytes).ok()?;
        let ci = dex.code_item(off).ok()?;
        let mut tries = Vec::with_capacity(ci.tries_size as usize);
        for t in dex.tries(&ci).ok()? {
            let h = dex.catch_handler(&ci, t.handler_off).ok()?;
            let mut hs: Vec<(u32, u32)> = h.typed.clone();
            if let Some(a) = h.catch_all {
                hs.push((NO_INDEX, a));
            }
            tries.push(TryEntry {
                start: t.start_addr,
                end: t.start_addr + t.insn_count as u32,
                handlers: hs.into_boxed_slice(),
            });
        }
        let insns = ci.insns();
        if let Err(e) = verify::verify(&insns, ci.registers_size) {
            self.methods[m as usize].kind = MKind::Invalid(Rc::from(e));
            return None;
        }
        let code = Rc::new(Code {
            regs: ci.registers_size,
            ins: ci.ins_size,
            insns: insns.into_boxed_slice(),
            tries: tries.into_boxed_slice(),
            dex: d,
            debug_info_off: ci.debug_info_off,
        });
        self.methods[m as usize].kind = MKind::Code(code.clone());
        Some(code)
    }

    // ------------------------------------------------------------------
    // Initialization
    // ------------------------------------------------------------------

    pub fn ensure_init(&mut self, c: ClassId) -> JResult<()> {
        match self.class(c).state {
            ClassState::Initialized | ClassState::Initializing => return Ok(()),
            ClassState::Failed => {
                let msg = format!("Could not initialize class {}", self.class(c).java_name());
                return self.throw_new("Ljava/lang/NoClassDefFoundError;", &msg);
            }
            ClassState::Loaded => {}
        }
        self.class_mut(c).state = ClassState::Initializing;
        if let Some(s) = self.class(c).super_ {
            if let Err(t) = self.ensure_init(s) {
                self.class_mut(c).state = ClassState::Failed;
                return Err(t);
            }
        }
        if let ClassSrc::Dex { dex, def } = self.class(c).src {
            let bytes = self.dexes[dex as usize].bytes.clone();
            let d = Dex::parse(&bytes).expect("validated");
            if let Ok(cd) = d.class_def(def) {
                if let Ok(vals) = d.encoded_array(cd.static_values_off) {
                    for (i, v) in vals.into_iter().enumerate() {
                        let Some(f) = self.class(c).sfields.get(i).cloned() else {
                            break;
                        };
                        let slot = f.slot as usize;
                        let (lo, hi) = match v {
                            EncodedValue::Byte(x) => (x as i32 as u32, None),
                            EncodedValue::Short(x) => (x as i32 as u32, None),
                            EncodedValue::Char(x) => (x as u32, None),
                            EncodedValue::Int(x) => (x as u32, None),
                            EncodedValue::Boolean(b) => (b as u32, None),
                            EncodedValue::Float(x) => (x.to_bits(), None),
                            EncodedValue::Long(x) => (x as u32, Some((x >> 32) as u32)),
                            EncodedValue::Double(x) => {
                                let b = x.to_bits();
                                (b as u32, Some((b >> 32) as u32))
                            }
                            EncodedValue::String(si) => (self.const_string(dex, si)?, None),
                            EncodedValue::Type(ti) => {
                                let cl = self.resolve_type(dex, ti)?;
                                (self.mirror(cl), None)
                            }
                            _ => (0, None),
                        };
                        let st = &mut self.class_mut(c).statics;
                        st[slot] = lo;
                        if let Some(h) = hi {
                            st[slot + 1] = h;
                        }
                    }
                }
            }
        }
        let clinit = self.find_declared(c, "<clinit>()V");
        if let Some(m) = clinit {
            if self.invoke(m, &[]).is_err() {
                self.class_mut(c).state = ClassState::Failed;
                let cause = self.exception;
                let is_error = self
                    .heap
                    .get(cause)
                    .map(|o| o.class)
                    .and_then(|cl| {
                        self.try_find_class("Ljava/lang/Error;")
                            .map(|e| self.is_subclass(cl, e))
                    })
                    .unwrap_or(false);
                if is_error {
                    return Err(Throw);
                }
                self.pins.push(cause);
                let r =
                    self.new_throwable_with_cause("Ljava/lang/ExceptionInInitializerError;", cause);
                self.pins.pop();
                let e = r?;
                self.exception = e;
                return Err(Throw);
            }
        }
        self.class_mut(c).state = ClassState::Initialized;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Method / field lookup
    // ------------------------------------------------------------------

    pub fn find_declared(&mut self, c: ClassId, name_sig: &str) -> Option<MethodIdx> {
        let id = self.sig_id(name_sig);
        self.class(c)
            .methods
            .iter()
            .copied()
            .find(|&m| self.methods[m as usize].sig_id == id)
    }

    fn find_declared_id(&self, c: ClassId, sig_id: u32) -> Option<MethodIdx> {
        self.class(c)
            .methods
            .iter()
            .copied()
            .find(|&m| self.methods[m as usize].sig_id == sig_id)
    }

    /// Lookup for static/direct invocation: class, then superclasses, then interfaces.
    pub fn find_method_in_hierarchy(&self, c: ClassId, sig_id: u32) -> Option<MethodIdx> {
        let mut cur = Some(c);
        while let Some(k) = cur {
            if let Some(m) = self.find_declared_id(k, sig_id) {
                return Some(m);
            }
            cur = self.class(k).super_;
        }
        self.find_in_interfaces(c, sig_id, true)
    }

    fn find_in_interfaces(
        &self,
        c: ClassId,
        sig_id: u32,
        allow_abstract: bool,
    ) -> Option<MethodIdx> {
        let mut seen = Vec::new();
        let mut work = vec![c];
        let mut abstract_hit = None;
        while let Some(k) = work.pop() {
            let mut cur = Some(k);
            while let Some(x) = cur {
                for &i in &self.class(x).interfaces {
                    if seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    if let Some(m) = self.find_declared_id(i, sig_id) {
                        if !matches!(self.methods[m as usize].kind, MKind::Abstract) {
                            return Some(m);
                        }
                        abstract_hit.get_or_insert(m);
                    }
                    work.push(i);
                }
                cur = self.class(x).super_;
            }
        }
        if allow_abstract {
            abstract_hit
        } else {
            None
        }
    }

    /// Virtual dispatch on the receiver's class.
    pub fn find_virtual(&mut self, c: ClassId, sig_id: u32) -> Option<MethodIdx> {
        if let Some(&r) = self.class(c).vcache.get(&sig_id) {
            return r;
        }
        let mut found = None;
        let mut cur = Some(c);
        while let Some(k) = cur {
            if let Some(m) = self.find_declared_id(k, sig_id) {
                let meth = &self.methods[m as usize];
                if !meth.is_static() && &*meth.name != "<init>" {
                    found = Some(m);
                    break;
                }
            }
            cur = self.class(k).super_;
        }
        let abstract_found =
            found.filter(|&m| matches!(self.methods[m as usize].kind, MKind::Abstract));
        if found.is_none() || abstract_found.is_some() {
            if let Some(m) = self.find_in_interfaces(c, sig_id, false) {
                found = Some(m);
            } else if found.is_none() {
                found = self.find_in_interfaces(c, sig_id, true);
            }
        }
        self.class_mut(c).vcache.insert(sig_id, found);
        found
    }

    fn find_field(&self, c: ClassId, name: &str, is_static: bool) -> Option<(ClassId, FieldDef)> {
        let mut cur = Some(c);
        while let Some(k) = cur {
            let cls = self.class(k);
            let list = if is_static {
                &cls.sfields
            } else {
                &cls.ifields
            };
            if let Some(f) = list.iter().find(|f| &*f.name == name) {
                return Some((k, f.clone()));
            }
            if is_static {
                // static constants on interfaces
                let mut work: Vec<ClassId> = cls.interfaces.clone();
                while let Some(i) = work.pop() {
                    if let Some(f) = self.class(i).sfields.iter().find(|f| &*f.name == name) {
                        return Some((i, f.clone()));
                    }
                    work.extend(self.class(i).interfaces.iter().copied());
                }
            }
            cur = cls.super_;
        }
        None
    }

    pub fn is_subclass(&self, sub: ClassId, sup: ClassId) -> bool {
        if sub == sup {
            return true;
        }
        if Some(sup) == self.well_known.object {
            return !matches!(self.class(sub).src, ClassSrc::Primitive(_));
        }
        let sc = self.class(sup);
        if let (ClassSrc::Array { component: a }, ClassSrc::Array { component: b }) =
            (&self.class(sub).src, &sc.src)
        {
            let (a, b) = (*a, *b);
            if matches!(self.class(a).src, ClassSrc::Primitive(_))
                || matches!(self.class(b).src, ClassSrc::Primitive(_))
            {
                return a == b;
            }
            return self.is_subclass(a, b);
        }
        if sc.is_interface() {
            let mut cur = Some(sub);
            while let Some(k) = cur {
                for &i in &self.class(k).interfaces {
                    if i == sup || self.is_subclass(i, sup) {
                        return true;
                    }
                }
                cur = self.class(k).super_;
            }
            return false;
        }
        let mut cur = self.class(sub).super_;
        while let Some(k) = cur {
            if k == sup {
                return true;
            }
            cur = self.class(k).super_;
        }
        false
    }

    pub fn instance_of(&self, r: Ref, c: ClassId) -> bool {
        match self.heap.get(r) {
            Some(o) => self.is_subclass(o.class, c),
            None => false,
        }
    }

    // ------------------------------------------------------------------
    // Resolution caches (per dex)
    // ------------------------------------------------------------------

    pub fn resolve_type(&mut self, dex: u16, type_idx: u32) -> JResult<ClassId> {
        let cached = self.dexes[dex as usize].types[type_idx as usize];
        if cached != 0 {
            return Ok(ClassId(cached - 1));
        }
        let bytes = self.dexes[dex as usize].bytes.clone();
        let d = Dex::parse(&bytes).expect("validated");
        let desc = d.type_descriptor(type_idx).unwrap_or_default().into_owned();
        let c = self.find_class(&desc)?;
        self.dexes[dex as usize].types[type_idx as usize] = c.0 + 1;
        Ok(c)
    }

    pub fn const_string(&mut self, dex: u16, string_idx: u32) -> JResult<Ref> {
        let cached = self.dexes[dex as usize].strings[string_idx as usize];
        if cached != 0 {
            return Ok(cached);
        }
        let bytes = self.dexes[dex as usize].bytes.clone();
        let d = Dex::parse(&bytes).expect("validated");
        let units = d.string_utf16(string_idx).unwrap_or_default();
        let r = self.intern_utf16(&units);
        self.dexes[dex as usize].strings[string_idx as usize] = r;
        Ok(r)
    }

    pub fn intern_utf16(&mut self, units: &[u16]) -> Ref {
        if let Some(&r) = self.interned.get(units) {
            return r;
        }
        let r = self.alloc_string(JStr::from_utf16(units));
        self.interned.insert(units.into(), r);
        r
    }

    pub fn resolve_field(
        &mut self,
        dex: u16,
        field_idx: u32,
        is_static: bool,
    ) -> JResult<FieldRef> {
        if let Some(f) = self.dexes[dex as usize].fields[field_idx as usize] {
            return Ok(f);
        }
        let bytes = self.dexes[dex as usize].bytes.clone();
        let d = Dex::parse(&bytes).expect("validated");
        let fid = d.field_id(field_idx).expect("validated");
        let c = self.resolve_type(dex, fid.class_idx as u32)?;
        let name = d.string(fid.name_idx).unwrap_or_default();
        match self.find_field(c, &name, is_static) {
            Some((owner, f)) => {
                let fr = FieldRef {
                    owner,
                    slot: f.slot,
                    kind: f.desc.as_bytes()[0],
                    is_static,
                };
                self.dexes[dex as usize].fields[field_idx as usize] = Some(fr);
                Ok(fr)
            }
            None => {
                let msg = format!("No field {} in class {}", name, self.class(c).name);
                self.throw_new("Ljava/lang/NoSuchFieldError;", &msg)
            }
        }
    }

    pub fn resolve_method_ref(&mut self, dex: u16, method_idx: u32) -> JResult<MethodRef> {
        if let Some(m) = self.dexes[dex as usize].methods[method_idx as usize] {
            return Ok(m);
        }
        let bytes = self.dexes[dex as usize].bytes.clone();
        let d = Dex::parse(&bytes).expect("validated");
        let mid = d.method_id(method_idx).expect("validated");
        let c = self.resolve_type(dex, mid.class_idx as u32)?;
        let name = d.string(mid.name_idx).unwrap_or_default();
        let sig = d.proto_signature(mid.proto_idx as u32).unwrap_or_default();
        let sig_id = self.sig_id(&format!("{name}{sig}"));
        let direct = self.find_method_in_hierarchy(c, sig_id);
        let mr = MethodRef {
            class: c,
            sig_id,
            direct,
        };
        self.dexes[dex as usize].methods[method_idx as usize] = Some(mr);
        Ok(mr)
    }

    pub fn method_ref_name(&self, sig_id: u32) -> String {
        self.sigs
            .iter()
            .find(|(_, &v)| v == sig_id)
            .map(|(k, _)| k.to_string())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // Allocation
    // ------------------------------------------------------------------

    pub fn alloc_string(&mut self, s: JStr) -> Ref {
        let c = self.string_class();
        self.heap.alloc(Obj {
            class: c,
            data: ObjData::Str(s),
        })
    }

    pub fn new_string(&mut self, s: &str) -> Ref {
        self.alloc_string(JStr::from_rust(s))
    }

    pub fn new_string_utf16(&mut self, s: &[u16]) -> Ref {
        self.alloc_string(JStr::from_utf16(s))
    }

    pub fn alloc_instance(&mut self, c: ClassId) -> Ref {
        if Some(c) == self.well_known.string {
            return self.alloc_string(JStr::from_utf16(&[]));
        }
        let n = self.class(c).nslots as usize;
        self.heap.alloc(Obj {
            class: c,
            data: ObjData::Inst(vec![0; n].into_boxed_slice()),
        })
    }

    /// Allocates an array of the given array class.
    pub fn alloc_array(&mut self, array_class: ClassId, len: usize) -> Ref {
        let elem = match self.class(array_class).src {
            ClassSrc::Array { component } => match self.class(component).src {
                ClassSrc::Primitive(p) => p,
                _ => b'L',
            },
            _ => b'L',
        };
        let data = match elem {
            b'Z' | b'B' => ObjData::ArrB(vec![0; len].into_boxed_slice()),
            b'C' => ObjData::ArrC(vec![0; len].into_boxed_slice()),
            b'S' => ObjData::ArrS(vec![0; len].into_boxed_slice()),
            b'I' | b'F' => ObjData::ArrI(vec![0; len].into_boxed_slice()),
            b'J' | b'D' => ObjData::ArrJ(vec![0; len].into_boxed_slice()),
            _ => ObjData::ArrL(vec![0; len].into_boxed_slice()),
        };
        self.heap.alloc(Obj {
            class: array_class,
            data,
        })
    }

    pub fn new_array(&mut self, desc: &str, len: usize) -> JResult<Ref> {
        let c = self.find_class(desc)?;
        Ok(self.alloc_array(c, len))
    }

    pub fn mirror(&mut self, c: ClassId) -> Ref {
        let m = self.class(c).mirror;
        if m != 0 {
            return m;
        }
        let cc = self.well_known.class.expect("java.lang.Class");
        let r = self.heap.alloc(Obj {
            class: cc,
            data: ObjData::Mirror(c),
        });
        self.class_mut(c).mirror = r;
        r
    }

    // ------------------------------------------------------------------
    // Strings
    // ------------------------------------------------------------------

    pub fn jstr(&self, r: Ref) -> Option<&JStr> {
        match self.heap.get(r).map(|o| &o.data) {
            Some(ObjData::Str(s)) => Some(s),
            _ => None,
        }
    }

    pub fn rust_string(&self, r: Ref) -> Option<String> {
        self.jstr(r).map(|s| s.to_rust())
    }

    // ------------------------------------------------------------------
    // Fields (by name, for Rust-side code)
    // ------------------------------------------------------------------

    pub fn get_field(&self, obj: Ref, name: &str) -> Option<u32> {
        let o = self.heap.get(obj)?;
        let (_, f) = self.find_field(o.class, name, false)?;
        match &o.data {
            ObjData::Inst(fs) => fs.get(f.slot as usize).copied(),
            _ => None,
        }
    }

    pub fn set_field(&mut self, obj: Ref, name: &str, v: u32) -> bool {
        let Some(class) = self.heap.get(obj).map(|o| o.class) else {
            return false;
        };
        let Some((_, f)) = self.find_field(class, name, false) else {
            return false;
        };
        if let Some(Obj {
            data: ObjData::Inst(fs),
            ..
        }) = self.heap.get_mut(obj)
        {
            fs[f.slot as usize] = v;
            return true;
        }
        false
    }

    // ------------------------------------------------------------------
    // Exceptions
    // ------------------------------------------------------------------

    /// Creates `class(msg)` and makes it the pending exception.
    pub fn throw_new<T>(&mut self, class: &str, msg: &str) -> JResult<T> {
        let e = self.new_throwable(class, Some(msg))?;
        self.exception = e;
        Err(Throw)
    }

    pub fn new_throwable(&mut self, class: &str, msg: Option<&str>) -> JResult<Ref> {
        // Building the throwable runs Java constructors; give them headroom when
        // the error being reported is the stack depth limit itself.
        let saved = self.max_depth;
        if self.frames.len() + 64 > self.max_depth {
            self.max_depth = self.frames.len() + 64;
        }
        let r = self.new_throwable_inner(class, msg);
        self.max_depth = saved;
        r
    }

    fn new_throwable_inner(&mut self, class: &str, msg: Option<&str>) -> JResult<Ref> {
        let c = match self.load_class(class) {
            Ok(c) => c,
            Err(_) => panic!("core class missing: {class}"),
        };
        self.ensure_init(c)?;
        let obj = self.alloc_instance(c);
        self.pins.push(obj);
        let r = match msg {
            Some(m) => {
                let s = self.new_string(m);
                self.pins.push(s);
                let ctor = self.find_declared(c, "<init>(Ljava/lang/String;)V");
                let r = match ctor {
                    Some(ctor) => self.invoke(ctor, &[obj, s]).map(|_| obj),
                    None => Ok(obj),
                };
                self.pins.pop();
                r
            }
            None => {
                let ctor = self.find_declared(c, "<init>()V");
                match ctor {
                    Some(ctor) => self.invoke(ctor, &[obj]).map(|_| obj),
                    None => Ok(obj),
                }
            }
        };
        self.pins.pop();
        r
    }

    fn new_throwable_with_cause(&mut self, class: &str, cause: Ref) -> JResult<Ref> {
        let c = self.find_class(class)?;
        self.ensure_init(c)?;
        let obj = self.alloc_instance(c);
        self.pins.push(obj);
        let ctor = self.find_declared(c, "<init>(Ljava/lang/Throwable;)V");
        let r = match ctor {
            Some(ctor) => self.invoke(ctor, &[obj, cause]).map(|_| obj),
            None => Ok(obj),
        };
        self.pins.pop();
        r
    }

    /// Takes the pending exception and renders "Class: message".
    pub fn describe_exception(&mut self) -> String {
        let e = std::mem::replace(&mut self.exception, 0);
        if e == 0 {
            return "unknown error".into();
        }
        self.pins.push(e);
        let s = self.call_to_string(e).unwrap_or_else(|_| {
            self.exception = 0;
            self.heap
                .get(e)
                .map(|o| self.class(o.class).java_name())
                .unwrap_or_default()
        });
        let mut out = s;
        // Append stack frames recorded by fillInStackTrace.
        if let Some(trace) = self.get_field(e, "vmTrace") {
            if let Some(ObjData::ArrL(items)) = self.heap.get(trace).map(|o| &o.data) {
                for &it in items.iter().take(12) {
                    if let Some(f) = self.rust_string(it) {
                        let p: Vec<&str> = f.split('\0').collect();
                        out.push_str(&format!("\n\tat {}.{}", p[0], p.get(1).unwrap_or(&"?")));
                    }
                }
            }
        }
        self.pins.pop();
        out
    }

    pub fn call_to_string(&mut self, obj: Ref) -> JResult<String> {
        if obj == 0 {
            return Ok("null".into());
        }
        if let Some(s) = self.rust_string(obj) {
            return Ok(s);
        }
        let c = self.heap.get(obj).unwrap().class;
        let id = self.sig_id("toString()Ljava/lang/String;");
        let m = self.find_virtual(c, id).expect("Object.toString");
        let r = self.invoke(m, &[obj])? as u32;
        Ok(self.rust_string(r).unwrap_or_else(|| "null".into()))
    }

    // ------------------------------------------------------------------
    // GC
    // ------------------------------------------------------------------

    pub fn gc(&mut self) {
        let mut roots: Vec<u32> = Vec::with_capacity(1024);
        // Conservative: every register slot of every live frame.
        roots.extend_from_slice(&self.stack[..self.sp]);
        roots.extend_from_slice(&self.pins);
        roots.push(self.exception);
        roots.push(self.ret as u32);
        roots.extend(self.interned.values().copied());
        roots.extend(self.lambdas.roots());
        for c in &self.classes {
            roots.push(c.mirror);
            for f in &c.sfields {
                if matches!(f.desc.as_bytes()[0], b'L' | b'[') {
                    roots.push(c.statics[f.slot as usize]);
                }
            }
        }
        for d in &self.dexes {
            roots.extend(d.strings.iter().copied().filter(|&r| r != 0));
        }
        let classes = &self.classes;
        self.heap.collect(&mut roots.into_iter(), |c| {
            &*classes[c.0 as usize].ref_slots as *const [u32]
        });
    }

    #[inline]
    pub fn maybe_gc(&mut self) {
        if self.heap.wants_gc() {
            self.gc();
        }
    }

    // ------------------------------------------------------------------
    // Entry points
    // ------------------------------------------------------------------

    /// Calls a static method by class descriptor and "name(sig)ret".
    pub fn call_static(&mut self, class: &str, name_sig: &str, args: &[u32]) -> JResult<u64> {
        let c = self.find_class(class)?;
        self.ensure_init(c)?;
        let Some(m) = self.find_declared(c, name_sig) else {
            let msg = format!("{class}->{name_sig}");
            return self.throw_new("Ljava/lang/NoSuchMethodError;", &msg);
        };
        self.invoke(m, args)
    }

    /// Runs `public static void main(String[])` of a class.
    pub fn run_main(&mut self, class: &str) -> Result<(), String> {
        let desc = if class.starts_with('L') && class.ends_with(';') {
            class.to_string()
        } else {
            java_to_descriptor(class)
        };
        let r = (|| -> JResult<()> {
            let arr = self.new_array("[Ljava/lang/String;", 0)?;
            self.pins.push(arr);
            let r = self.call_static(&desc, "main([Ljava/lang/String;)V", &[arr]);
            self.pins.pop();
            r?;
            self.call_static("Ljava/lang/VMThread;", "runPending()V", &[])?;
            Ok(())
        })();
        match r {
            Ok(()) => Ok(()),
            Err(Throw) if self.exit_code.is_some() => Ok(()),
            Err(Throw) => Err(self.describe_exception()),
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

enum LoadErr {
    NotFound,
    #[allow(dead_code)]
    Thrown,
}

impl Vm {
    /// Lists framework classes, methods and fields referenced by the dex at
    /// `dex` that the bundled class library does not provide. Used by
    /// `indroid missing` to plan compatibility work for an app.
    pub fn missing_report(&mut self, dex: u16) -> Vec<String> {
        let bytes = self.dexes[dex as usize].bytes.clone();
        let Ok(d) = Dex::parse(&bytes) else {
            return vec!["unparsable dex".into()];
        };
        let mut out = Vec::new();
        let mut bad_class = std::collections::HashSet::new();
        for t in 0..d.type_count() {
            let Ok(desc) = d.type_descriptor(t) else {
                continue;
            };
            let base = desc.trim_start_matches('[');
            if !base.starts_with('L') {
                continue;
            }
            self.missing = None;
            if self.load_class(base).is_err() {
                bad_class.insert(base.to_string());
                match self.missing.take() {
                    Some(m) if &*m != base => out.push(format!("class {base} (needs {m})")),
                    _ => out.push(format!("class {base}")),
                }
            }
        }
        for m in 0..d.method_count() {
            let Ok(mid) = d.method_id(m) else { continue };
            let Ok(cdesc) = d.type_descriptor(mid.class_idx as u32) else {
                continue;
            };
            if cdesc.starts_with('[') || bad_class.contains(cdesc.as_ref()) {
                continue;
            }
            let Ok(c) = self.load_class(&cdesc) else {
                continue;
            };
            let name = d.string(mid.name_idx).unwrap_or_default();
            let sig = d.proto_signature(mid.proto_idx as u32).unwrap_or_default();
            let sid = self.sig_id(&format!("{name}{sig}"));
            if self.find_method_in_hierarchy(c, sid).is_none()
                && self.find_virtual(c, sid).is_none()
            {
                out.push(format!("method {cdesc}->{name}{sig}"));
            }
        }
        for f in 0..d.field_count() {
            let Ok(fid) = d.field_id(f) else { continue };
            let Ok(cdesc) = d.type_descriptor(fid.class_idx as u32) else {
                continue;
            };
            if bad_class.contains(cdesc.as_ref()) {
                continue;
            }
            let Ok(c) = self.load_class(&cdesc) else {
                continue;
            };
            let name = d.string(fid.name_idx).unwrap_or_default();
            if self.find_field(c, &name, true).is_none()
                && self.find_field(c, &name, false).is_none()
            {
                let ty = d.type_descriptor(fid.type_idx as u32).unwrap_or_default();
                out.push(format!("field {cdesc}->{name}:{ty}"));
            }
        }
        out
    }
}
