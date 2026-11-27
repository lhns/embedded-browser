//! M0 hello-Blitz firmware (gate H1): parse, style (sequential Stylo), lay out and paint one
//! embedded dashboard page at 1024x600 into an RGBA buffer in PSRAM, and print the time of each
//! phase, the heap high-water marks and the task count over serial. No display needed.
//!
//! The same code runs on the host (`--target x86_64-unknown-linux-gnu`) for a quick sanity check.

use std::time::Instant;

use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use parley::FontContext;
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 600;
/// The plan asks for >= 256 KB; Stylo's recursive traversal and Taffy get some headroom.
const ENGINE_STACK: usize = 512 * 1024;

const PAGE: &str = include_str!("dashboard.html");
#[cfg(not(feature = "measured-set"))]
const FONTS: [&[u8]; 2] = [
    include_bytes!("../assets/fonts/NotoSans-Regular-latin.ttf"),
    include_bytes!("../assets/fonts/NotoSans-Bold-latin.ttf"),
];
#[cfg(feature = "measured-set")]
const FONTS: [&[u8]; 2] = [ttf_noto_sans::REGULAR, ttf_noto_sans::BOLD];
// The image crate is linked (via blitz-dom) either way; this keeps the feature's codecs explicit.
#[cfg(feature = "measured-set")]
use image as _;

fn main() {
    #[cfg(target_os = "espidf")]
    esp_idf_sys::link_patches();
    #[cfg(all(target_os = "espidf", feature = "qemu"))]
    platform::qemu_psram();

    println!("eb-hello: {WIDTH}x{HEIGHT}, page {} bytes", PAGE.len());
    platform::report("boot");
    atomics_check();
    platform::engine_stack_in_psram();

    let engine = std::thread::Builder::new()
        .name("engine".into())
        .stack_size(ENGINE_STACK)
        .spawn(run)
        .expect("spawn engine thread");
    engine.join().expect("engine thread panicked");

    platform::report("end");
    println!("eb-hello: done");
}

fn run() {
    platform::report("engine start");

    let t = Instant::now();
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    for font in FONTS {
        collection.register_fonts(Blob::new(std::sync::Arc::new(font)), None);
    }
    collection.register_fonts(Blob::new(std::sync::Arc::new(blitz_dom::BULLET_FONT)), None);
    let font_ctx = FontContext {
        collection,
        source_cache: SourceCache::default(),
    };
    let config = DocumentConfig {
        viewport: Some(Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light)),
        font_ctx: Some(font_ctx),
        ..Default::default()
    };
    phase("fonts", t);

    let t = Instant::now();
    let mut doc = HtmlDocument::from_html(PAGE, config);
    phase("parse", t);

    // Sequential style traversal is DocumentConfig's default (StyleThreading::Sequential).
    // With the log-phase-times feature, blitz-dom prints the style/layout split itself.
    let t = Instant::now();
    doc.resolve(0.0);
    phase("style+layout", t);

    let mut renderer = VelloCpuImageRenderer::new(WIDTH, HEIGHT);
    let mut frame = Vec::new();
    for pass in ["paint (cold)", "paint (warm)"] {
        let t = Instant::now();
        renderer.render_to_vec(
            |scene| blitz_paint::paint_scene(scene, &mut doc, 1.0, WIDTH, HEIGHT, 0, 0),
            &mut frame,
        );
        phase(pass, t);
    }

    // Prove that something was drawn: the share of non-background pixels and a checksum.
    let background = [0xee, 0xf1, 0xf5, 0xff];
    let drawn = frame.chunks_exact(4).filter(|px| *px != background).count();
    let sum = frame.iter().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(*b))
    });
    println!(
        "eb-hello: frame {} bytes at {:p}, {drawn} non-background pixels, checksum {sum:08x}",
        frame.len(),
        frame.as_ptr()
    );
}

