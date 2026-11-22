//! Turns an Android arm64 system image zip (as published for the SDK
//! emulator) into the directory tree indroid neo boots from, without any
//! external tools, so it can run on the device (iOS) itself:
//!
//! zip → system.img (GPT) → super partition → ext4 partitions (system,
//! system_ext, product, vendor) → files on disk, plus activated APEX payloads,
//! apex-info-list.xml and the SELinux labels of every file
//! (`.indroid-labels`, read by crates/kernel/src/selinux.rs).

pub mod ext4;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub type Progress<'a> = &'a mut dyn FnMut(&str);

/// Installs the ROM from `zip_path` into `out` using `work` for temporary
/// files (needs ~1.5 GB for the tree plus the sparse system image).
pub fn install(zip_path: &Path, out: &Path, work: &Path, progress: Progress) -> io::Result<()> {
    fs::create_dir_all(work)?;
    let tmp_out = out.with_extension("partial");
    let _ = fs::remove_dir_all(&tmp_out);
    fs::create_dir_all(&tmp_out)?;
    let img = work.join("system.img");
    progress("extracting system image");
    extract_sparse(zip_path, "system.img", &img)?;
    let mut labels = BTreeMap::new();
    {
        let mut f = File::open(&img)?;
        let super_off = gpt_partition(&mut f, "super")?.unwrap_or(0);
        for (off, name) in scan_ext4(&mut f, super_off)? {
            let dir = match name.as_str() {
                "/" => tmp_out.clone(),
                "system" | "" => continue, // system_other (odex) or unnamed
                n => tmp_out.join(n.trim_start_matches('/')),
            };
            let prefix = if dir == tmp_out {
                String::new()
            } else {
                format!("/{}", name.trim_start_matches('/'))
            };
            progress(&format!(
                "unpacking {}",
                if prefix.is_empty() { "system" } else { &prefix }
            ));
            fs::create_dir_all(&dir)?;
            let mut fs4 = ext4::Ext4::open(File::open(&img)?, off)?;
            extract_tree(&mut fs4, &dir, &prefix, &mut labels)?;
        }
    }
    let _ = fs::remove_file(&img);
    activate_apexes(&tmp_out, work, &mut labels, progress)?;
    progress("writing metadata");
    write_apex_info_list(&tmp_out)?;
    let mut lf = BufWriter::new(File::create(tmp_out.join(".indroid-labels"))?);
    for (p, l) in &labels {
        writeln!(lf, "{p}\t{l}")?;
    }
    lf.flush()?;
    let _ = fs::remove_dir_all(out);
    fs::rename(&tmp_out, out)?;
    progress("done");
    Ok(())
}

