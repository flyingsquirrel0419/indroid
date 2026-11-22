//! java.lang.reflect support: Constructor/Method/Field objects backed by VM
//! method indices and field slots.

use std::collections::HashMap;

use indroid_dex::access;

use crate::heap::{ObjData, Ref};
use crate::{ClassId, JResult, NativeFn, Vm};

fn mirror_class(vm: &Vm, r: u32) -> Option<ClassId> {
    match vm.heap.get(r).map(|o| &o.data) {
        Some(ObjData::Mirror(c)) => Some(*c),
        _ => None,
    }
}

/// Splits a method signature "(II[Ljava/lang/String;)V" into parameter
/// descriptors and the return descriptor.
fn split(sig: &str) -> (Vec<String>, String) {
    let close = sig.find(')').unwrap_or(sig.len());
    let b = &sig.as_bytes()[1..close];
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        while b[i] == b'[' {
            i += 1;
        }
        if b[i] == b'L' {
            while b[i] != b';' {
                i += 1;
            }
        }
        i += 1;
        out.push(String::from_utf8_lossy(&b[start..i]).into_owned());
    }
    (out, sig[(close + 1).min(sig.len())..].to_string())
}

impl Vm {
    fn mirror_of_desc(&mut self, desc: &str) -> Option<Ref> {
        let c = self.try_find_class(desc)?;
        Some(self.mirror(c))
    }

    fn class_array(&mut self, mirrors: &[Ref]) -> JResult<Ref> {
        let arr = self.new_array("[Ljava/lang/Class;", mirrors.len())?;
        if let Some(ObjData::ArrL(a)) = self.heap.get_mut(arr).map(|o| &mut o.data) {
            a.copy_from_slice(mirrors);
        }
        Ok(arr)
    }

