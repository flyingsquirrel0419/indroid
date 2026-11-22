//! Guest address spaces: VMAs, lazily populated 4 KiB pages, copy-on-write
//! fork, and a page cache that shares read-only file pages between processes.

use crate::vfs::FileRef;
use indroid_a64::{Access, AddressSpace, PAGE_SIZE};
use parking_lot::{Mutex, RwLock};
use std::cell::UnsafeCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

pub const PROT_READ: u32 = 1;
pub const PROT_WRITE: u32 = 2;
pub const PROT_EXEC: u32 = 4;

pub const PAGE: u64 = PAGE_SIZE;
pub const PAGE_MASK: u64 = PAGE - 1;

#[inline]
pub fn page_up(x: u64) -> u64 {
    (x + PAGE_MASK) & !PAGE_MASK
}
#[inline]
pub fn page_down(x: u64) -> u64 {
    x & !PAGE_MASK
}

/// One 4 KiB page of guest memory.
pub struct Page(PageStorage);
enum PageStorage {
    Owned(Box<UnsafeCell<[u8; 4096]>>),
    // The owner keeps the renderer mapping alive until the last guest page
    // translation disappears, including translations surviving munmap/fork.
    External {
        ptr: *mut u8,
        _owner: Arc<dyn Send + Sync>,
    },
}
// Guest memory is plain bytes; concurrent access is the guest's business.
unsafe impl Sync for Page {}
unsafe impl Send for Page {}

impl Page {
    pub fn zeroed() -> Arc<Page> {
        Arc::new(Page(PageStorage::Owned(Box::new(UnsafeCell::new(
            [0; 4096],
        )))))
    }
    pub fn copy_of(other: &Page) -> Arc<Page> {
        let p = Page::zeroed();
        p.bytes_mut().copy_from_slice(other.bytes());
        p
    }
    #[inline]
    pub fn ptr(&self) -> *mut u8 {
        match &self.0 {
            PageStorage::Owned(data) => data.get().cast(),
            PageStorage::External { ptr, .. } => *ptr,
        }
    }
    pub fn bytes(&self) -> &[u8; 4096] {
        unsafe { &*self.ptr().cast::<[u8; 4096]>() }
    }
    #[allow(clippy::mut_from_ref)]
    pub fn bytes_mut(&self) -> &mut [u8; 4096] {
        unsafe { &mut *self.ptr().cast::<[u8; 4096]>() }
    }
}

/// Shared, page-granular memory object (MAP_SHARED anonymous, ashmem, memfd,
/// shared file mappings of writable files).
pub struct SharedMem {
    pub pages: Mutex<Vec<Option<Arc<Page>>>>,
    fixed: bool,
}

impl SharedMem {
    pub fn new(len: u64) -> Arc<SharedMem> {
        Arc::new(SharedMem {
            pages: Mutex::new(vec![None; (page_up(len) / PAGE) as usize]),
            fixed: false,
        })
    }
    /// Alias renderer-owned writable memory. The owner must retain the mapping
    /// and keep it accessible until every cloned page has been dropped.
    ///
    /// # Safety
    /// `ptr..ptr+len` must remain valid, writable, and exclusively managed as
    /// guest shared memory for the lifetime of `owner`. The mapping must be a
    /// whole number of guest pages, and `ptr` must be non-null.
    pub unsafe fn from_external(
        ptr: *mut u8,
        len: u64,
        owner: Arc<dyn Send + Sync>,
    ) -> Option<Arc<Self>> {
        if ptr.is_null() || len == 0 || len & PAGE_MASK != 0 || len > isize::MAX as u64 {
            return None;
        }
        let mut pages = Vec::with_capacity((len / PAGE) as usize);
        for i in 0..len / PAGE {
            pages.push(Some(Arc::new(Page(PageStorage::External {
                ptr: ptr.add((i * PAGE) as usize),
                _owner: owner.clone(),
            }))));
        }
        Some(Arc::new(Self {
            pages: Mutex::new(pages),
            fixed: true,
        }))
    }
    pub fn page(&self, idx: u64) -> Option<Arc<Page>> {
        let mut v = self.pages.lock();
        let i = idx as usize;
        if i >= v.len() {
            if self.fixed {
                return None;
            }
            // ashmem/memfd may grow (ftruncate) after mapping; be lenient.
            v.resize(i + 1, None);
        }
        Some(v[i].get_or_insert_with(Page::zeroed).clone())
    }
    pub fn resize(&self, len: u64) {
        if self.fixed {
            return;
        }
        self.pages
            .lock()
            .resize((page_up(len) / PAGE) as usize, None);
    }
}

