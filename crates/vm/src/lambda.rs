//! `invoke-custom` support for `LambdaMetafactory` call sites (Java 8 lambdas
//! and method references in apps built with `minSdk >= 26`, where d8 stops
//! desugaring them).
//!
//! Each call site gets a synthetic class implementing the functional
//! interface. Captured values live in its instance fields; the interface
//! method is an `MKind::Lambda` that adapts arguments (boxing, unboxing,
//! primitive widening) and calls the implementation method handle.

use std::collections::HashMap;
use std::rc::Rc;

use indroid_dex::{access, Dex, EncodedValue};

use crate::heap::{ObjData, Ref};
use crate::{Class, ClassId, ClassSrc, ClassState, FieldDef, JResult, MKind, Method, Vm};

/// Method handle kinds (dex `method_handle_item.type`).
const MH_INVOKE_STATIC: u16 = 4;
const MH_INVOKE_INSTANCE: u16 = 5;
const MH_INVOKE_CONSTRUCTOR: u16 = 6;
const MH_INVOKE_DIRECT: u16 = 7;
const MH_INVOKE_INTERFACE: u16 = 8;

const FLAG_MARKERS: i32 = 2;
const FLAG_BRIDGES: i32 = 4;

pub struct Lambda {
    handle: u16,
    /// Implementation method: declaring class + name/signature id.
    target_class: ClassId,
    target_sig: u32,
    /// Descriptor kind (first char) and field slot of each captured value.
    captured: Box<[(u8, u32)]>,
    /// Incoming parameter kinds of this interface method (after `this`).
    in_params: Box<[u8]>,
    in_ret: u8,
    /// Implementation parameter kinds, receiver included for instance handles.
    impl_params: Box<[u8]>,
    impl_ret: u8,
}

/// Cached result of linking one call site.
#[derive(Clone, Copy)]
pub struct CallSite {
    class: ClassId,
    /// Shared instance for non-capturing lambdas.
    singleton: Ref,
}

#[derive(Default)]
pub struct Lambdas {
    sites: HashMap<(u16, u32), CallSite>,
    count: u32,
}

impl Lambdas {
    /// Singleton instances that must stay alive.
    pub fn roots(&self) -> impl Iterator<Item = Ref> + '_ {
        self.sites.values().map(|s| s.singleton).filter(|&r| r != 0)
    }
}

fn kinds(sig_params: &str) -> Vec<u8> {
    let b = sig_params.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        out.push(c);
        while b[i] == b'[' {
            i += 1;
        }
        if b[i] == b'L' {
            while b[i] != b';' {
                i += 1;
            }
        }
        i += 1;
    }
    out
}

/// Splits "(params)ret" into parameter kinds and the return descriptor.
fn split_sig(sig: &str) -> (Vec<u8>, String) {
    let close = sig.find(')').unwrap_or(sig.len() - 1);
    (kinds(&sig[1..close]), sig[close + 1..].to_string())
}

fn is_wide(k: u8) -> bool {
    k == b'J' || k == b'D'
}

fn is_ref(k: u8) -> bool {
    k == b'L' || k == b'['
}

fn box_class(k: u8) -> &'static str {
    match k {
        b'Z' => "Ljava/lang/Boolean;",
        b'B' => "Ljava/lang/Byte;",
        b'S' => "Ljava/lang/Short;",
        b'C' => "Ljava/lang/Character;",
        b'I' => "Ljava/lang/Integer;",
        b'J' => "Ljava/lang/Long;",
        b'F' => "Ljava/lang/Float;",
        _ => "Ljava/lang/Double;",
    }
}

impl Vm {
    /// Links (once) and executes an `invoke-custom` call site; returns the
    /// functional-interface instance.
    pub(crate) fn invoke_custom(&mut self, dex: u16, site: u32, args: &[u32]) -> JResult<u64> {
        let cs = match self.lambdas.sites.get(&(dex, site)) {
            Some(&cs) => cs,
            None => {
                let cs = self.link_call_site(dex, site)?;
                self.lambdas.sites.insert((dex, site), cs);
                cs
            }
        };
        if cs.singleton != 0 {
            return Ok(cs.singleton as u64);
        }
        let obj = self.alloc_instance(cs.class);
        if let Some(ObjData::Inst(f)) = self.heap.get_mut(obj).map(|o| &mut o.data) {
            let n = args.len().min(f.len());
            f[..n].copy_from_slice(&args[..n]);
        }
        Ok(obj as u64)
    }

