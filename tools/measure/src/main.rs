//! Measurement tool: per-category heap counting, corpus runner and raster timer.
//!
//! Commands (pages default to the committed corpus):
//!   heap <page> [--raster vello|tiny|none] [--band H]   one page, one process
//!   heap-all [pages..]                                  spawns `heap` per page and config
//!   raster [--iters N] [pages..]                        full-frame and band timings
//!   band [pages..]                                      band vs full-frame pixel diff
//!   damage                                              restyle damage experiment
//!   png <out-dir> [pages..]                             save full frames

mod alloc;
mod engine;
mod threads;

use std::path::{Path, PathBuf};

use alloc::{CATS, Cat, Counting};
use engine::{H, Raster, W};

#[global_allocator]
static GLOBAL: Counting = Counting;

const CORPUS: [&str; 3] = [
    "testpages/dashboard.html",
    "testpages/article.html",
    "testpages/links.html",
];
const BAND: u32 = 32;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = args
        .split_first()
        .map(|(c, r)| (c.as_str(), r))
        .unwrap_or(("", &[]));
    match cmd {
        "heap" => heap(rest),
        "heap-all" => heap_all(rest),
        "raster" => raster(rest),
        "band" => band(rest),
        "damage" => damage(),
        "png" => png(rest),
        _ => {
            eprintln!("usage: measure heap|heap-all|raster|band|damage|png ... (see src/main.rs)");
            std::process::exit(2);
        }
    }
}

/// Positional page arguments, or the corpus plus any fetched pages.
fn pages(args: &[String]) -> Vec<PathBuf> {
    let given: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if !given.is_empty() {
        return given;
    }
    let mut v: Vec<PathBuf> = CORPUS.iter().map(PathBuf::from).collect();
    if let Ok(dir) = std::fs::read_dir("testpages/fetched") {
        let mut fetched: Vec<PathBuf> = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "html"))
            .collect();
        fetched.sort();
        v.extend(fetched);
    }
    v
}

fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn parse_raster(s: &str) -> Option<Raster> {
    match s {
        "vello" => Some(Raster::VelloCpu),
        "tiny" => Some(Raster::TinySkia),
        _ => None,
    }
}

fn page_name(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().into_owned()
}

fn mb(b: isize) -> String {
    format!("{:.2}", b as f64 / (1024.0 * 1024.0))
}