#[derive(Clone)]
pub enum Backing {
    /// Private anonymous memory (zero-filled on first touch).
    Anon,
    /// Private file mapping: pages come from the page cache, copied on write.
    File { file: FileRef, offset: u64 },
    /// Shared memory object; `offset` in bytes into it.
    Shared { mem: Arc<SharedMem>, offset: u64 },
}

#[derive(Clone)]
pub struct Vma {
    pub start: u64,
    pub end: u64,
    pub prot: u32,
    pub backing: Backing,
    /// For /proc/self/maps.
    pub name: Arc<str>,
}

#[derive(Clone)]
struct Slot {
    page: Arc<Page>,
    /// Private page possibly shared with others (page cache / fork): must be
    /// copied before the first write.
    cow: bool,
}

#[derive(Clone, Default)]
struct Inner {
    vmas: BTreeMap<u64, Vma>,
    pages: HashMap<u64, Slot>,
}

pub struct Mm {
    inner: RwLock<Inner>,
    generation: AtomicU64,
    pub brk: Mutex<(u64, u64)>, // (start, current)
    pub mmap_base: u64,
    /// Serializes "pick a free range, then map it" so two threads mmapping
    /// at once cannot receive (and then clobber) the same range.
    pub map_lock: Mutex<()>,
}

/// Global cache of read-only file pages, keyed by (file id, page index).
#[derive(Default)]
pub struct PageCache {
    map: Mutex<HashMap<(u64, u64), Weak<Page>>>,
}

pub static PAGE_CACHE: std::sync::LazyLock<PageCache> =
    std::sync::LazyLock::new(PageCache::default);

impl PageCache {
    fn get(&self, file: &FileRef, idx: u64) -> Arc<Page> {
        let key = (file.id(), idx);
        if let Some(p) = self.map.lock().get(&key).and_then(|w| w.upgrade()) {
            return p;
        }
        let p = Page::zeroed();
        file.read_page(idx * PAGE, p.bytes_mut());
        let mut m = self.map.lock();
        if m.len() > 1 << 16 {
            m.retain(|_, w| w.strong_count() > 0);
        }
        m.insert(key, Arc::downgrade(&p));
        p
    }
}

impl Default for Mm {
    fn default() -> Self {
        Self::new()
    }
}

