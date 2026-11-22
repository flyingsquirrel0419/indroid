//! Rust implementations of `native` methods declared in core/src.

use std::cell::Cell;
use std::collections::HashMap;

use crate::heap::{JStr, Obj, ObjData, Ref, StrData};
use crate::{
    descriptor_to_java, java_to_descriptor, regex, text, ClassId, ClassSrc, JResult, NativeFn,
    Throw, Vm,
};

const SIOOBE: &str = "Ljava/lang/StringIndexOutOfBoundsException;";
const NPE: &str = "Ljava/lang/NullPointerException;";

pub fn registry() -> HashMap<&'static str, NativeFn> {
    let mut m: HashMap<&'static str, NativeFn> = HashMap::new();
    macro_rules! reg {
        ($($k:literal => $f:expr),* $(,)?) => { $( m.insert($k, $f); )* };
    }
    reg! {
        // java.lang.Object
        "Ljava/lang/Object;->getClass()Ljava/lang/Class;" => |vm, a| {
            let c = vm.heap.get(a[0]).unwrap().class;
            Ok(vm.mirror(c) as u64)
        },
        "Ljava/lang/Object;->hashCode()I" => |_, a| Ok(identity_hash(a[0]) as u64),
        "Ljava/lang/Object;->clone()Ljava/lang/Object;" => object_clone,
        "Ljava/lang/System;->identityHashCode(Ljava/lang/Object;)I" => |_, a| Ok(identity_hash(a[0]) as u64),

        // java.lang.String
        "Ljava/lang/String;->initChars([CII)V" => |vm, a| {
            let (off, len) = (a[2] as i32, a[3] as i32);
            let units = match vm.heap.get(a[1]).map(|o| &o.data) {
                Some(ObjData::ArrC(c)) => {
                    if off < 0 || len < 0 || (off as usize + len as usize) > c.len() {
                        None
                    } else {
                        Some(c[off as usize..(off + len) as usize].to_vec())
                    }
                }
                _ => return vm.throw_new(NPE, "chars"),
            };
            let Some(units) = units else { return vm.throw_new(SIOOBE, &format!("offset={off} count={len}")) };
            set_str(vm, a[0], JStr::from_utf16(&units));
            Ok(0)
        },
        "Ljava/lang/String;->initBytes([BII)V" => |vm, a| {
            let (off, len) = (a[2] as usize, a[3] as usize);
            let bytes: Vec<u8> = match vm.heap.get(a[1]).map(|o| &o.data) {
                Some(ObjData::ArrB(b)) if off + len <= b.len() => b[off..off + len].iter().map(|&x| x as u8).collect(),
                Some(ObjData::ArrB(_)) => return vm.throw_new(SIOOBE, "bytes"),
                _ => return vm.throw_new(NPE, "bytes"),
            };
            let s = String::from_utf8_lossy(&bytes);
            set_str(vm, a[0], JStr::from_rust(&s));
            Ok(0)
        },
        "Ljava/lang/String;->length()I" => |vm, a| Ok(js(vm, a[0]).len() as u64),
        "Ljava/lang/String;->charAt(I)C" => |vm, a| {
            let s = js(vm, a[0]);
            let i = a[1] as i32;
            if i < 0 || i as usize >= s.len() {
                let msg = format!("index={i}, length={}", s.len());
                return vm.throw_new(SIOOBE, &msg);
            }
            Ok(s.at(i as usize) as u64)
        },
        "Ljava/lang/String;->equals(Ljava/lang/Object;)Z" => |vm, a| {
            if a[0] == a[1] { return Ok(1) }
            Ok(match vm.jstr(a[1]) { Some(o) => js(vm, a[0]).equals(o) as u64, None => 0 })
        },
        "Ljava/lang/String;->hashCode()I" => |vm, a| {
            match &mut vm.heap.get_mut(a[0]).unwrap().data {
                ObjData::Str(s) => Ok(s.java_hash() as u64),
                _ => Ok(0),
            }
        },
        "Ljava/lang/String;->compareTo(Ljava/lang/String;)I" => |vm, a| {
            let Some(o) = vm.jstr(a[1]) else { return vm.throw_new(NPE, "compareTo null") };
            let s = js(vm, a[0]);
            let n = s.len().min(o.len());
            for i in 0..n {
                let (x, y) = (s.at(i), o.at(i));
                if x != y { return Ok((x as i32 - y as i32) as u32 as u64) }
            }
            Ok((s.len() as i32 - o.len() as i32) as u32 as u64)
        },
        "Ljava/lang/String;->equalsIgnoreCase(Ljava/lang/String;)Z" => |vm, a| {
            let Some(o) = vm.jstr(a[1]) else { return Ok(0) };
            let s = js(vm, a[0]);
            if s.len() != o.len() { return Ok(0) }
            Ok((0..s.len()).all(|i| fold(s.at(i)) == fold(o.at(i))) as u64)
        },
        "Ljava/lang/String;->indexOf(II)I" => |vm, a| {
            let s = js(vm, a[0]);
            let ch = a[1];
            let from = (a[2] as i32).max(0) as usize;
            if ch >= 0x10000 {
                let u = utf16_of(ch);
                return Ok(find_sub(&s.to_utf16(), &u, from) as u32 as u64);
            }
            Ok((from..s.len()).find(|&i| s.at(i) as u32 == ch).map(|i| i as i32).unwrap_or(-1) as u32 as u64)
        },
        "Ljava/lang/String;->indexOf(Ljava/lang/String;I)I" => |vm, a| {
            let Some(o) = vm.jstr(a[1]) else { return vm.throw_new(NPE, "indexOf null") };
            let needle = o.to_utf16();
            let hay = js(vm, a[0]).to_utf16();
            let from = (a[2] as i32).max(0) as usize;
            Ok(find_sub(&hay, &needle, from) as u32 as u64)
        },
        "Ljava/lang/String;->lastIndexOf(II)I" => |vm, a| {
            let s = js(vm, a[0]);
            let from = a[2] as i32;
            if from < 0 { return Ok(-1i32 as u32 as u64) }
            let from = (from as usize).min(s.len().saturating_sub(1));
            if s.is_empty() { return Ok(-1i32 as u32 as u64) }
            Ok((0..=from).rev().find(|&i| s.at(i) as u32 == a[1]).map(|i| i as i32).unwrap_or(-1) as u32 as u64)
        },
        "Ljava/lang/String;->lastIndexOf(Ljava/lang/String;I)I" => |vm, a| {
            let Some(o) = vm.jstr(a[1]) else { return vm.throw_new(NPE, "lastIndexOf null") };
            let needle = o.to_utf16();
            let hay = js(vm, a[0]).to_utf16();
            let from = a[2] as i32;
            if from < 0 || needle.len() > hay.len() { return Ok(-1i32 as u32 as u64) }
            let start = (from as usize).min(hay.len() - needle.len());
            Ok((0..=start).rev().find(|&i| hay[i..i + needle.len()] == needle[..]).map(|i| i as i32).unwrap_or(-1) as u32 as u64)
        },
        "Ljava/lang/String;->substring(II)Ljava/lang/String;" => |vm, a| {
            let s = js(vm, a[0]);
            let (b, e) = (a[1] as i32, a[2] as i32);
            if b < 0 || e as usize > s.len() || b > e || e < 0 {
                let msg = format!("begin {b}, end {e}, length {}", s.len());
                return vm.throw_new(SIOOBE, &msg);
            }
            if b == 0 && e as usize == s.len() { return Ok(a[0] as u64) }
            let (b, e) = (b as usize, e as usize);
            let js2 = match &s.data {
                StrData::Latin1(x) => JStr { data: StrData::Latin1(x[b..e].into()), hash: 0 },
                StrData::Utf16(x) => JStr::from_utf16(&x[b..e]),
            };
            Ok(vm.alloc_string(js2) as u64)
        },
        "Ljava/lang/String;->concat(Ljava/lang/String;)Ljava/lang/String;" => |vm, a| {
            let Some(o) = vm.jstr(a[1]) else { return vm.throw_new(NPE, "concat null") };
            if o.is_empty() { return Ok(a[0] as u64) }
            let mut u = js(vm, a[0]).to_utf16();
            u.extend(o.to_utf16());
            Ok(vm.new_string_utf16(&u) as u64)
        },
        "Ljava/lang/String;->intern()Ljava/lang/String;" => |vm, a| {
            let u = js(vm, a[0]).to_utf16();
            Ok(vm.intern_utf16(&u) as u64)
        },
        "Ljava/lang/String;->toCharArray()[C" => |vm, a| {
            let u = js(vm, a[0]).to_utf16();
            let arr = vm.new_array("[C", u.len())?;
            if let ObjData::ArrC(c) = &mut vm.heap.get_mut(arr).unwrap().data { c.copy_from_slice(&u) }
            Ok(arr as u64)
        },
        "Ljava/lang/String;->getChars(II[CI)V" => |vm, a| {
            let s = js(vm, a[0]);
            let (sb, se, db) = (a[1] as i32, a[2] as i32, a[4] as i32);
            if sb < 0 || se as usize > s.len() || sb > se {
                return vm.throw_new(SIOOBE, &format!("srcBegin {sb}, srcEnd {se}, length {}", s.len()));
            }
            let src: Vec<u16> = (sb as usize..se as usize).map(|i| s.at(i)).collect();
            match vm.heap.get_mut(a[3]).map(|o| &mut o.data) {
                Some(ObjData::ArrC(d)) => {
                    if db < 0 || db as usize + src.len() > d.len() {
                        return vm.throw_new("Ljava/lang/ArrayIndexOutOfBoundsException;", "getChars");
                    }
                    d[db as usize..db as usize + src.len()].copy_from_slice(&src);
                    Ok(0)
                }
                _ => vm.throw_new(NPE, "getChars dst"),
            }
        },
        "Ljava/lang/String;->getBytes()[B" => |vm, a| {
            let s = js(vm, a[0]).to_rust();
            let arr = vm.new_array("[B", s.len())?;
            if let ObjData::ArrB(b) = &mut vm.heap.get_mut(arr).unwrap().data {
                for (d, x) in b.iter_mut().zip(s.bytes()) { *d = x as i8 }
            }
            Ok(arr as u64)
        },
        "Ljava/lang/String;->startsWith(Ljava/lang/String;I)Z" => |vm, a| {
            let Some(p) = vm.jstr(a[1]) else { return vm.throw_new(NPE, "startsWith null") };
            let s = js(vm, a[0]);
            let off = a[2] as i32;
            if off < 0 || off as usize + p.len() > s.len() { return Ok(0) }
            Ok((0..p.len()).all(|i| s.at(off as usize + i) == p.at(i)) as u64)
        },
        "Ljava/lang/String;->toLowerCase()Ljava/lang/String;" => |vm, a| {
            let s = js(vm, a[0]).to_rust();
            if !s.chars().any(|c| c.is_uppercase()) { return Ok(a[0] as u64) }
            Ok(vm.new_string(&s.to_lowercase()) as u64)
        },
        "Ljava/lang/String;->toUpperCase()Ljava/lang/String;" => |vm, a| {
            let s = js(vm, a[0]).to_rust();
            if !s.chars().any(|c| c.is_lowercase()) { return Ok(a[0] as u64) }
            Ok(vm.new_string(&s.to_uppercase()) as u64)
        },
        "Ljava/lang/String;->trim()Ljava/lang/String;" => |vm, a| {
            let u = js(vm, a[0]).to_utf16();
            let b = u.iter().position(|&c| c > 0x20).unwrap_or(u.len());
            let e = u.iter().rposition(|&c| c > 0x20).map(|x| x + 1).unwrap_or(b);
            if b == 0 && e == u.len() { return Ok(a[0] as u64) }
            Ok(vm.new_string_utf16(&u[b..e]) as u64)
        },
        "Ljava/lang/String;->replace(CC)Ljava/lang/String;" => |vm, a| {
            let (from, to) = (a[1] as u16, a[2] as u16);
            let mut u = js(vm, a[0]).to_utf16();
            if !u.contains(&from) { return Ok(a[0] as u64) }
            for c in u.iter_mut() { if *c == from { *c = to } }
            Ok(vm.new_string_utf16(&u) as u64)
        },
        "Ljava/lang/String;->matches(Ljava/lang/String;)Z" => |vm, a| {
            let p = str_arg(vm, a[1])?;
            let s = js(vm, a[0]).to_utf16();
            match regex::Regex::new(&p) {
                Ok(r) => Ok(r.full_match(&s) as u64),
                Err(e) => vm.throw_new("Ljava/util/regex/PatternSyntaxException;", &e),
            }
        },
        "Ljava/lang/String;->replaceAll(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;" => |vm, a| replace_regex(vm, a, true),
        "Ljava/lang/String;->replaceFirst(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;" => |vm, a| replace_regex(vm, a, false),
        "Ljava/lang/String;->split(Ljava/lang/String;I)[Ljava/lang/String;" => |vm, a| {
            let p = str_arg(vm, a[1])?;
            let s = js(vm, a[0]).to_utf16();
            let parts = match regex::split(&s, &p, a[2] as i32) {
                Ok(p) => p,
                Err(e) => return vm.throw_new("Ljava/util/regex/PatternSyntaxException;", &e),
            };
            let arr = vm.new_array("[Ljava/lang/String;", parts.len())?;
            for (i, part) in parts.iter().enumerate() {
                let r = vm.new_string_utf16(part);
                if let ObjData::ArrL(x) = &mut vm.heap.get_mut(arr).unwrap().data { x[i] = r }
            }
            Ok(arr as u64)
        },
        "Ljava/lang/String;->valueOf(C)Ljava/lang/String;" => |vm, a| Ok(vm.new_string_utf16(&[a[0] as u16]) as u64),
        "Ljava/lang/String;->format(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;" => |vm, a| {
            let f = str_arg(vm, a[0])?;
            let s = format(vm, &String::from_utf16_lossy(&f), a[1])?;
            Ok(vm.new_string(&s) as u64)
        },

        // java.lang.Class
        "Ljava/lang/Class;->getName()Ljava/lang/String;" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            let n = descriptor_to_java(&vm.class(c).name);
            Ok(vm.new_string(&n) as u64)
        },
        "Ljava/lang/Class;->isInstance(Ljava/lang/Object;)Z" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            Ok(vm.instance_of(a[1], c) as u64)
        },
        "Ljava/lang/Class;->isAssignableFrom(Ljava/lang/Class;)Z" => |vm, a| {
            if a[1] == 0 { return vm.throw_new(NPE, "class") }
            let (c, o) = (mirror_class(vm, a[0]), mirror_class(vm, a[1]));
            Ok(vm.is_subclass(o, c) as u64)
        },
        "Ljava/lang/Class;->isInterface()Z" => |vm, a| { let c = mirror_class(vm, a[0]); Ok(vm.class(c).is_interface() as u64) },
        "Ljava/lang/Class;->isArray()Z" => |vm, a| { let c = mirror_class(vm, a[0]); Ok(vm.class(c).is_array() as u64) },
        "Ljava/lang/Class;->isPrimitive()Z" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            Ok(matches!(vm.class(c).src, ClassSrc::Primitive(_)) as u64)
        },
        "Ljava/lang/Class;->getModifiers()I" => |vm, a| { let c = mirror_class(vm, a[0]); Ok((vm.class(c).flags & 0xffff) as u64) },
        "Ljava/lang/Class;->getSuperclass()Ljava/lang/Class;" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            if vm.class(c).is_interface() { return Ok(0) }
            match vm.class(c).super_ { Some(s) => Ok(vm.mirror(s) as u64), None => Ok(0) }
        },
        "Ljava/lang/Class;->getComponentType()Ljava/lang/Class;" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            match vm.class(c).src { ClassSrc::Array { component } => Ok(vm.mirror(component) as u64), _ => Ok(0) }
        },
        "Ljava/lang/Class;->newInstance()Ljava/lang/Object;" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            Ok(new_instance(vm, c)? as u64)
        },
        "Ljava/lang/Class;->getEnumConstants()[Ljava/lang/Object;" => |vm, a| {
            let c = mirror_class(vm, a[0]);
            let sig = format!("values()[{}", vm.class(c).name);
            vm.ensure_init(c)?;
            match vm.find_declared(c, &sig) {
                Some(m) => vm.invoke(m, &[]),
                None => Ok(0),
            }
        },
        "Ljava/lang/Class;->forName(Ljava/lang/String;)Ljava/lang/Class;" => |vm, a| {
            let name = match vm.rust_string(a[0]) { Some(n) => n, None => return vm.throw_new(NPE, "name") };
            match vm.try_find_class(&java_to_descriptor(&name)) {
                Some(c) => { vm.ensure_init(c)?; Ok(vm.mirror(c) as u64) }
                None => { vm.exception = 0; vm.throw_new("Ljava/lang/ClassNotFoundException;", &name) }
            }
        },
        "Ljava/lang/VMPrimitive;->get(C)Ljava/lang/Class;" => |vm, a| {
            let d = char::from_u32(a[0]).unwrap_or('V').to_string();
            let c = vm.find_class(&d)?;
            Ok(vm.mirror(c) as u64)
        },

        // java.lang.System
        "Ljava/lang/System;->arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V" => arraycopy,
        "Ljava/lang/System;->currentTimeMillis()J" => |_, _| {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            Ok(t)
        },
        "Ljava/lang/System;->nanoTime()J" => |vm, _| Ok(vm.start_time.elapsed().as_nanos() as u64),
        "Ljava/lang/System;->exit(I)V" => |vm, a| { vm.exit_code = Some(a[0] as i32); vm.exception = 0; Err(Throw) },
        "Ljava/io/PrintStream;->writeFd(ILjava/lang/String;)V" => |vm, a| {
            let s = vm.rust_string(a[1]).unwrap_or_else(|| "null".into());
            (vm.out)(a[0] as i32, &s);
            Ok(0)
        },

        // java.lang.Throwable
        "Ljava/lang/Throwable;->fillInStackTrace()Ljava/lang/Throwable;" => fill_in_stack_trace,

        // java.lang.Float / Double
        "Ljava/lang/Float;->toString(F)Ljava/lang/String;" => |vm, a| { let s = text::float_to_string(f32::from_bits(a[0])); Ok(vm.new_string(&s) as u64) },
        "Ljava/lang/Float;->floatToRawIntBits(F)I" => |_, a| Ok(a[0] as u64),
        "Ljava/lang/Float;->intBitsToFloat(I)F" => |_, a| Ok(a[0] as u64),
        "Ljava/lang/Double;->toString(D)Ljava/lang/String;" => |vm, a| { let s = text::double_to_string(f64::from_bits(wide(a, 0))); Ok(vm.new_string(&s) as u64) },
        "Ljava/lang/Double;->doubleToRawLongBits(D)J" => |_, a| Ok(wide(a, 0)),
        "Ljava/lang/Double;->longBitsToDouble(J)D" => |_, a| Ok(wide(a, 0)),
        "Ljava/lang/Double;->parseDouble(Ljava/lang/String;)D" => |vm, a| {
            let Some(s) = vm.rust_string(a[0]) else { return vm.throw_new(NPE, "parseDouble null") };
            match text::parse_double(&s) {
                Some(d) => Ok(d.to_bits()),
                None => vm.throw_new("Ljava/lang/NumberFormatException;", &format!("For input string: \"{s}\"")),
            }
        },

        // java.lang.Character
        "Ljava/lang/Character;->isLetter(I)Z" => |_, a| Ok(char::from_u32(a[0]).map(|c| c.is_alphabetic()).unwrap_or(false) as u64),
        "Ljava/lang/Character;->isUpperCase(I)Z" => |_, a| Ok(char::from_u32(a[0]).map(|c| c.is_uppercase()).unwrap_or(false) as u64),
        "Ljava/lang/Character;->isLowerCase(I)Z" => |_, a| Ok(char::from_u32(a[0]).map(|c| c.is_lowercase()).unwrap_or(false) as u64),
        "Ljava/lang/Character;->toUpperCase(I)I" => |_, a| Ok(map_case(a[0], true) as u64),
        "Ljava/lang/Character;->toLowerCase(I)I" => |_, a| Ok(map_case(a[0], false) as u64),

        "Ljava/lang/Math;->random()D" => |_, _| Ok(random_f64().to_bits()),
    }
    macro_rules! math1 {
        ($($name:literal => $f:expr),*) => { $( m.insert(concat!("Ljava/lang/Math;->", $name, "(D)D"), |_, a| { let f: fn(f64) -> f64 = $f; Ok(f(f64::from_bits(wide(a, 0))).to_bits()) }); )* };
    }
    math1! {
        "sin" => f64::sin, "cos" => f64::cos, "tan" => f64::tan, "asin" => f64::asin, "acos" => f64::acos,
        "atan" => f64::atan, "sinh" => f64::sinh, "cosh" => f64::cosh, "tanh" => f64::tanh, "exp" => f64::exp,
        "log" => f64::ln, "log10" => f64::log10, "log1p" => f64::ln_1p, "expm1" => f64::exp_m1, "sqrt" => f64::sqrt,
        "cbrt" => f64::cbrt, "ceil" => f64::ceil, "floor" => f64::floor, "rint" => |x: f64| {
            let r = x.round();
            if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }
        }
    }
    m.insert("Ljava/lang/Math;->atan2(DD)D", |_, a| {
        Ok(f64::from_bits(wide(a, 0))
            .atan2(f64::from_bits(wide(a, 2)))
            .to_bits())
    });
    m.insert("Ljava/lang/Math;->pow(DD)D", |_, a| {
        Ok(java_pow(f64::from_bits(wide(a, 0)), f64::from_bits(wide(a, 2))).to_bits())
    });
    m.insert("Ljava/lang/Math;->hypot(DD)D", |_, a| {
        Ok(f64::from_bits(wide(a, 0))
            .hypot(f64::from_bits(wide(a, 2)))
            .to_bits())
    });
    crate::natives_android::register(&mut m);
    crate::natives_android::register_reflect(&mut m);
    crate::reflect::register(&mut m);
    m
}