    fn reflect_executables(&mut self, c: ClassId, ctors: bool, public_only: bool) -> JResult<Ref> {
        let class_desc = if ctors {
            "Ljava/lang/reflect/Constructor;"
        } else {
            "Ljava/lang/reflect/Method;"
        };
        let ec = self.find_class(class_desc)?;
        let pins = self.pins.len();
        let mut objs = Vec::new();
        let owner = self.mirror(c);
        self.pins.push(owner);
        for m in self.class(c).methods.clone() {
            let meth = &self.methods[m as usize];
            let name = meth.name.clone();
            let is_ctor = &*name == "<init>";
            if is_ctor != ctors || &*name == "<clinit>" {
                continue;
            }
            if public_only && meth.flags & access::PUBLIC == 0 {
                continue;
            }
            let flags = meth.flags;
            let (params, ret) = split(&meth.sig.clone());
            let mut pm = Vec::with_capacity(params.len());
            let mut ok = true;
            for p in &params {
                match self.mirror_of_desc(p) {
                    Some(r) => pm.push(r),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            let ret_m = if ctors {
                Some(0)
            } else {
                self.mirror_of_desc(&ret)
            };
            let (true, Some(ret_m)) = (ok, ret_m) else {
                continue;
            };
            let obj = self.alloc_instance(ec);
            self.pins.push(obj);
            let arr = self.class_array(&pm)?;
            let jname = self.new_string(&name);
            self.set_field(obj, "declaringClass", owner);
            self.set_field(obj, "name", jname);
            self.set_field(obj, "parameterTypes", arr);
            self.set_field(obj, "modifiers", flags & 0xffff);
            self.set_field(obj, "slot", m);
            if !ctors {
                self.set_field(obj, "returnType", ret_m);
            }
            objs.push(obj);
        }
        let arr = self.new_array(&format!("[{class_desc}"), objs.len())?;
        if let Some(ObjData::ArrL(a)) = self.heap.get_mut(arr).map(|o| &mut o.data) {
            a.copy_from_slice(&objs);
        }
        self.pins.truncate(pins);
        Ok(arr)
    }

    fn reflect_fields(&mut self, c: ClassId, public_only: bool) -> JResult<Ref> {
        let fc = self.find_class("Ljava/lang/reflect/Field;")?;
        let pins = self.pins.len();
        let owner = self.mirror(c);
        self.pins.push(owner);
        let mut objs = Vec::new();
        let defs: Vec<(crate::FieldDef, bool)> = self
            .class(c)
            .ifields
            .iter()
            .map(|f| (f.clone(), false))
            .chain(self.class(c).sfields.iter().map(|f| (f.clone(), true)))
            .collect();
        for (f, is_static) in defs {
            if public_only && f.flags & access::PUBLIC == 0 {
                continue;
            }
            let Some(ty) = self.mirror_of_desc(&f.desc) else {
                continue;
            };
            let obj = self.alloc_instance(fc);
            self.pins.push(obj);
            let name = self.new_string(&f.name);
            let flags = (f.flags & 0xffff) | if is_static { access::STATIC } else { 0 };
            self.set_field(obj, "declaringClass", owner);
            self.set_field(obj, "name", name);
            self.set_field(obj, "type", ty);
            self.set_field(obj, "modifiers", flags);
            self.set_field(obj, "slot", f.slot);
            objs.push(obj);
        }
        let arr = self.new_array("[Ljava/lang/reflect/Field;", objs.len())?;
        if let Some(ObjData::ArrL(a)) = self.heap.get_mut(arr).map(|o| &mut o.data) {
            a.copy_from_slice(&objs);
        }
        self.pins.truncate(pins);
        Ok(arr)
    }

    /// Primitive kind of a Class mirror ('L' for references, 'V' for void).
    fn mirror_kind(&self, mirror: Ref) -> u8 {
        match mirror_class(self, mirror) {
            Some(c) => match self.class(c).src {
                crate::ClassSrc::Primitive(p) => p,
                _ => b'L',
            },
            None => b'L',
        }
    }

    fn reflect_invoke(&mut self, a: &[u32]) -> JResult<u64> {
        let (slot, is_virtual, recv, args, params, ret) = (a[0], a[1] != 0, a[2], a[3], a[4], a[5]);
        let pins = self.pins.len();
        self.pins.extend_from_slice(&[recv, args, params, ret]);
        let r = (|| -> JResult<u64> {
            let arg_refs: Vec<u32> = match self.heap.get(args).map(|o| &o.data) {
                Some(ObjData::ArrL(v)) => v.to_vec(),
                _ => Vec::new(),
            };
            let param_mirrors: Vec<u32> = match self.heap.get(params).map(|o| &o.data) {
                Some(ObjData::ArrL(v)) => v.to_vec(),
                _ => Vec::new(),
            };
            let meth_static = self.methods[slot as usize].is_static();
            let mut slots = Vec::with_capacity(arg_refs.len() * 2 + 1);
            if !meth_static {
                slots.push(recv);
            }
            for (i, &arg) in arg_refs.iter().enumerate() {
                let kind = param_mirrors
                    .get(i)
                    .map(|&m| self.mirror_kind(m))
                    .unwrap_or(b'L');
                if kind == b'L' {
                    if arg != 0 {
                        if let Some(&pm) = param_mirrors.get(i) {
                            if let Some(pc) = mirror_class(self, pm) {
                                if !self.instance_of(arg, pc) {
                                    return self.throw_new(
                                        "Ljava/lang/IllegalArgumentException;",
                                        &format!("argument {} has the wrong type", i + 1),
                                    );
                                }
                            }
                        }
                    }
                    slots.push(arg);
                } else {
                    if arg == 0 {
                        return self.throw_new(
                            "Ljava/lang/IllegalArgumentException;",
                            "null passed for a primitive argument",
                        );
                    }
                    let boxed = self
                        .heap
                        .get(arg)
                        .map(|o| self.class(o.class).name.clone())
                        .unwrap_or_default();
                    let ok = matches!(
                        &*boxed,
                        "Ljava/lang/Integer;"
                            | "Ljava/lang/Long;"
                            | "Ljava/lang/Short;"
                            | "Ljava/lang/Byte;"
                            | "Ljava/lang/Character;"
                            | "Ljava/lang/Boolean;"
                            | "Ljava/lang/Float;"
                            | "Ljava/lang/Double;"
                    ) && (kind == b'Z') == (&*boxed == "Ljava/lang/Boolean;");
                    if !ok {
                        return self.throw_new(
                            "Ljava/lang/IllegalArgumentException;",
                            &format!("argument {} has the wrong type", i + 1),
                        );
                    }
                    let v = self.lambda_convert(b'L', kind, arg as u64)?;
                    slots.push(v as u32);
                    if kind == b'J' || kind == b'D' {
                        slots.push((v >> 32) as u32);
                    }
                }
            }
            let mut target = slot;
            if is_virtual {
                let sig = self.methods[slot as usize].sig_id;
                if let Some(rc) = self.heap.get(recv).map(|o| o.class) {
                    if let Some(t) = self.find_virtual(rc, sig) {
                        target = t;
                    }
                }
            } else if meth_static {
                let c = self.methods[slot as usize].class;
                self.ensure_init(c)?;
            }
            let v = self.invoke(target, &slots)?;
            let rk = self.mirror_kind(ret);
            match rk {
                b'V' => Ok(0),
                b'L' => Ok(v),
                k => self.lambda_convert(k, b'L', v),
            }
        })();
        self.pins.truncate(pins);
        r
    }

    fn reflect_field(&mut self, a: &[u32], set: bool) -> JResult<u64> {
        let (cls, slot, is_static, target, ty) = (a[0], a[1] as usize, a[2] != 0, a[3], a[4]);
        let Some(c) = mirror_class(self, cls) else {
            return Ok(0);
        };
        let kind = self.mirror_kind(ty);
        let wide = kind == b'J' || kind == b'D';
        if is_static {
            self.ensure_init(c)?;
        }
        if set {
            let value = a[5];
            let raw = if kind == b'L' {
                value as u64
            } else {
                self.lambda_convert(b'L', kind, value as u64)?
            };
            let dst: Option<&mut [u32]> = if is_static {
                Some(&mut self.class_mut(c).statics[..])
            } else {
                match self.heap.get_mut(target).map(|o| &mut o.data) {
                    Some(ObjData::Inst(f)) => Some(&mut f[..]),
                    _ => None,
                }
            };
            if let Some(d) = dst {
                d[slot] = raw as u32;
                if wide {
                    d[slot + 1] = (raw >> 32) as u32;
                }
            }
            return Ok(0);
        }
        let src: Option<&[u32]> = if is_static {
            Some(&self.class(c).statics[..])
        } else {
            match self.heap.get(target).map(|o| &o.data) {
                Some(ObjData::Inst(f)) => Some(&f[..]),
                _ => None,
            }
        };
        let Some(s) = src else { return Ok(0) };
        let raw = s[slot] as u64 | if wide { (s[slot + 1] as u64) << 32 } else { 0 };
        if kind == b'L' {
            Ok(raw)
        } else {
            self.lambda_convert(kind, b'L', raw)
        }
    }
}

pub fn register(m: &mut HashMap<&'static str, NativeFn>) {
    m.insert(
        "Ljava/lang/Class;->constructors0(Z)[Ljava/lang/reflect/Constructor;",
        |vm, a| {
            let c = mirror_class(vm, a[0]).unwrap();
            vm.reflect_executables(c, true, a[1] != 0).map(|r| r as u64)
        },
    );
    m.insert(
        "Ljava/lang/Class;->methods0(Z)[Ljava/lang/reflect/Method;",
        |vm, a| {
            let c = mirror_class(vm, a[0]).unwrap();
            vm.reflect_executables(c, false, a[1] != 0)
                .map(|r| r as u64)
        },
    );
    m.insert(
        "Ljava/lang/Class;->fields0(Z)[Ljava/lang/reflect/Field;",
        |vm, a| {
            let c = mirror_class(vm, a[0]).unwrap();
            vm.reflect_fields(c, a[1] != 0).map(|r| r as u64)
        },
    );
    m.insert(
        "Ljava/lang/Class;->getInterfaces()[Ljava/lang/Class;",
        |vm, a| {
            let c = mirror_class(vm, a[0]).unwrap();
            let ifaces = vm.class(c).interfaces.clone();
            let mirrors: Vec<Ref> = ifaces.into_iter().map(|i| vm.mirror(i)).collect();
            vm.class_array(&mirrors).map(|r| r as u64)
        },
    );
    m.insert(
        "Ljava/lang/reflect/Executable;->invoke0(IZLjava/lang/Object;[Ljava/lang/Object;[Ljava/lang/Class;Ljava/lang/Class;)Ljava/lang/Object;",
        |vm, a| vm.reflect_invoke(a),
    );
    m.insert(
        "Ljava/lang/reflect/Constructor;->allocate(Ljava/lang/Class;)Ljava/lang/Object;",
        |vm, a| {
            let c = mirror_class(vm, a[0]).unwrap();
            vm.ensure_init(c)?;
            Ok(vm.alloc_instance(c) as u64)
        },
    );
    m.insert(
        "Ljava/lang/reflect/Field;->get0(Ljava/lang/Class;IZLjava/lang/Object;Ljava/lang/Class;)Ljava/lang/Object;",
        |vm, a| vm.reflect_field(a, false),
    );
    m.insert(
        "Ljava/lang/reflect/Field;->set0(Ljava/lang/Class;IZLjava/lang/Object;Ljava/lang/Class;Ljava/lang/Object;)V",
        |vm, a| vm.reflect_field(a, true),
    );
}