impl Mm {
    pub fn new() -> Mm {
        Mm {
            inner: RwLock::new(Inner::default()),
            generation: AtomicU64::new(1),
            brk: Mutex::new((0, 0)),
            mmap_base: 0x7f_0000_0000,
            map_lock: Mutex::new(()),
        }
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// fork(): a copy-on-write duplicate. Bumps our generation so that every
    /// thread's TLB drops write translations of now-shared pages.
    pub fn fork(&self) -> Mm {
        let mut inner = self.inner.write();
        for (_, slot) in inner.pages.iter_mut() {
            slot.cow = true;
        }
        // Shared mappings must stay shared: their slots are re-derived from
        // the SharedMem on demand, so drop them from both tables.
        let shared: Vec<(u64, u64)> = inner
            .vmas
            .values()
            .filter(|v| matches!(v.backing, Backing::Shared { .. }))
            .map(|v| (v.start, v.end))
            .collect();
        for (s, e) in shared {
            let mut p = s;
            while p < e {
                if let Some(slot) = inner.pages.get_mut(&(p / PAGE)) {
                    slot.cow = false;
                }
                p += PAGE;
            }
        }
        let copy = inner.clone();
        drop(inner);
        self.bump();
        Mm {
            inner: RwLock::new(copy),
            generation: AtomicU64::new(1),
            brk: Mutex::new(*self.brk.lock()),
            mmap_base: self.mmap_base,
            map_lock: Mutex::new(()),
        }
    }

    /// Finds a free range of `len` bytes, preferring `hint`.
    pub fn find_free(&self, hint: u64, len: u64) -> Option<u64> {
        let inner = self.inner.read();
        let free = |a: u64| -> bool {
            if a == 0 || a.checked_add(len).is_none_or(|e| e > 1 << 47) {
                return false;
            }
            // First VMA starting before a+len.
            match inner.vmas.range(..a + len).next_back() {
                Some((_, v)) => v.end <= a,
                None => true,
            }
        };
        if hint != 0 && free(page_down(hint)) {
            return Some(page_down(hint));
        }
        // Top-down search below mmap_base.
        let mut top = self.mmap_base;
        for (_, v) in inner.vmas.range(..self.mmap_base).rev() {
            if v.end <= top && top - v.end >= len {
                return Some(top - len);
            }
            top = top.min(v.start);
        }
        if top >= len + 0x1000_0000 {
            return Some(top - len);
        }
        None
    }

    /// Maps [start, start+len) replacing anything there (MAP_FIXED semantics).
    pub fn map(&self, start: u64, len: u64, prot: u32, backing: Backing, name: Arc<str>) {
        let len = page_up(len);
        let mut inner = self.inner.write();
        Self::unmap_locked(&mut inner, start, len);
        inner.vmas.insert(
            start,
            Vma {
                start,
                end: start + len,
                prot,
                backing,
                name,
            },
        );
        drop(inner);
        self.bump();
    }

    fn unmap_locked(inner: &mut Inner, start: u64, len: u64) {
        let end = start + len;
        // Split/trim overlapping VMAs.
        let overlapping: Vec<u64> = inner
            .vmas
            .range(..end)
            .filter(|(_, v)| v.end > start)
            .map(|(k, _)| *k)
            .collect();
        for k in overlapping {
            let v = inner.vmas.remove(&k).unwrap();
            if v.start < start {
                let mut left = v.clone();
                left.end = start;
                inner.vmas.insert(left.start, left);
            }
            if v.end > end {
                let mut right = v.clone();
                let delta = end - v.start;
                right.start = end;
                right.backing = advance(&v.backing, delta);
                inner.vmas.insert(right.start, right);
            }
        }
        if len / PAGE < inner.pages.len() as u64 {
            let mut p = start;
            while p < end {
                inner.pages.remove(&(p / PAGE));
                p += PAGE;
            }
        } else {
            inner.pages.retain(|k, _| {
                let a = k * PAGE;
                a < start || a >= end
            });
        }
    }

    pub fn unmap(&self, start: u64, len: u64) {
        let mut inner = self.inner.write();
        Self::unmap_locked(&mut inner, start, page_up(len));
        drop(inner);
        self.bump();
    }

    /// mprotect. Returns false if part of the range is unmapped (ENOMEM).
    pub fn protect(&self, start: u64, len: u64, prot: u32) -> bool {
        let end = start + page_up(len);
        let mut inner = self.inner.write();
        // Check coverage.
        let mut cur = start;
        for (_, v) in inner.vmas.range(..end) {
            if v.end <= cur {
                continue;
            }
            if v.start > cur {
                return false;
            }
            cur = v.end;
            if cur >= end {
                break;
            }
        }
        if cur < end {
            return false;
        }
        // Split at boundaries then set prot.
        split_at(&mut inner, start);
        split_at(&mut inner, end);
        let keys: Vec<u64> = inner.vmas.range(start..end).map(|(k, _)| *k).collect();
        for k in keys {
            inner.vmas.get_mut(&k).unwrap().prot = prot;
        }
        if prot & PROT_WRITE != 0 {
            // Un-share copy-on-write pages now (see page_for).
            let mut p = start;
            while p < end {
                if let Some(slot) = inner.pages.get_mut(&(p / PAGE)) {
                    if slot.cow {
                        if Arc::strong_count(&slot.page) > 1 {
                            slot.page = Page::copy_of(&slot.page);
                        }
                        slot.cow = false;
                    }
                }
                p += PAGE;
            }
        }
        drop(inner);
        self.bump();
        true
    }

    pub fn vmas(&self) -> Vec<Vma> {
        self.inner.read().vmas.values().cloned().collect()
    }

    pub fn vma_at(&self, addr: u64) -> Option<Vma> {
        let inner = self.inner.read();
        inner
            .vmas
            .range(..=addr)
            .next_back()
            .filter(|(_, v)| v.end > addr)
            .map(|(_, v)| v.clone())
    }

    /// mremap (simple: grow in place if free, else move when allowed).
    pub fn remap(
        &self,
        old: u64,
        old_len: u64,
        new_len: u64,
        may_move: bool,
        fixed: Option<u64>,
    ) -> Result<u64, i64> {
        let _map_guard = self.map_lock.lock();
        let old_len = page_up(old_len);
        let new_len = page_up(new_len);
        let Some(v) = self.vma_at(old) else {
            return Err(-14);
        };
        if new_len <= old_len && fixed.is_none() {
            if new_len < old_len {
                self.unmap(old + new_len, old_len - new_len);
            }
            return Ok(old);
        }
        let grow_ok = fixed.is_none() && {
            let inner = self.inner.read();
            let e = old + new_len;
            inner.vmas.range(old + old_len..e).next().is_none() && v.end == old + old_len
        };
        if grow_ok {
            let mut inner = self.inner.write();
            if let Some(vm) = inner.vmas.get_mut(&v.start) {
                vm.end = old + new_len;
            }
            drop(inner);
            self.bump();
            return Ok(old);
        }
        if !may_move {
            return Err(-12);
        }
        let dst = match fixed {
            Some(f) => f,
            None => self.find_free(0, new_len).ok_or(-12i64)?,
        };
        // Move VMA + pages.
        let mut inner = self.inner.write();
        let mut moved: Vec<(u64, Slot)> = Vec::new();
        let mut p = old;
        while p < old + old_len {
            if let Some(s) = inner.pages.remove(&(p / PAGE)) {
                moved.push((p - old, s));
            }
            p += PAGE;
        }
        let backing = advance(&v.backing, old - v.start);
        Self::unmap_locked(&mut inner, old, old_len);
        Self::unmap_locked(&mut inner, dst, new_len);
        inner.vmas.insert(
            dst,
            Vma {
                start: dst,
                end: dst + new_len,
                prot: v.prot,
                backing,
                name: v.name.clone(),
            },
        );
        for (off, s) in moved {
            inner.pages.insert((dst + off) / PAGE, s);
        }
        drop(inner);
        self.bump();
        Ok(dst)
    }

    /// madvise(MADV_DONTNEED): drop private pages (they read back as zero /
    /// file contents).
    pub fn discard(&self, start: u64, len: u64) {
        let mut inner = self.inner.write();
        let mut p = page_down(start);
        while p < start + len {
            inner.pages.remove(&(p / PAGE));
            p += PAGE;
        }
        drop(inner);
        self.bump();
    }

    /// Resolves (populating if necessary) the page holding `addr` for the
    /// kernel's own copies (ignores PROT bits except for existence).
    fn page_for(&self, page: u64, write: bool, check: Option<Access>) -> Option<*mut u8> {
        {
            let inner = self.inner.read();
            if let Some(slot) = inner.pages.get(&page) {
                if let Some(acc) = check {
                    let v = inner.vmas.range(..=page * PAGE).next_back()?.1;
                    if !allowed(v.prot, acc) {
                        return None;
                    }
                }
                if !write || !slot.cow {
                    return Some(slot.page.ptr());
                }
            }
        }
        let mut inner = self.inner.write();
        let addr = page * PAGE;
        let v = inner
            .vmas
            .range(..=addr)
            .next_back()
            .filter(|(_, v)| v.end > addr)?
            .1
            .clone();
        if let Some(acc) = check {
            if !allowed(v.prot, acc) {
                return None;
            }
        }
        if let Some(slot) = inner.pages.get_mut(&page) {
            if write && slot.cow {
                let mut copied = false;
                if Arc::strong_count(&slot.page) > 1 {
                    slot.page = Page::copy_of(&slot.page);
                    copied = true;
                }
                slot.cow = false;
                let ptr = slot.page.ptr();
                drop(inner);
                if copied {
                    // Read TLBs may still point at the old shared page.
                    self.bump();
                }
                return Some(ptr);
            }
            return Some(slot.page.ptr());
        }
        let off = addr - v.start;
        let slot = match &v.backing {
            Backing::Anon => Slot {
                page: Page::zeroed(),
                cow: false,
            },
            Backing::File { file, offset } => {
                let src = PAGE_CACHE.get(file, (offset + off) / PAGE);
                // Writable private mappings get their own copy on first touch
                // (even a read), so that per-thread TLBs never hold a pointer
                // to a page that a later write would replace.
                if write || v.prot & PROT_WRITE != 0 {
                    Slot {
                        page: Page::copy_of(&src),
                        cow: false,
                    }
                } else {
                    Slot {
                        page: src,
                        cow: true,
                    }
                }
            }
            Backing::Shared { mem, offset } => Slot {
                page: mem.page((offset + off) / PAGE)?,
                cow: false,
            },
        };
        let ptr = slot.page.ptr();
        inner.pages.insert(page, slot);
        Some(ptr)
    }

    // ---- kernel-side copies (used by syscalls) ----

    pub fn read(&self, addr: u64, buf: &mut [u8]) -> bool {
        let addr = untag(addr);
        let mut done = 0;
        while done < buf.len() {
            let a = addr.wrapping_add(done as u64);
            let off = (a & PAGE_MASK) as usize;
            let n = (PAGE as usize - off).min(buf.len() - done);
            let Some(p) = self.page_for(a / PAGE, false, Some(Access::Read)) else {
                return false;
            };
            unsafe { std::ptr::copy_nonoverlapping(p.add(off), buf[done..].as_mut_ptr(), n) };
            done += n;
        }
        true
    }

    pub fn write(&self, addr: u64, buf: &[u8]) -> bool {
        self.write_inner(addr, buf, true)
    }

    /// Writes ignoring page protection (loader use).
    pub fn poke(&self, addr: u64, buf: &[u8]) -> bool {
        self.write_inner(addr, buf, false)
    }

    fn write_inner(&self, addr: u64, buf: &[u8], check: bool) -> bool {
        let addr = untag(addr);
        let mut done = 0;
        while done < buf.len() {
            let a = addr.wrapping_add(done as u64);
            let off = (a & PAGE_MASK) as usize;
            let n = (PAGE as usize - off).min(buf.len() - done);
            let Some(p) = self.page_for(a / PAGE, true, check.then_some(Access::Write)) else {
                return false;
            };
            unsafe { std::ptr::copy_nonoverlapping(buf[done..].as_ptr(), p.add(off), n) };
            done += n;
        }
        true
    }

    pub fn read_u64(&self, addr: u64) -> Option<u64> {
        let mut b = [0; 8];
        self.read(addr, &mut b).then(|| u64::from_le_bytes(b))
    }
    pub fn read_u32(&self, addr: u64) -> Option<u32> {
        let mut b = [0; 4];
        self.read(addr, &mut b).then(|| u32::from_le_bytes(b))
    }
    pub fn write_u64(&self, addr: u64, v: u64) -> bool {
        self.write(addr, &v.to_le_bytes())
    }
    pub fn write_u32(&self, addr: u64, v: u32) -> bool {
        self.write(addr, &v.to_le_bytes())
    }

    /// Reads a NUL-terminated string (at most `max` bytes).
    pub fn read_cstr(&self, addr: u64, max: usize) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let mut a = untag(addr);
        loop {
            let off = (a & PAGE_MASK) as usize;
            let p = self.page_for(a / PAGE, false, Some(Access::Read))?;
            let chunk = unsafe { std::slice::from_raw_parts(p.add(off), PAGE as usize - off) };
            if let Some(i) = chunk.iter().position(|&b| b == 0) {
                out.extend_from_slice(&chunk[..i]);
                return Some(out);
            }
            out.extend_from_slice(chunk);
            if out.len() > max {
                return None;
            }
            a += chunk.len() as u64;
        }
    }