    fn link_call_site(&mut self, dex: u16, site: u32) -> JResult<CallSite> {
        let bytes = self.dexes[dex as usize].bytes.clone();
        let d = Dex::parse(&bytes).expect("validated");
        let fail = |vm: &mut Vm, why: &str| -> JResult<CallSite> {
            let msg = format!("invoke-custom call site {site}: {why}");
            vm.throw_new("Ljava/lang/BootstrapMethodError;", &msg)
        };
        let Ok(vals) = d.call_site(site) else {
            return fail(self, "malformed call site");
        };
        let (
            Some(EncodedValue::MethodHandle(bsm)),
            Some(EncodedValue::String(name)),
            Some(EncodedValue::MethodType(invoked)),
        ) = (vals.first(), vals.get(1), vals.get(2))
        else {
            return fail(self, "malformed call site");
        };
        let bsm_name = d
            .method_handle(*bsm)
            .ok()
            .and_then(|(_, m)| d.method_id(m as u32).ok())
            .and_then(|m| {
                let c = d.type_descriptor(m.class_idx as u32).ok()?;
                let n = d.string(m.name_idx).ok()?;
                Some(format!("{c}->{n}"))
            })
            .unwrap_or_default();
        let alt = match bsm_name.as_str() {
            "Ljava/lang/invoke/LambdaMetafactory;->metafactory" => false,
            "Ljava/lang/invoke/LambdaMetafactory;->altMetafactory" => true,
            other => return fail(self, &format!("unsupported bootstrap method {other}")),
        };
        let (Some(EncodedValue::MethodType(sam)), Some(EncodedValue::MethodHandle(imh))) =
            (vals.get(3), vals.get(4))
        else {
            return fail(self, "malformed metafactory arguments");
        };
        let iface_name = d.string(*name).unwrap_or_default().into_owned();
        let invoked_sig = d.proto_signature(*invoked).unwrap_or_default();
        let sam_sig = d.proto_signature(*sam).unwrap_or_default();
        let (cap_kinds, iface_desc) = split_sig(&invoked_sig);
        let iface = self.find_class(&iface_desc)?;
        let mut interfaces = vec![iface];
        let mut sigs = vec![sam_sig];
        if alt {
            let flags = match vals.get(6) {
                Some(EncodedValue::Int(f)) => *f,
                _ => 0,
            };
            let mut i = 7;
            let count = |i: usize| match vals.get(i) {
                Some(EncodedValue::Int(n)) => *n as usize,
                _ => 0,
            };
            if flags & FLAG_MARKERS != 0 {
                let n = count(i);
                for k in 0..n {
                    if let Some(EncodedValue::Type(t)) = vals.get(i + 1 + k) {
                        let td = d.type_descriptor(*t).unwrap_or_default().into_owned();
                        interfaces.push(self.find_class(&td)?);
                    }
                }
                i += 1 + n;
            }
            if flags & FLAG_BRIDGES != 0 {
                let n = count(i);
                for k in 0..n {
                    if let Some(EncodedValue::MethodType(p)) = vals.get(i + 1 + k) {
                        sigs.push(d.proto_signature(*p).unwrap_or_default());
                    }
                }
            }
        }

        // Implementation method handle.
        let Ok((handle, impl_method)) = d.method_handle(*imh) else {
            return fail(self, "bad implementation handle");
        };
        if !matches!(
            handle,
            MH_INVOKE_STATIC
                | MH_INVOKE_INSTANCE
                | MH_INVOKE_CONSTRUCTOR
                | MH_INVOKE_DIRECT
                | MH_INVOKE_INTERFACE
        ) {
            return fail(self, "field method handles are not supported");
        }
        let mr = self.resolve_method_ref(dex, impl_method as u32)?;
        let target_sig = self.method_ref_name(mr.sig_id);
        let paren = target_sig.find('(').unwrap_or(0);
        let (mut impl_params, impl_ret) = split_sig(&target_sig[paren..]);
        if matches!(
            handle,
            MH_INVOKE_INSTANCE | MH_INVOKE_DIRECT | MH_INVOKE_INTERFACE
        ) {
            impl_params.insert(0, b'L');
        }
        let impl_ret = if handle == MH_INVOKE_CONSTRUCTOR {
            b'L'
        } else {
            impl_ret.as_bytes()[0]
        };

        // Synthetic class.
        self.lambdas.count += 1;
        let host = self.class(mr.class).name.trim_end_matches(';').to_string();
        let cname: Rc<str> = Rc::from(format!("{host}$$Lambda${};", self.lambdas.count));
        let mut ifields = Vec::new();
        let mut ref_slots = Vec::new();
        let mut captured = Vec::new();
        let mut slot = 0u32;
        for (i, &k) in cap_kinds.iter().enumerate() {
            if is_ref(k) {
                ref_slots.push(slot);
            }
            ifields.push(FieldDef {
                name: Rc::from(format!("f${i}")),
                desc: Rc::from(if is_ref(k) {
                    "Ljava/lang/Object;".to_string()
                } else {
                    (k as char).to_string()
                }),
                slot,
                flags: access::PRIVATE | access::FINAL,
            });
            captured.push((k, slot));
            slot += if is_wide(k) { 2 } else { 1 };
        }
        let id = ClassId(self.classes.len() as u32);
        let mut methods = Vec::new();
        for sig in &sigs {
            let (in_params, in_ret) = split_sig(sig);
            let arg_slots = 1 + in_params
                .iter()
                .map(|&k| if is_wide(k) { 2 } else { 1 })
                .sum::<u16>();
            let lambda = Lambda {
                handle,
                target_class: mr.class,
                target_sig: mr.sig_id,
                captured: captured.clone().into_boxed_slice(),
                in_params: in_params.into_boxed_slice(),
                in_ret: in_ret.as_bytes()[0],
                impl_params: impl_params.clone().into_boxed_slice(),
                impl_ret,
            };
            let sig_id = self.sig_id(&format!("{iface_name}{sig}"));
            let midx = self.methods.len() as u32;
            self.methods.push(Method {
                class: id,
                name: Rc::from(iface_name.as_str()),
                sig: Rc::from(sig.as_str()),
                sig_id,
                flags: access::PUBLIC,
                kind: MKind::Lambda(Rc::new(lambda)),
                arg_slots,
                ret: in_ret.as_bytes()[0],
                code_off: (0, 0),
            });
            methods.push(midx);
        }
        let object = self.object_class();
        self.register_class(Class {
            name: cname,
            super_: Some(object),
            interfaces,
            flags: access::PUBLIC | access::FINAL | 0x1000,
            src: ClassSrc::Synthetic,
            state: ClassState::Initialized,
            ifields,
            sfields: Vec::new(),
            statics: Vec::new(),
            nslots: slot,
            ref_slots: ref_slots.into_boxed_slice(),
            methods,
            vcache: HashMap::new(),
            mirror: 0,
            source_file: None,
        });
        let singleton = if cap_kinds.is_empty() {
            self.alloc_instance(id)
        } else {
            0
        };
        Ok(CallSite {
            class: id,
            singleton,
        })
    }