/// One page in a fresh process, so one-time globals (UA sheet, Stylo statics) are counted.
fn heap(args: &[String]) {
    let page = PathBuf::from(args.first().expect("page"));
    let raster = opt(args, "--raster").and_then(parse_raster);
    let band: u32 = opt(args, "--band").map_or(H, |b| b.parse().expect("band height"));
    let threads_before = threads::count();
    alloc::reset_peak();
    let base = alloc::snapshot();

    let mut doc = engine::load(&page);
    let engine = alloc::snapshot();
    let nodes = doc.tree().iter().count();

    let mut colors = 0;
    if let Some(r) = raster {
        alloc::in_phase(Cat::Raster, || {
            for y in (0..H).step_by(band as usize) {
                let (buf, _) = engine::render(&mut doc, r, W, band, y);
                colors = colors.max(engine::distinct_colors(&buf, 64));
            }
        });
    }
    let end = alloc::snapshot();
    let threads_after = threads::count();
    // `build_global` fails if anything initialized rayon's global pool earlier.
    // wasm32-wasip1 has no threads at all, so finishing there is the proof instead.
    let rayon_untouched = if cfg!(target_os = "wasi") {
        "n/a(wasi)".to_string()
    } else {
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build_global()
            .is_ok()
            .to_string()
    };

    let cats = |s: &[isize; 5]| {
        CATS.iter()
            .map(|c| format!("{}={}", c.name(), s[*c as usize] - base.live[*c as usize]))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!(
        "page={} raster={} band={} nodes={} baseline={} engine_peak={} peak={} blocks_at_peak={} threads_before={} threads_after={} rayon_global_untouched={} colors={} | at_peak: {} | engine_live: {}",
        page_name(&page),
        raster.map_or("none", Raster::name),
        band,
        nodes,
        base.total,
        engine.peak - base.total,
        end.peak - base.total,
        end.blocks_at_peak,
        threads_before.map_or("n/a".into(), |n| n.to_string()),
        threads_after.map_or("n/a".into(), |n| n.to_string()),
        rayon_untouched,
        colors,
        cats(&end.at_peak),
        cats(&engine.live),
    );
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
        .unwrap_or("?")
}

/// With `--wasm <measure.wasm>`, each run goes through Node's WASI (`wasi-run.mjs`),
/// with the repo (the current directory) mounted at `/repo`.
fn heap_all(args: &[String]) {
    let exe = std::env::current_exe().expect("exe");
    let wasm = opt(args, "--wasm").map(str::to_owned);
    let args: Vec<String> = args
        .iter()
        .filter(|a| Some(a.as_str()) != wasm.as_deref() && a.as_str() != "--wasm")
        .cloned()
        .collect();
    println!(
        "| page | config | DOM nodes | engine peak MB | total peak MB | at peak: dom+style+layout / text / images / raster / other MB | blocks at peak | threads before→after | rayon pool untouched |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for page in pages(&args) {
        let configs: [(&str, Option<&str>, Option<&str>); 5] = [
            ("engine only", None, None),
            ("vello_cpu full frame", Some("vello"), None),
            ("vello_cpu 32px bands", Some("vello"), Some("32")),
            ("tiny-skia full frame", Some("tiny"), None),
            ("tiny-skia 32px bands", Some("tiny"), Some("32")),
        ];
        for (label, r, b) in configs {
            let mut cmd = match &wasm {
                Some(w) => {
                    let mut c = std::process::Command::new("node");
                    c.args(["tools/measure/wasi-run.mjs", w.as_str(), "heap"]);
                    let rel = page.to_string_lossy().replace('\\', "/");
                    c.arg(format!("/repo/{rel}"));
                    c
                }
                None => {
                    let mut c = std::process::Command::new(&exe);
                    c.arg("heap").arg(&page);
                    c
                }
            };
            if let Some(r) = r {
                cmd.args(["--raster", r]);
            }
            if let Some(b) = b {
                cmd.args(["--band", b]);
            }
            let out = cmd.output().expect("spawn heap");
            let line = String::from_utf8_lossy(&out.stdout).into_owned();
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                println!(
                    "| {} | {label} | failed: {} |",
                    page_name(&page),
                    err.lines().last().unwrap_or("")
                );
                continue;
            }
            let at_peak = line.split("at_peak: ").nth(1).unwrap_or("");
            let c = |k: &str| field(at_peak, k).parse::<isize>().map_or("?".into(), mb);
            let num = |k: &str| field(&line, k).parse::<isize>().map_or("?".into(), mb);
            println!(
                "| {} | {label} | {} | {} | {} | {} / {} / {} / {} / {} | {} | {}→{} | {} |",
                page_name(&page),
                field(&line, "nodes"),
                num("engine_peak"),
                num("peak"),
                c(Cat::Dom.name()),
                c(Cat::Text.name()),
                c(Cat::Images.name()),
                c(Cat::Raster.name()),
                c(Cat::Other.name()),
                field(&line, "blocks_at_peak"),
                field(&line, "threads_before"),
                field(&line, "threads_after"),
                field(&line, "rayon_global_untouched"),
            );
        }
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn raster(args: &[String]) {
    let iters: usize = opt(args, "--iters").map_or(5, |n| n.parse().expect("iters"));
    let args: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !(a.as_str() == "--iters" || (*i > 0 && args[i - 1] == "--iters")))
        .map(|(_, a)| a.clone())
        .collect();
    println!("simd: {}", simd_note());
    println!(
        "| page | rasterizer | cold full frame ms (paint+raster) | warm full frame ms median (paint / raster) | 19 bands of 1024x32 ms median (paint / raster) |"
    );
    println!("|---|---|---|---|---|");
    for page in pages(&args) {
        let mut doc = engine::load(&page);
        for r in [Raster::VelloCpu, Raster::TinySkia] {
            let (_, cold) = engine::render(&mut doc, r, W, H, 0);
            let (mut fp, mut fr, mut bp, mut br) = (vec![], vec![], vec![], vec![]);
            for _ in 0..iters {
                let (_, t) = engine::render(&mut doc, r, W, H, 0);
                fp.push(t.paint);
                fr.push(t.raster);
                let (mut p, mut q) = (0.0, 0.0);
                for y in (0..H).step_by(BAND as usize) {
                    let (_, t) = engine::render(&mut doc, r, W, BAND, y);
                    p += t.paint;
                    q += t.raster;
                }
                bp.push(p);
                br.push(q);
            }
            let ms = |s: f64| format!("{:.1}", s * 1000.0);
            let (fp, fr, bp, br) = (
                median(&mut fp),
                median(&mut fr),
                median(&mut bp),
                median(&mut br),
            );
            println!(
                "| {} | {} | {} | {} ({} / {}) | {} ({} / {}) |",
                page_name(&page),
                r.name(),
                ms(cold.paint + cold.raster),
                ms(fp + fr),
                ms(fp),
                ms(fr),
                ms(bp + br),
                ms(bp),
                ms(br),
            );
        }
    }
}

fn simd_note() -> String {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        format!(
            "{} host; runtime sse4.2={} avx2={}; compile-time sse2={}",
            std::env::consts::ARCH,
            std::arch::is_x86_feature_detected!("sse4.2"),
            std::arch::is_x86_feature_detected!("avx2"),
            cfg!(target_feature = "sse2"),
        )
    }
    #[cfg(target_arch = "wasm32")]
    {
        format!("wasm32; simd128={}", cfg!(target_feature = "simd128"))
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "wasm32")))]
    {
        std::env::consts::ARCH.to_string()
    }
}