#[inline]
fn wide(a: &[u32], i: usize) -> u64 {
    a[i] as u64 | ((a[i + 1] as u64) << 32)
}

fn java_pow(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return 1.0;
    }
    x.powf(y)
}

fn identity_hash(r: Ref) -> u32 {
    // Spread handles so hashes look like the JVM's (and distribute well).
    let mut x = r.wrapping_mul(0x9E37_79B9);
    x ^= x >> 15;
    x & 0x7fff_ffff
}

fn fold(c: u16) -> u32 {
    let ch = char::from_u32(c as u32).unwrap_or('\0');
    ch.to_lowercase().next().unwrap_or(ch) as u32
}

fn map_case(c: u32, upper: bool) -> u32 {
    let Some(ch) = char::from_u32(c) else {
        return c;
    };
    let mut it: Box<dyn Iterator<Item = char>> = if upper {
        Box::new(ch.to_uppercase())
    } else {
        Box::new(ch.to_lowercase())
    };
    let first = it.next().unwrap_or(ch);
    if it.next().is_some() {
        c
    } else {
        first as u32
    }
}

fn utf16_of(cp: u32) -> Vec<u16> {
    char::from_u32(cp)
        .map(|c| c.encode_utf16(&mut [0; 2]).to_vec())
        .unwrap_or_default()
}