    /// Calls a lambda's implementation. `args[0]` is the lambda instance.
    pub(crate) fn call_lambda(&mut self, l: &Lambda, args: &[u32]) -> JResult<u64> {
        let pins = self.pins.len();
        let r = self.call_lambda_inner(l, args);
        self.pins.truncate(pins);
        r
    }

    fn call_lambda_inner(&mut self, l: &Lambda, args: &[u32]) -> JResult<u64> {
        let this = args[0];
        let mut out: Vec<u32> = Vec::with_capacity(args.len() + l.captured.len() + 1);
        // Values with their kinds, captured first, then incoming arguments.
        let mut vals: Vec<(u8, u64)> = Vec::with_capacity(l.captured.len() + l.in_params.len());
        if let Some(ObjData::Inst(f)) = self.heap.get(this).map(|o| &o.data) {
            for &(k, s) in l.captured.iter() {
                let s = s as usize;
                let v = if is_wide(k) {
                    f[s] as u64 | (f[s + 1] as u64) << 32
                } else {
                    f[s] as u64
                };
                vals.push((k, v));
            }
        }
        let mut i = 1;
        for &k in l.in_params.iter() {
            let v = if is_wide(k) {
                let v = args[i] as u64 | (args.get(i + 1).copied().unwrap_or(0) as u64) << 32;
                i += 2;
                v
            } else {
                let v = args.get(i).copied().unwrap_or(0) as u64;
                i += 1;
                v
            };
            vals.push((k, v));
        }
        for &(k, v) in &vals {
            if is_ref(k) && v != 0 {
                self.pins.push(v as u32);
            }
        }
        let ctor_obj = if l.handle == MH_INVOKE_CONSTRUCTOR {
            self.ensure_init(l.target_class)?;
            let o = self.alloc_instance(l.target_class);
            self.pins.push(o);
            out.push(o);
            o
        } else {
            0
        };
        for (n, &want) in l.impl_params.iter().enumerate() {
            let (have, v) = vals.get(n).copied().unwrap_or((want, 0));
            let v = self.convert(have, want, v)?;
            if is_ref(want) && v != 0 {
                self.pins.push(v as u32);
            }
            out.push(v as u32);
            if is_wide(want) {
                out.push((v >> 32) as u32);
            }
        }
        let target = match l.handle {
            MH_INVOKE_INSTANCE | MH_INVOKE_INTERFACE => {
                let recv = out[0];
                let Some(rc) = self.heap.get(recv).map(|o| o.class) else {
                    let name = self.method_ref_name(l.target_sig);
                    return self.throw_new(
                        "Ljava/lang/NullPointerException;",
                        &format!("Attempt to invoke {name} on a null object reference"),
                    );
                };
                self.find_virtual(rc, l.target_sig)
            }
            _ => self.find_method_in_hierarchy(l.target_class, l.target_sig),
        };
        let Some(target) = target else {
            let name = self.method_ref_name(l.target_sig);
            return self.throw_new("Ljava/lang/NoSuchMethodError;", &name);
        };
        if l.handle == MH_INVOKE_STATIC {
            let c = self.methods[target as usize].class;
            self.ensure_init(c)?;
        }
        let r = self.invoke(target, &out)?;
        let (have, r) = if l.handle == MH_INVOKE_CONSTRUCTOR {
            (b'L', ctor_obj as u64)
        } else {
            (l.impl_ret, r)
        };
        if l.in_ret == b'V' {
            return Ok(0);
        }
        self.convert(have, l.in_ret, r)
    }