    /// Host pointer to a guest u32 for futex operations (must be aligned and
    /// writable or readable).
    pub fn futex_ptr(&self, addr: u64) -> Option<*mut u32> {
        let addr = untag(addr);
        if addr & 3 != 0 {
            return None;
        }
        let p = self.page_for(addr / PAGE, false, Some(Access::Read))?;
        Some(unsafe { p.add((addr & PAGE_MASK) as usize) } as *mut u32)
    }

    /// Identity of the memory backing `addr`, for process-shared futexes:
    /// (Some(page pointer), offset) when the page is shared memory.
    pub fn shared_key(&self, addr: u64) -> Option<usize> {
        let addr = untag(addr);
        let v = self.vma_at(addr)?;
        if matches!(v.backing, Backing::Shared { .. }) {
            let p = self.page_for(addr / PAGE, false, None)?;
            Some(p as usize + (addr & PAGE_MASK) as usize)
        } else {
            None
        }
    }
}

/// Strips the AArch64 top-byte tag (TBI) from a user pointer.
#[inline]
pub fn untag(addr: u64) -> u64 {
    addr & 0x00ff_ffff_ffff_ffff
}

fn allowed(prot: u32, acc: Access) -> bool {
    match acc {
        Access::Read => prot & (PROT_READ | PROT_WRITE | PROT_EXEC) != 0,
        Access::Write => prot & PROT_WRITE != 0,
        Access::Exec => prot & PROT_EXEC != 0,
    }
}