fn find_sub(hay: &[u16], needle: &[u16], from: usize) -> i32 {
    if needle.is_empty() {
        return from.min(hay.len()) as i32;
    }
    if from >= hay.len() || needle.len() > hay.len() {
        return -1;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| (p + from) as i32)
        .unwrap_or(-1)
}

thread_local! {
    static RNG: Cell<u64> = Cell::new(0x2545_F491_4F6C_DD1D ^ (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)));
}

pub fn random_f64() -> f64 {
    RNG.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    })
}

/// `this` of a String native: always a non-null string.
#[inline]
fn js(vm: &Vm, r: Ref) -> &JStr {
    vm.jstr(r).expect("string receiver")
}

fn str_arg(vm: &mut Vm, r: Ref) -> JResult<Vec<u16>> {
    match vm.jstr(r) {
        Some(s) => Ok(s.to_utf16()),
        None => vm.throw_new(NPE, "null string argument"),
    }
}

fn set_str(vm: &mut Vm, r: Ref, s: JStr) {
    if let Some(o) = vm.heap.get_mut(r) {
        o.data = ObjData::Str(s);
    }
}

fn mirror_class(vm: &Vm, r: Ref) -> ClassId {
    match vm.heap.get(r).map(|o| &o.data) {
        Some(ObjData::Mirror(c)) => *c,
        _ => panic!("not a class mirror"),
    }
}