    /// Adapts a value between descriptor kinds (box, unbox, widen).
    /// Boxing/unboxing/widening between descriptor kinds (also used by reflection).
    pub(crate) fn lambda_convert(&mut self, have: u8, want: u8, v: u64) -> JResult<u64> {
        self.convert(have, want, v)
    }

    fn convert(&mut self, have: u8, want: u8, v: u64) -> JResult<u64> {
        if have == want || (is_ref(have) && is_ref(want)) || have == b'V' {
            return Ok(v);
        }
        if is_ref(want) {
            // box
            let sig = format!(
                "valueOf({})L{};",
                have as char,
                &box_class(have)[1..box_class(have).len() - 1]
            );
            let c = self.find_class(box_class(have))?;
            self.ensure_init(c)?;
            let sid = self.sig_id(&sig);
            let Some(m) = self.find_method_in_hierarchy(c, sid) else {
                return self.throw_new("Ljava/lang/NoSuchMethodError;", &sig);
            };
            let mut a = vec![v as u32];
            if is_wide(have) {
                a.push((v >> 32) as u32);
            }
            return self.invoke(m, &a);
        }
        if is_ref(have) {
            // unbox: Character/Boolean have their own accessor, numbers go through Number.
            let obj = v as u32;
            let Some(rc) = self.heap.get(obj).map(|o| o.class) else {
                return self.throw_new("Ljava/lang/NullPointerException;", "unboxing a null value");
            };
            let (name, k) = match want {
                b'Z' => ("booleanValue()Z", b'Z'),
                b'C' => ("charValue()C", b'C'),
                b'J' => ("longValue()J", b'J'),
                b'F' => ("floatValue()F", b'F'),
                b'D' => ("doubleValue()D", b'D'),
                _ => ("intValue()I", b'I'),
            };
            let sid = self.sig_id(name);
            let Some(m) = self.find_virtual(rc, sid) else {
                let msg = format!("cannot unbox {}", self.class(rc).java_name());
                return self.throw_new("Ljava/lang/ClassCastException;", &msg);
            };
            let r = self.invoke(m, &[obj])?;
            return self.convert(k, want, r);
        }
        // primitive widening
        let as_long = |v: u64| -> i64 {
            match have {
                b'J' => v as i64,
                b'C' => (v as u32 & 0xffff) as i64,
                b'F' | b'D' => 0,
                _ => v as u32 as i32 as i64,
            }
        };
        Ok(match want {
            b'J' => match have {
                b'F' => f32::from_bits(v as u32) as i64 as u64,
                b'D' => f64::from_bits(v) as i64 as u64,
                _ => as_long(v) as u64,
            },
            b'F' => match have {
                b'D' => (f64::from_bits(v) as f32).to_bits() as u64,
                b'J' => (v as i64 as f32).to_bits() as u64,
                _ => (as_long(v) as f32).to_bits() as u64,
            },
            b'D' => match have {
                b'F' => (f32::from_bits(v as u32) as f64).to_bits(),
                b'J' => (v as i64 as f64).to_bits(),
                _ => (as_long(v) as f64).to_bits(),
            },
            _ => as_long(v) as u32 as u64,
        })
    }
}