/// Extracts the zip entry whose name ends with `suffix`, writing zero runs as
/// holes (APFS/ext4 keep them sparse).
fn extract_sparse(zip_path: &Path, suffix: &str, dest: &Path) -> io::Result<()> {
    let mut z = zip::ZipArchive::new(File::open(zip_path)?).map_err(io::Error::other)?;
    let name = (0..z.len())
        .filter_map(|i| z.by_index(i).ok().map(|e| e.name().to_string()))
        .find(|n| n.ends_with(suffix))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{suffix} not in zip")))?;
    let mut e = z.by_name(&name).map_err(io::Error::other)?;
    let mut out = File::create(dest)?;
    let mut buf = vec![0u8; 1 << 16];
    let mut pos = 0u64;
    loop {
        let mut filled = 0;
        while filled < buf.len() {
            let n = e.read(&mut buf[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        if buf[..filled].iter().any(|&b| b != 0) {
            out.seek(SeekFrom::Start(pos))?;
            out.write_all(&buf[..filled])?;
        }
        pos += filled as u64;
        if filled < buf.len() {
            break;
        }
    }
    out.set_len(pos)?;
    Ok(())
}

/// Byte offset of the GPT partition named `name`, if the image has a GPT.
fn gpt_partition(f: &mut File, name: &str) -> io::Result<Option<u64>> {
    let mut hdr = [0u8; 92];
    f.seek(SeekFrom::Start(512))?;
    f.read_exact(&mut hdr)?;
    if &hdr[..8] != b"EFI PART" {
        return Ok(None);
    }
    let lba = u64::from_le_bytes(hdr[72..80].try_into().unwrap());
    let count = u32::from_le_bytes(hdr[80..84].try_into().unwrap()) as u64;
    let size = u32::from_le_bytes(hdr[84..88].try_into().unwrap()) as u64;
    for i in 0..count.min(128) {
        let mut e = vec![0u8; size as usize];
        f.seek(SeekFrom::Start(lba * 512 + i * size))?;
        f.read_exact(&mut e)?;
        let first = u64::from_le_bytes(e[32..40].try_into().unwrap());
        let n: Vec<u16> = e[56..128]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        if String::from_utf16_lossy(&n) == name {
            return Ok(Some(first * 512));
        }
    }
    Ok(None)
}

/// ext4 file systems inside a super partition (1 MiB aligned, contiguous in
/// SDK images): (byte offset, mount point recorded in the superblock).
fn scan_ext4(f: &mut File, start: u64) -> io::Result<Vec<(u64, String)>> {
    let len = f.metadata()?.len();
    let mut out = Vec::new();
    let mut off = start;
    let mut sb = [0u8; 256];
    while off + 2048 < len {
        f.seek(SeekFrom::Start(off + 1024))?;
        f.read_exact(&mut sb)?;
        if sb[0x38] == 0x53 && sb[0x39] == 0xef {
            let blocks = u32::from_le_bytes(sb[4..8].try_into().unwrap()) as u64;
            let bs = 1024u64 << u32::from_le_bytes(sb[24..28].try_into().unwrap());
            let name: Vec<u8> = sb[0x78..0x88]
                .iter()
                .copied()
                .take_while(|&b| b != 0)
                .collect();
            let name = String::from_utf8_lossy(&name).into_owned();
            out.push((off, name));
            off += (blocks * bs).div_ceil(1 << 20) << 20;
            continue;
        }
        off += 1 << 20;
    }
    Ok(out)
}

fn selinux_label(fs4: &mut ext4::Ext4<File>, ino: &ext4::Inode) -> Option<String> {
    let x = fs4.xattrs(ino).ok()?;
    let v = x.get("security.selinux")?;
    Some(
        String::from_utf8_lossy(v)
            .trim_end_matches('\0')
            .to_string(),
    )
}

/// Writes the tree rooted at inode 2 into `dir`; `prefix` is the guest path
/// of `dir` ("" for /).
fn extract_tree(
    fs4: &mut ext4::Ext4<File>,
    dir: &Path,
    prefix: &str,
    labels: &mut BTreeMap<String, String>,
) -> io::Result<()> {
    let root = fs4.inode(2)?;
    if let Some(l) = selinux_label(fs4, &root) {
        labels.insert(
            if prefix.is_empty() {
                "/".into()
            } else {
                prefix.into()
            },
            l,
        );
    }
    let mut stack: Vec<(ext4::Inode, PathBuf, String)> =
        vec![(root, dir.to_path_buf(), prefix.to_string())];
    while let Some((ino, host, guest)) = stack.pop() {
        for (name, num) in fs4.read_dir(&ino)? {
            if host == dir && name == "lost+found" {
                continue;
            }
            let child = fs4.inode(num)?;
            let hp = host.join(&name);
            let gp = format!("{guest}/{name}");
            if let Some(l) = selinux_label(fs4, &child) {
                labels.insert(gp.clone(), l);
            }
            match child.mode & ext4::S_IFMT {
                ext4::S_IFDIR => {
                    fs::create_dir_all(&hp)?;
                    set_mode(&hp, (child.mode as u32 & 0o7777) | 0o700);
                    stack.push((child, hp, gp));
                }
                ext4::S_IFREG => {
                    let mut f = File::create(&hp)?;
                    fs4.copy_file(&child, &mut f)?;
                    set_mode(&hp, (child.mode as u32 & 0o7777) | 0o400);
                }
                ext4::S_IFLNK => {
                    let t = fs4.read_link(&child)?;
                    let _ = fs::remove_file(&hp);
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&t, &hp)?;
                }
                _ => {} // device nodes, fifos: not present in images
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(p, fs::Permissions::from_mode(mode));
}
#[cfg(not(unix))]
fn set_mode(_p: &Path, _mode: u32) {}

/// Field 1 (name) and 2 (version) of an apex_manifest.pb.
fn apex_manifest(b: &[u8]) -> (String, u64) {
    fn varint(b: &[u8], i: &mut usize) -> u64 {
        let (mut v, mut s) = (0u64, 0);
        while *i < b.len() {
            let c = b[*i];
            *i += 1;
            v |= ((c & 0x7f) as u64) << s;
            s += 7;
            if c < 0x80 {
                break;
            }
        }
        v
    }
    let (mut name, mut version) = (String::new(), 0);
    let mut i = 0;
    while i < b.len() {
        let key = varint(b, &mut i);
        match (key >> 3, key & 7) {
            (f, 0) => {
                let v = varint(b, &mut i);
                if f == 2 {
                    version = v;
                }
            }
            (f, 2) => {
                let n = varint(b, &mut i) as usize;
                if f == 1 && i + n <= b.len() {
                    name = String::from_utf8_lossy(&b[i..i + n]).into_owned();
                }
                i += n;
            }
            (_, 5) => i += 4,
            (_, 1) => i += 8,
            _ => break,
        }
    }
    (name, version)
}

fn activate_apexes(
    root: &Path,
    work: &Path,
    labels: &mut BTreeMap<String, String>,
    progress: Progress,
) -> io::Result<()> {
    let apex_root = root.join("apex");
    fs::create_dir_all(&apex_root)?;
    for dir in [
        "system/apex",
        "system_ext/apex",
        "vendor/apex",
        "product/apex",
    ] {
        let Ok(rd) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for f in files {
            let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "apex" && ext != "capex" {
                continue;
            }
            let payload = work.join("apex_payload.img");
            let manifest = {
                let mut z = zip::ZipArchive::new(File::open(&f)?).map_err(io::Error::other)?;
                if ext == "capex" {
                    let inner = work.join("original_apex");
                    io::copy(
                        &mut z.by_name("original_apex").map_err(io::Error::other)?,
                        &mut File::create(&inner)?,
                    )?;
                    z = zip::ZipArchive::new(File::open(&inner)?).map_err(io::Error::other)?;
                }
                io::copy(
                    &mut z.by_name("apex_payload.img").map_err(io::Error::other)?,
                    &mut File::create(&payload)?,
                )?;
                let mut m = Vec::new();
                z.by_name("apex_manifest.pb")
                    .map_err(io::Error::other)?
                    .read_to_end(&mut m)?;
                m
            };
            let (name, _) = apex_manifest(&manifest);
            if name.is_empty() {
                continue;
            }
            progress(&format!("activating {name}"));
            let dest = apex_root.join(&name);
            fs::create_dir_all(&dest)?;
            let mut fs4 = ext4::Ext4::open(File::open(&payload)?, 0)?;
            extract_tree(&mut fs4, &dest, &format!("/apex/{name}"), labels)?;
            let _ = fs::remove_file(&payload);
            let _ = fs::remove_file(work.join("original_apex"));
        }
    }
    Ok(())
}

/// What apexd would write to /apex/apex-info-list.xml.
fn write_apex_info_list(root: &Path) -> io::Result<()> {
    let apex = root.join("apex");
    let mut sources = Vec::new();
    for d in [
        "system/apex",
        "system_ext/apex",
        "vendor/apex",
        "product/apex",
    ] {
        if let Ok(rd) = fs::read_dir(root.join(d)) {
            for e in rd.flatten() {
                sources.push(format!("/{d}/{}", e.file_name().to_string_lossy()));
            }
        }
    }
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<apex-info-list>\n");
    let mut names: Vec<_> = fs::read_dir(&apex)?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let Ok(m) = fs::read(apex.join(&name).join("apex_manifest.pb")) else {
            continue;
        };
        let (_, version) = apex_manifest(&m);
        let path = sources
            .iter()
            .find(|p| {
                let base = p.rsplit('/').next().unwrap_or("");
                base.starts_with(&format!("{name}."))
                    || base.starts_with(&format!(
                        "{}.",
                        name.replace("com.android.", "com.google.android.")
                    ))
            })
            .cloned()
            .unwrap_or_else(|| format!("/system/apex/{name}.apex"));
        s.push_str(&format!(
            "    <apex-info moduleName=\"{name}\" modulePath=\"{path}\" preinstalledModulePath=\"{path}\" versionCode=\"{}\" versionName=\"\" isFactory=\"true\" isActive=\"true\" lastUpdateMillis=\"0\" provideSharedApexLibs=\"false\" />\n",
            version.max(1)
        ));
    }
    s.push_str("</apex-info-list>\n");
    fs::write(apex.join("apex-info-list.xml"), s)
}