fn replace_regex(vm: &mut Vm, a: &[u32], all: bool) -> JResult<u64> {
    let p = str_arg(vm, a[1])?;
    let r = str_arg(vm, a[2])?;
    let s = js(vm, a[0]).to_utf16();
    match regex::replace(&s, &p, &r, all) {
        Ok(out) => Ok(vm.new_string_utf16(&out) as u64),
        Err(e) => vm.throw_new("Ljava/util/regex/PatternSyntaxException;", &e),
    }
}

pub fn new_instance(vm: &mut Vm, c: ClassId) -> JResult<Ref> {
    use indroid_dex::access;
    if vm.class(c).flags & (access::ABSTRACT | access::INTERFACE) != 0 {
        let n = vm.class(c).java_name();
        return vm.throw_new("Ljava/lang/InstantiationException;", &n);
    }
    vm.ensure_init(c)?;
    let Some(ctor) = vm.find_declared(c, "<init>()V") else {
        let n = format!(
            "{} has no zero argument constructor",
            vm.class(c).java_name()
        );
        return vm.throw_new("Ljava/lang/InstantiationException;", &n);
    };
    let obj = vm.alloc_instance(c);
    vm.pins.push(obj);
    let r = vm.invoke(ctor, &[obj]);
    vm.pins.pop();
    r.map(|_| obj)
}