fn band(args: &[String]) {
    println!(
        "| page | rasterizer | route | bands | bands with differences | differing pixels | of which in edge rows | max channel delta |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for page in pages(args) {
        let mut doc = engine::load(&page);
        for r in [Raster::VelloCpu, Raster::TinySkia] {
            let (full, _) = engine::render(&mut doc, r, W, H, 0);
            for route in [
                "viewport scroll + 32px viewport",
                "recorded scene replayed at -y",
            ] {
                let (mut n, mut bad, mut px, mut edge, mut max) = (0, 0, 0, 0, 0);
                // Only bands fully inside the frame, so every row has a reference.
                for y in (0..=H - BAND).step_by(BAND as usize) {
                    let buf = if route.starts_with("viewport") {
                        engine::render(&mut doc, r, W, BAND, y).0
                    } else {
                        engine::render_band_replay(&mut doc, r, BAND, y)
                    };
                    let d = engine::diff_band(&full, &buf, y);
                    n += 1;
                    bad += usize::from(d.differing > 0);
                    px += d.differing;
                    edge += d.at_edges;
                    max = max.max(d.max_delta);
                }
                println!(
                    "| {} | {} | {route} | {n} | {bad} | {px} | {edge} | {max} |",
                    page_name(&page),
                    r.name()
                );
            }
        }
    }
}

fn png(args: &[String]) {
    let out = PathBuf::from(args.first().expect("out dir"));
    for page in pages(&args[1..]) {
        let mut doc = engine::load(&page);
        for r in [Raster::VelloCpu, Raster::TinySkia] {
            let (buf, _) = engine::render(&mut doc, r, W, H, 0);
            let path = out.join(format!("{}-{}.png", page_name(&page), r.name()));
            engine::save_png(&path, &buf, W, H);
            println!(
                "{} ({} colours)",
                path.display(),
                engine::distinct_colors(&buf, 100_000)
            );
        }
    }
}

