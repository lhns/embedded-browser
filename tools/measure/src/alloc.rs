//! Counting allocator with per-category attribution.
//!
//! Attribution is by *phase*: the caller sets the current [`Cat`] around each
//! phase of work, and every allocation made meanwhile is tagged with it. The
//! tag lives in a small header in front of the block, so a free is charged
//! back to the category that allocated it, whichever phase frees it. A
//! `realloc` moves the block's bytes to the current phase's category.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering::Relaxed};

/// Heap categories reported by the tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cat {
    /// Runtime, CLI, page source bytes: anything not tagged otherwise.
    Other = 0,
    /// HTML parse, DOM, stylesheets, Stylo, Taffy layout and Parley text layout.
    Dom = 1,
    /// Font collection and font data (the font bytes themselves are `'static`).
    Text = 2,
    /// Decoded images.
    Images = 3,
    /// Paint: scene building, rasterizer state, glyph outlines and the pixel buffer.
    Raster = 4,
}

pub const CATS: [Cat; 5] = [Cat::Other, Cat::Dom, Cat::Text, Cat::Images, Cat::Raster];
const N: usize = CATS.len();

impl Cat {
    pub fn name(self) -> &'static str {
        match self {
            Cat::Other => "other",
            Cat::Dom => "dom+style+layout",
            Cat::Text => "text/fonts",
            Cat::Images => "images",
            Cat::Raster => "raster",
        }
    }
}

static PHASE: AtomicU8 = AtomicU8::new(0);
static LIVE: [AtomicIsize; N] = [const { AtomicIsize::new(0) }; N];
static TOTAL: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
/// Per-category live bytes at the moment `PEAK` was last raised.
static AT_PEAK: [AtomicIsize; N] = [const { AtomicIsize::new(0) }; N];
/// Live block count, and its value at the peak (for estimating allocator overhead).
static BLOCKS: AtomicIsize = AtomicIsize::new(0);
static BLOCKS_AT_PEAK: AtomicIsize = AtomicIsize::new(0);
/// Allocations that would take the live total past this fail (return null), as on a
/// device whose heap is full. `isize::MAX` means no cap.
static CAP: AtomicIsize = AtomicIsize::new(isize::MAX);

/// Make allocations beyond `bytes` of live heap fail.
pub fn set_cap(bytes: isize) {
    CAP.store(bytes, Relaxed);
}

fn over_cap(extra: usize) -> bool {
    TOTAL.load(Relaxed).saturating_add(extra as isize) > CAP.load(Relaxed)
}

/// Header size in front of each block: enough for the tag and the block's alignment.
fn header(align: usize) -> usize {
    align.max(16)
}

pub struct Counting;

impl Counting {
    fn charge(cat: u8, delta: isize) {
        LIVE[cat as usize].fetch_add(delta, Relaxed);
        let total = TOTAL.fetch_add(delta, Relaxed) + delta;
        // Single engine thread: a plain compare is good enough for the peak snapshot.
        if total > PEAK.load(Relaxed) {
            PEAK.store(total, Relaxed);
            BLOCKS_AT_PEAK.store(BLOCKS.load(Relaxed), Relaxed);
            for (i, live) in LIVE.iter().enumerate() {
                AT_PEAK[i].store(live.load(Relaxed), Relaxed);
            }
        }
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if over_cap(layout.size()) {
            return std::ptr::null_mut();
        }
        let h = header(layout.align());
        let Ok(outer) = Layout::from_size_align(layout.size() + h, layout.align().max(16)) else {
            return std::ptr::null_mut();
        };
        // SAFETY: outer has non-zero size.
        let base = unsafe { System.alloc(outer) };
        if base.is_null() {
            return base;
        }
        let cat = PHASE.load(Relaxed);
        // SAFETY: base..base+h is ours; the tag sits in the byte just before the user block.
        unsafe {
            let user = base.add(h);
            *user.sub(1) = cat;
            BLOCKS.fetch_add(1, Relaxed);
            Self::charge(cat, layout.size() as isize);
            user
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let h = header(layout.align());
        // SAFETY: ptr came from `alloc`/`realloc` above with the same layout.
        unsafe {
            let cat = *ptr.sub(1);
            BLOCKS.fetch_sub(1, Relaxed);
            Self::charge(cat, -(layout.size() as isize));
            let outer =
                Layout::from_size_align_unchecked(layout.size() + h, layout.align().max(16));
            System.dealloc(ptr.sub(h), outer);
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() && over_cap(new_size - layout.size()) {
            return std::ptr::null_mut();
        }
        let h = header(layout.align());
        // SAFETY: as in dealloc; the header is moved along with the block by System.realloc.
        unsafe {
            let old_cat = *ptr.sub(1);
            let outer =
                Layout::from_size_align_unchecked(layout.size() + h, layout.align().max(16));
            let base = System.realloc(ptr.sub(h), outer, new_size + h);
            if base.is_null() {
                return base;
            }
            let user = base.add(h);
            let cat = PHASE.load(Relaxed);
            *user.sub(1) = cat;
            Self::charge(old_cat, -(layout.size() as isize));
            Self::charge(cat, new_size as isize);
            user
        }
    }
}

/// Set the current phase; returns the previous one.
pub fn set_phase(cat: Cat) -> Cat {
    CATS[PHASE.swap(cat as u8, Relaxed) as usize]
}

/// Run `f` with allocations tagged as `cat`.
pub fn in_phase<R>(cat: Cat, f: impl FnOnce() -> R) -> R {
    let prev = set_phase(cat);
    let r = f();
    set_phase(prev);
    r
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub live: [isize; N],
    pub total: isize,
    pub peak: isize,
    pub at_peak: [isize; N],
    pub blocks_at_peak: isize,
}

pub fn snapshot() -> Snapshot {
    let mut s = Snapshot {
        total: TOTAL.load(Relaxed),
        peak: PEAK.load(Relaxed),
        blocks_at_peak: BLOCKS_AT_PEAK.load(Relaxed),
        ..Default::default()
    };
    for i in 0..N {
        s.live[i] = LIVE[i].load(Relaxed);
        s.at_peak[i] = AT_PEAK[i].load(Relaxed);
    }
    s
}

/// Restart peak tracking from the current live total.
pub fn reset_peak() {
    PEAK.store(TOTAL.load(Relaxed), Relaxed);
    BLOCKS_AT_PEAK.store(BLOCKS.load(Relaxed), Relaxed);
    for i in 0..N {
        AT_PEAK[i].store(LIVE[i].load(Relaxed), Relaxed);
    }
}