fn object_clone(vm: &mut Vm, a: &[u32]) -> JResult<u64> {
    let obj = vm.heap.get(a[0]).unwrap().clone();
    if let ObjData::Inst(_) = obj.data {
        let cl = vm.find_class("Ljava/lang/Cloneable;")?;
        if !vm.is_subclass(obj.class, cl) {
            let n = vm.class(obj.class).java_name();
            return vm.throw_new("Ljava/lang/CloneNotSupportedException;", &n);
        }
    }
    let data = match obj.data {
        ObjData::Str(mut s) => {
            s.hash = 0;
            ObjData::Str(s)
        }
        d => d,
    };
    Ok(vm.heap.alloc(Obj {
        class: obj.class,
        data,
    }) as u64)
}

fn arraycopy(vm: &mut Vm, a: &[u32]) -> JResult<u64> {
    let (src, sp, dst, dp, len) = (a[0], a[1] as i32, a[2], a[3] as i32, a[4] as i32);
    if src == 0 || dst == 0 {
        return vm.throw_new(NPE, "arraycopy null");
    }
    let sl = vm.heap.get(src).and_then(|o| o.data.array_len());
    let dl = vm.heap.get(dst).and_then(|o| o.data.array_len());
    let (Some(sl), Some(dl)) = (sl, dl) else {
        return vm.throw_new("Ljava/lang/ArrayStoreException;", "arraycopy: not an array");
    };
    if sp < 0
        || dp < 0
        || len < 0
        || sp as usize + len as usize > sl
        || dp as usize + len as usize > dl
    {
        let msg = format!("src.length={sl} srcPos={sp} dst.length={dl} dstPos={dp} length={len}");
        return vm.throw_new("Ljava/lang/ArrayIndexOutOfBoundsException;", &msg);
    }
    let (sp, dp, len) = (sp as usize, dp as usize, len as usize);
    if len == 0 {
        return Ok(0);
    }
    if src == dst {
        let o = vm.heap.get_mut(src).unwrap();
        match &mut o.data {
            ObjData::ArrB(x) => x.copy_within(sp..sp + len, dp),
            ObjData::ArrC(x) => x.copy_within(sp..sp + len, dp),
            ObjData::ArrS(x) => x.copy_within(sp..sp + len, dp),
            ObjData::ArrI(x) => x.copy_within(sp..sp + len, dp),
            ObjData::ArrJ(x) => x.copy_within(sp..sp + len, dp),
            ObjData::ArrL(x) => x.copy_within(sp..sp + len, dp),
            _ => {}
        }
        return Ok(0);
    }
    // Take the source out temporarily to avoid aliasing borrows.
    let src_data = std::mem::replace(
        &mut vm.heap.get_mut(src).unwrap().data,
        ObjData::ArrB(Box::new([])),
    );
    let mut store_err = false;
    let mut type_err = false;
    if let ObjData::ArrL(s) = &src_data {
        let dc = vm.heap.get(dst).unwrap().class;
        let sc = vm.heap.get(src).unwrap().class;
        if dc != sc {
            if let ClassSrc::Array { component } = vm.class(dc).src {
                store_err = s[sp..sp + len]
                    .iter()
                    .any(|&e| e != 0 && !vm.instance_of(e, component));
            }
        }
    }
    if !store_err {
        let d = &mut vm.heap.get_mut(dst).unwrap().data;
        match (&src_data, d) {
            (ObjData::ArrB(s), ObjData::ArrB(d)) => {
                d[dp..dp + len].copy_from_slice(&s[sp..sp + len])
            }
            (ObjData::ArrC(s), ObjData::ArrC(d)) => {
                d[dp..dp + len].copy_from_slice(&s[sp..sp + len])
            }
            (ObjData::ArrS(s), ObjData::ArrS(d)) => {
                d[dp..dp + len].copy_from_slice(&s[sp..sp + len])
            }
            (ObjData::ArrI(s), ObjData::ArrI(d)) => {
                d[dp..dp + len].copy_from_slice(&s[sp..sp + len])
            }
            (ObjData::ArrJ(s), ObjData::ArrJ(d)) => {
                d[dp..dp + len].copy_from_slice(&s[sp..sp + len])
            }
            (ObjData::ArrL(s), ObjData::ArrL(d)) => {
                d[dp..dp + len].copy_from_slice(&s[sp..sp + len])
            }
            _ => type_err = true,
        }
    }
    vm.heap.get_mut(src).unwrap().data = src_data;
    if store_err || type_err {
        return vm.throw_new(
            "Ljava/lang/ArrayStoreException;",
            "arraycopy: type mismatch",
        );
    }
    Ok(0)
}

