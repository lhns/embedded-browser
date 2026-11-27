//! Heap-cap gate: each page goes through the public `embedded-browser` API (load, settle,
//! render one screen in bands) in a fresh process whose heap is capped. CI runs it on
//! wasm32-wasip1, whose layout matches the 32-bit device (docs/m0/host-results.md).
//!
//! The 1024x600 RGB565 framebuffer is allocated before measuring and not counted: on the
//! device it is the panel's buffer. Allocator overhead per block is not counted either.

use std::path::PathBuf;
use std::process::Command;

use embedded_browser::{Browser, Config, FileNet};

use crate::alloc::{self, Cat};
use crate::{field, mb, opt, page_name, pages};

const MB: f64 = 1024.0 * 1024.0;

/// One page under `--cap-mb`; prints `page=.. peak=..` and aborts if the cap is hit.
pub fn page(args: &[String]) {
    let page = PathBuf::from(args.first().expect("page"));
    let cap_mb: f64 = opt(args, "--cap-mb").map_or(12.0, |c| c.parse().expect("cap"));
    let config = Config::default();
    let (w, h) = (usize::from(config.width), usize::from(config.height));
    let mut fb = vec![0u16; w * h];
    let abs = std::path::absolute(&page).expect("absolute path");
    let url = url::Url::from_file_path(&abs)
        .expect("file url")
        .to_string();

    alloc::reset_peak();
    let base = alloc::snapshot();
    alloc::set_cap(base.total + (cap_mb * MB) as isize);
    let damage = alloc::in_phase(Cat::Dom, || {
        let mut browser = Browser::new(config, FileNet::default());
        browser.load(&url);
        for _ in 0..10_000 {
            browser.tick();
            if browser.is_idle() {
                break;
            }
        }
        assert!(browser.is_idle(), "page did not settle");
        browser.render_raw(&mut fb, w)
    });
    let end = alloc::snapshot();
    alloc::set_cap(isize::MAX);
    println!(
        "page={} peak={} blocks_at_peak={} rendered_px={}",
        page_name(&page),
        end.peak - base.total,
        end.blocks_at_peak,
        damage.area()
    );
}

/// Spawn `heap-cap-page` per page and fail unless every page stays under the P4 cap.
pub fn run(args: &[String]) {
    let wasm = opt(args, "--wasm").map(str::to_owned);
    let cap: f64 = opt(args, "--cap-mb").map_or(12.0, |c| c.parse().expect("cap"));
    let s3: f64 = opt(args, "--s3-mb").map_or(4.0, |c| c.parse().expect("s3 tier"));
    // Drop option values so `pages` sees only page paths.
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a.starts_with("--") {
            it.next();
        } else {
            positional.push(a.clone());
        }
    }
    let exe = std::env::current_exe().expect("exe");
    println!("| page | peak MB | P4 gate (≤{cap} MB) | S3 tier (≤{s3} MB, informational) |");
    println!("|---|---|---|---|");
    let mut failed = false;
    for page in pages(&positional) {
        let mut cmd = match &wasm {
            Some(w) => {
                let mut c = Command::new("node");
                c.args(["--no-warnings", "tools/measure/wasi-run.mjs", w.as_str()]);
                c.arg("heap-cap-page");
                c.arg(format!(
                    "/repo/{}",
                    page.to_string_lossy().replace('\\', "/")
                ));
                c
            }
            None => {
                let mut c = Command::new(&exe);
                c.arg("heap-cap-page").arg(&page);
                c
            }
        };
        cmd.args(["--cap-mb", &cap.to_string()]);
        let out = cmd.output().expect("spawn heap-cap-page");
        let line = String::from_utf8_lossy(&out.stdout).into_owned();
        let name = page_name(&page);
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            println!(
                "| {name} | over cap or failed | **fail**: {} | — |",
                err.lines().find(|l| !l.trim().is_empty()).unwrap_or("")
            );
            failed = true;
            continue;
        }
        let peak: isize = field(&line, "peak").parse().expect("peak");
        let p4 = peak as f64 <= cap * MB;
        failed |= !p4;
        println!(
            "| {name} | {} | {} | {} |",
            mb(peak),
            if p4 { "pass" } else { "**fail**" },
            if peak as f64 <= s3 * MB {
                "fits"
            } else {
                "over"
            },
        );
    }
    if failed {
        std::process::exit(1);
    }
}