/// Toggle one dashboard card and check what Blitz exposes about the change.
fn damage() {
    use blitz_dom::{LocalName, Namespace, QualName};
    let mut doc = engine::load(Path::new("testpages/dashboard.html"));
    let (before, _) = engine::render(&mut doc, Raster::VelloCpu, W, H, 0);
    let card = doc.get_element_by_id("card-1").expect("card-1");
    {
        let mut m = doc.mutate();
        m.set_attribute(
            card,
            QualName::new(None, Namespace::from(""), LocalName::from("class")),
            "card on",
        );
    }
    // Style only: Stylo records RestyleDamage per element; resolve() would clear it.
    doc.resolve_stylist(0.0);
    let damaged: Vec<_> = doc
        .tree()
        .iter()
        .filter_map(|(id, n)| n.damage().filter(|d| !d.is_empty()).map(|d| (id, d)))
        .collect();
    let bounds = |doc: &blitz_dom::BaseDocument, id| {
        let n = doc.get_node(id).unwrap();
        let p = n.absolute_position(0.0, 0.0);
        let s = n.final_layout().size;
        (p.y, p.y + s.height)
    };
    let old: Vec<_> = damaged.iter().map(|(id, _)| bounds(&doc, *id)).collect();
    doc.resolve(0.0);
    let still: usize = doc
        .tree()
        .iter()
        .filter(|(_, n)| n.damage().is_some_and(|d| !d.is_empty()))
        .count();
    let new: Vec<_> = damaged.iter().map(|(id, _)| bounds(&doc, *id)).collect();
    let (after, _) = engine::render(&mut doc, Raster::VelloCpu, W, H, 0);

    println!("damaged nodes after resolve_stylist: {}", damaged.len());
    for ((id, d), (o, n)) in damaged.iter().zip(old.iter().zip(&new)) {
        let tag = doc
            .get_node(*id)
            .and_then(|n| n.element_data().map(|e| e.name.local.to_string()))
            .unwrap_or_else(|| "(pseudo/anon)".into());
        println!(
            "  node {id:?} <{tag}> damage={d:?} y {:.0}..{:.0} -> {:.0}..{:.0}",
            o.0, o.1, n.0, n.1
        );
    }
    println!("nodes still carrying damage after resolve(): {still}");
    let band_of = |y: f32| (y.max(0.0) as u32 / BAND).min(H / BAND);
    let mut bands: Vec<u32> = old
        .iter()
        .chain(&new)
        .flat_map(|(a, b)| band_of(*a)..=band_of((*b - 1.0).max(*a)))
        .collect();
    bands.sort();
    bands.dedup();
    let row = (W * 4) as usize;
    let changed_rows: Vec<u32> = (0..H)
        .filter(|y| {
            let r = *y as usize * row..(*y as usize + 1) * row;
            before[r.clone()] != after[r]
        })
        .collect();
    let mut changed_bands: Vec<u32> = changed_rows.iter().map(|y| y / BAND).collect();
    changed_bands.dedup();
    let outside: Vec<_> = changed_bands
        .iter()
        .filter(|b| !bands.contains(b))
        .collect();
    println!("bands from damaged-node bounds: {bands:?}");
    println!(
        "bands with changed pixels: {changed_bands:?} (rows {}..{})",
        changed_rows.first().unwrap_or(&0),
        changed_rows.last().map_or(0, |y| y + 1)
    );
    println!("changed bands not covered by damage: {outside:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The phase tag is process-global, so tests that load pages or check counters run one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn page(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testpages")
            .join(name)
    }

    #[test]
    fn frees_are_charged_to_the_allocating_category() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let before = alloc::snapshot();
        let block = alloc::in_phase(Cat::Images, || vec![1u8; 1 << 20]);
        let mid = alloc::snapshot();
        assert!(mid.live[Cat::Images as usize] - before.live[Cat::Images as usize] >= 1 << 20);
        alloc::in_phase(Cat::Raster, || drop(block));
        let after = alloc::snapshot();
        assert_eq!(
            after.live[Cat::Images as usize],
            before.live[Cat::Images as usize]
        );
        assert!(after.peak >= mid.total);
    }

    #[test]
    fn band_matches_full_frame_and_page_is_not_blank() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut doc = engine::load(&page("dashboard.html"));
        let (full, _) = engine::render(&mut doc, Raster::VelloCpu, W, H, 0);
        assert!(engine::distinct_colors(&full, 64) >= 64, "blank frame");
        let (band, _) = engine::render(&mut doc, Raster::VelloCpu, W, BAND, 128);
        let d = engine::diff_band(&full, &band, 128);
        assert!(d.differing <= 2 && d.max_delta <= 2, "{d:?}");
    }

    #[test]
    fn images_from_data_uris_are_decoded() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let before = alloc::snapshot();
        let doc = engine::load(&page("article.html"));
        let after = alloc::snapshot();
        // Three distinct 160x110 PNGs decode to RGBA: at least 3 * 160 * 110 * 4 bytes live.
        assert!(
            after.live[Cat::Images as usize] - before.live[Cat::Images as usize]
                >= 3 * 160 * 110 * 4
        );
        drop(doc);
    }
}