fn fill_in_stack_trace(vm: &mut Vm, a: &[u32]) -> JResult<u64> {
    let this = a[0];
    let this_class = vm.heap.get(this).unwrap().class;
    let mut frames: Vec<String> = Vec::new();
    let mut skipping = true;
    for f in vm.frames.iter().rev() {
        let m = &vm.methods[f.method as usize];
        // Skip the exception's own constructor chain.
        if skipping
            && (&*m.name == "<init>" || &*m.name == "fillInStackTrace")
            && vm.is_subclass(this_class, m.class)
        {
            continue;
        }
        skipping = false;
        let c = vm.class(m.class);
        let line = crate::debuginfo::line_for(vm, f.method, f.pc as u32);
        frames.push(format!(
            "{}\0{}\0{}\0{}",
            c.java_name(),
            m.name,
            c.source_file.as_deref().unwrap_or(""),
            line
        ));
        if frames.len() >= 64 {
            break;
        }
    }
    let arr = vm.new_array("[Ljava/lang/String;", frames.len())?;
    vm.pins.push(arr);
    for (i, f) in frames.iter().enumerate() {
        let s = vm.new_string(f);
        if let ObjData::ArrL(x) = &mut vm.heap.get_mut(arr).unwrap().data {
            x[i] = s;
        }
    }
    vm.pins.pop();
    vm.set_field(this, "vmTrace", arr);
    vm.set_field(this, "stackTrace", 0);
    Ok(this as u64)
}