fn advance(b: &Backing, delta: u64) -> Backing {
    match b {
        Backing::Anon => Backing::Anon,
        Backing::File { file, offset } => Backing::File {
            file: file.clone(),
            offset: offset + delta,
        },
        Backing::Shared { mem, offset } => Backing::Shared {
            mem: mem.clone(),
            offset: offset + delta,
        },
    }
}

fn split_at(inner: &mut Inner, at: u64) {
    let Some((&k, v)) = inner.vmas.range(..at).next_back() else {
        return;
    };
    if v.end <= at || v.start >= at {
        return;
    }
    let mut right = v.clone();
    let left_end = at;
    right.backing = advance(&v.backing, at - v.start);
    right.start = at;
    inner.vmas.get_mut(&k).unwrap().end = left_end;
    inner.vmas.insert(at, right);
}

impl AddressSpace for Mm {
    #[inline]
    fn translate(&self, page: u64, access: Access) -> Option<*mut u8> {
        self.page_for(page, access == Access::Write, Some(access))
    }
    fn aot_unit(&self, page: u64) -> Option<indroid_a64::mem::AotUnit> {
        if !indroid_aot::HAS_UNITS {
            return None;
        }
        // AOT is experimental: opt in until it reliably beats the block cache.
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*ENABLED.get_or_init(|| std::env::var_os("INDROID_ENABLE_AOT").is_some()) {
            return None;
        }
        let addr = page.checked_mul(PAGE)?;
        let vma = self.vma_at(addr)?;
        if vma.prot & PROT_EXEC == 0 || vma.prot & PROT_WRITE != 0 {
            return None;
        }
        if let Backing::File { offset, .. } = vma.backing {
            let file_offset = offset.checked_add(addr.checked_sub(vma.start)?)?;
            // Optional filter makes it possible to measure individual units.
            static ONLY: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
            if let Some(selected) = *ONLY.get_or_init(|| {
                std::env::var("INDROID_AOT_ONLY_OFFSET")
                    .ok()
                    .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            }) {
                if selected != file_offset {
                    return None;
                }
            }
            indroid_aot::lookup(&vma.name, file_offset)
        } else {
            None
        }
    }
    #[inline]
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod external_page_tests {
    use super::*;
    struct Allocation(Box<[u8; 8192]>);
    #[test]
    fn pages_alias_external_memory_and_retain_owner() {
        let mut allocation = Arc::new(Allocation(Box::new([0; 8192])));
        let ptr = Arc::get_mut(&mut allocation).unwrap().0.as_mut_ptr();
        let weak = Arc::downgrade(&allocation);
        let owner: Arc<dyn Send + Sync> = allocation;
        let memory = unsafe { SharedMem::from_external(ptr, 8192, owner).unwrap() };
        let second = memory.page(1).unwrap();
        second.bytes_mut()[7] = 0x5a;
        assert_eq!(unsafe { *ptr.add(4103) }, 0x5a);
        assert!(memory.page(2).is_none());
        drop(memory);
        assert!(weak.upgrade().is_some());
        drop(second);
        assert!(weak.upgrade().is_none());
    }
}