/// H1/H2 "PSRAM atomics" check: two threads (one per core where the scheduler spreads them)
/// hammer atomics and `Arc` refcounts that live in a PSRAM allocation (> 4 KB, so malloc puts
/// it in PSRAM), including a 64-bit atomic (an ESP-IDF libcall). Lost updates mean broken atomics.
fn atomics_check() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    const ROUNDS: u32 = 200_000;
    struct Shared {
        counters: [AtomicU32; 2048],
        wide: AtomicU64,
    }
    let shared: Arc<Shared> = Arc::new(Shared {
        counters: std::array::from_fn(|_| AtomicU32::new(0)),
        wide: AtomicU64::new(0),
    });
    let t = Instant::now();
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || {
                for i in 0..ROUNDS {
                    let local = Arc::clone(&shared); // refcount traffic on the PSRAM block
                    local.counters[0].fetch_add(1, Ordering::AcqRel);
                    let _ = local.counters[1].compare_exchange(
                        i,
                        i + 1,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    );
                    local.wide.fetch_add(1, Ordering::AcqRel);
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("atomics worker panicked");
    }
    let (n, wide) = (
        shared.counters[0].load(Ordering::Acquire),
        shared.wide.load(Ordering::Acquire),
    );
    let ok = n == 2 * ROUNDS && wide == u64::from(2 * ROUNDS) && Arc::strong_count(&shared) == 1;
    println!(
        "eb-hello: atomics {} at {:p}: u32 {n}/{}, u64 {wide}/{}, arc count {}, {:.1} ms",
        if ok { "OK" } else { "BROKEN" },
        Arc::as_ptr(&shared),
        2 * ROUNDS,
        2 * ROUNDS,
        Arc::strong_count(&shared),
        t.elapsed().as_secs_f64() * 1000.0
    );
}

fn phase(name: &str, start: Instant) {
    println!(
        "eb-hello: phase {name}: {:.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    platform::report(name);
}

#[cfg(target_os = "espidf")]
mod platform {
    use esp_idf_sys as sys;

    /// Print heap usage per region: used now, peak (high-water mark), total; plus the task count
    /// (sequential Stylo must not spawn threads).
    pub fn report(label: &str) {
        for (name, caps) in [
            ("internal", sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT),
            ("psram", sys::MALLOC_CAP_SPIRAM),
        ] {
            // SAFETY: plain queries of the heap allocator's statistics.
            let (total, free, min_free) = unsafe {
                (
                    sys::heap_caps_get_total_size(caps),
                    sys::heap_caps_get_free_size(caps),
                    sys::heap_caps_get_minimum_free_size(caps),
                )
            };
            println!(
                "eb-hello: heap {label} {name}: used {} KB, peak {} KB, total {} KB",
                (total - free) / 1024,
                (total - min_free) / 1024,
                total / 1024
            );
        }
        // SAFETY: FreeRTOS query without side effects.
        let tasks = unsafe { sys::uxTaskGetNumberOfTasks() };
        println!("eb-hello: tasks {label}: {tasks}");
    }

    /// Make the next pthread (the engine thread) allocate its stack in PSRAM: internal RAM is
    /// too small for a 512 KB stack next to Wi-Fi and DMA buffers.
    pub fn engine_stack_in_psram() {
        // SAFETY: reads the default config and sets it for threads created by this task.
        unsafe {
            let mut cfg = sys::esp_pthread_get_default_config();
            cfg.stack_alloc_caps = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
            sys::esp!(sys::esp_pthread_set_cfg(&cfg)).expect("esp_pthread_set_cfg");
        }
    }

    /// QEMU's esp32p4 has a plain-RAM PSRAM window at 0x48000000 but fails the chip probe, so
    /// ESP-IDF boots without PSRAM (SPIRAM_IGNORE_NOTFOUND). Add 32 MB of the window to the heap
    /// and turn on the malloc policy that esp_psram_init() would have enabled.
    #[cfg(feature = "qemu")]
    pub fn qemu_psram() {
        // esp_heap_caps_init.h is not in esp-idf-sys's bindings.
        unsafe extern "C" {
            fn heap_caps_add_region(start: isize, end: isize) -> sys::esp_err_t;
        }
        const BASE: isize = 0x4800_0000;
        const SIZE: isize = 32 << 20;
        // SAFETY: only built for the emulator, where this window is RAM nothing else uses.
        unsafe {
            if sys::heap_caps_get_total_size(sys::MALLOC_CAP_SPIRAM) == 0 {
                sys::esp!(heap_caps_add_region(BASE, BASE + SIZE)).expect("add PSRAM heap");
                sys::heap_caps_malloc_extmem_enable(sys::CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL as _);
                println!(
                    "eb-hello: qemu: added {} MB PSRAM heap at {BASE:#x}",
                    SIZE >> 20
                );
            }
        }
    }
}

#[cfg(not(target_os = "espidf"))]
mod platform {
    /// On Linux hosts, print the process's thread count (sequential Stylo must not spawn threads).
    pub fn report(label: &str) {
        if let Ok(tasks) = std::fs::read_dir("/proc/self/task") {
            println!("eb-hello: tasks {label}: {}", tasks.count());
        }
    }
    pub fn engine_stack_in_psram() {}
}