// ----------------------------------------------------------------------
// String.format
// ----------------------------------------------------------------------

enum Num {
    I(i64, u32),
    F(f64),
    C(u32),
    Z(bool),
}

fn unbox(vm: &Vm, r: Ref) -> Option<Num> {
    let o = vm.heap.get(r)?;
    let name = &*vm.class(o.class).name;
    let ObjData::Inst(f) = &o.data else {
        return None;
    };
    let v0 = *f.first()?;
    Some(match name {
        "Ljava/lang/Integer;" => Num::I(v0 as i32 as i64, 32),
        "Ljava/lang/Short;" => Num::I(v0 as i32 as i64, 16),
        "Ljava/lang/Byte;" => Num::I(v0 as i32 as i64, 8),
        "Ljava/lang/Long;" => Num::I((v0 as u64 | ((f[1] as u64) << 32)) as i64, 64),
        "Ljava/lang/Float;" => Num::F(f32::from_bits(v0) as f64),
        "Ljava/lang/Double;" => Num::F(f64::from_bits(v0 as u64 | ((f[1] as u64) << 32))),
        "Ljava/lang/Character;" => Num::C(v0),
        "Ljava/lang/Boolean;" => Num::Z(v0 != 0),
        _ => {
            // AtomicInteger etc.: fall back to field "value" if present
            return None;
        }
    })
}

fn format(vm: &mut Vm, fmt: &str, args: Ref) -> JResult<String> {
    let argv: Vec<Ref> = match vm.heap.get(args).map(|o| &o.data) {
        Some(ObjData::ArrL(x)) => x.to_vec(),
        _ => Vec::new(),
    };
    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    let mut next_arg = 0usize;
    let bad = |vm: &mut Vm, m: &str| -> JResult<String> {
        vm.throw_new("Ljava/util/IllegalFormatException;", m)
    };
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        // [index$][flags][width][.prec]conv
        let start = i;
        let mut num = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            num.push(chars[i]);
            i += 1;
        }
        let mut explicit: Option<usize> = None;
        if i < chars.len() && chars[i] == '$' && !num.is_empty() {
            explicit = num.parse::<usize>().ok().map(|n| n - 1);
            i += 1;
        } else {
            i = start;
        }
        let mut flags = String::new();
        while i < chars.len() && "-#+ 0,(".contains(chars[i]) {
            flags.push(chars[i]);
            i += 1;
        }
        let mut width = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            width.push(chars[i]);
            i += 1;
        }
        let mut prec: Option<usize> = None;
        if i < chars.len() && chars[i] == '.' {
            i += 1;
            let mut p = String::new();
            while i < chars.len() && chars[i].is_ascii_digit() {
                p.push(chars[i]);
                i += 1;
            }
            prec = p.parse().ok();
        }
        let Some(&conv) = chars.get(i) else {
            return bad(vm, "trailing %");
        };
        i += 1;
        let width: usize = width.parse().unwrap_or(0);
        if conv == '%' {
            out.push_str(&pad("%".into(), width, &flags));
            continue;
        }
        if conv == 'n' {
            out.push('\n');
            continue;
        }
        let ai = explicit.unwrap_or_else(|| {
            let n = next_arg;
            next_arg += 1;
            n
        });
        let Some(&arg) = argv.get(ai) else {
            return vm.throw_new(
                "Ljava/util/MissingFormatArgumentException;",
                &format!("Format specifier '%{conv}'"),
            );
        };
        let body = match conv {
            'd' => match unbox(vm, arg) {
                Some(Num::I(v, _)) => {
                    let mut s = v.to_string();
                    if flags.contains(',') {
                        s = text::group(&s);
                    }
                    if flags.contains('+') && v >= 0 {
                        s.insert(0, '+');
                    } else if flags.contains(' ') && v >= 0 {
                        s.insert(0, ' ');
                    }
                    s
                }
                _ if arg == 0 => "null".into(),
                _ => {
                    return vm.throw_new(
                        "Ljava/util/IllegalFormatConversionException;",
                        &format!("d != {}", class_name(vm, arg)),
                    )
                }
            },
            'x' | 'X' | 'o' => match unbox(vm, arg) {
                Some(Num::I(v, bits)) => {
                    let u = if bits == 64 {
                        v as u64
                    } else {
                        (v as u64) & ((1u64 << bits) - 1)
                    };
                    let s = if conv == 'o' {
                        format!("{u:o}")
                    } else {
                        format!("{u:x}")
                    };
                    if conv == 'X' {
                        s.to_uppercase()
                    } else {
                        s
                    }
                }
                _ if arg == 0 => "null".into(),
                _ => return bad(vm, "bad %x argument"),
            },
            'f' | 'e' | 'E' | 'g' | 'G' => match unbox(vm, arg) {
                Some(Num::F(v)) => {
                    let p = prec.unwrap_or(6);
                    let mut s = match conv {
                        'f' => text::fixed(v, p),
                        'e' | 'E' => text::sci(v, p, conv == 'E'),
                        _ => {
                            let a = v.abs();
                            if a != 0.0 && !(1e-4..10f64.powi(p.max(1) as i32)).contains(&a) {
                                text::sci(v, p.saturating_sub(1), conv == 'G')
                            } else {
                                let digits = if a == 0.0 {
                                    0
                                } else {
                                    a.log10().floor() as i32 + 1
                                };
                                text::fixed(v, (p as i32 - digits).max(0) as usize)
                            }
                        }
                    };
                    if flags.contains(',') {
                        s = text::group(&s);
                    }
                    if flags.contains('+') && v >= 0.0 {
                        s.insert(0, '+');
                    }
                    s
                }
                _ if arg == 0 => "null".into(),
                _ => {
                    return vm.throw_new(
                        "Ljava/util/IllegalFormatConversionException;",
                        &format!("{conv} != {}", class_name(vm, arg)),
                    )
                }
            },
            'c' | 'C' => match unbox(vm, arg) {
                Some(Num::C(c)) => char::from_u32(c).unwrap_or('?').to_string(),
                Some(Num::I(v, _)) => char::from_u32(v as u32).unwrap_or('?').to_string(),
                _ => "null".into(),
            },
            'b' | 'B' => match unbox(vm, arg) {
                Some(Num::Z(z)) => z.to_string(),
                _ => (arg != 0).to_string(),
            },
            'h' | 'H' => {
                if arg == 0 {
                    "null".into()
                } else {
                    format!("{:x}", identity_hash(arg))
                }
            }
            's' | 'S' => {
                let mut s = vm.call_to_string(arg)?;
                if let Some(p) = prec {
                    s = s.chars().take(p).collect();
                }
                if conv == 'S' {
                    s.to_uppercase()
                } else {
                    s
                }
            }
            _ => {
                return vm.throw_new(
                    "Ljava/util/UnknownFormatConversionException;",
                    &format!("Conversion = '{conv}'"),
                )
            }
        };
        out.push_str(&pad(body, width, &flags));
    }
    Ok(out)
}

fn class_name(vm: &Vm, r: Ref) -> String {
    vm.heap
        .get(r)
        .map(|o| vm.class(o.class).java_name())
        .unwrap_or_default()
}

fn pad(s: String, width: usize, flags: &str) -> String {
    let n = s.chars().count();
    if n >= width {
        return s;
    }
    let fill = width - n;
    if flags.contains('-') {
        format!("{s}{}", " ".repeat(fill))
    } else if flags.contains('0') {
        let (sign, rest) = if s.starts_with(['-', '+']) {
            s.split_at(1)
        } else {
            ("", s.as_str())
        };
        format!("{sign}{}{rest}", "0".repeat(fill))
    } else {
        format!("{}{s}", " ".repeat(fill))
    }
}
