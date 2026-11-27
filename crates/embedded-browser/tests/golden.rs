//! Golden screenshots of the committed corpus through the public API.
//!
//! Each `testpages/<name>.html` renders at 1024x600 and is compared with
//! `testpages/golden/<name>.png` (RGB565 expanded to 8-bit RGB). vello_cpu picks its SIMD
//! level at runtime, so CI machines differ slightly: a pixel counts as different only past
//! one RGB565 step, and a small share of such pixels is allowed.
//!
//! `UPDATE_GOLDENS=1 cargo test -p embedded-browser --test golden` rewrites the goldens.
//! On a mismatch, the actual frame and a diff mask go to `target/golden-diff/`.

use std::path::{Path, PathBuf};

use embedded_browser::{Browser, Config, FileNet};

const W: usize = 1024;
const H: usize = 600;
/// Per-channel delta (8-bit) still counted as equal: one RGB565 step expands to 8 or 9.
const CHANNEL_TOLERANCE: u8 = 9;
/// Share of pixels allowed past the channel tolerance.
const PIXEL_BUDGET: f64 = 0.002;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn render(name: &str) -> Vec<u8> {
    let path = std::path::absolute(root().join("testpages").join(name)).unwrap();
    let url = url::Url::from_file_path(path).unwrap().to_string();
    let mut b = Browser::new(Config::default(), FileNet::default());
    b.load(&url);
    for _ in 0..1000 {
        b.tick();
        if b.is_idle() {
            break;
        }
    }
    assert!(b.is_idle(), "{name} did not settle");
    let mut fb = vec![0u16; W * H];
    b.render_raw(&mut fb, W);
    fb.iter()
        .flat_map(|&p| {
            let (r, g, b) = ((p >> 11) as u8, ((p >> 5) & 0x3f) as u8, (p & 0x1f) as u8);
            [
                (r << 3) | (r >> 2),
                (g << 2) | (g >> 4),
                (b << 3) | (b >> 2),
            ]
        })
        .collect()
}

fn check(name: &str) {
    let actual = render(&format!("{name}.html"));
    let golden = root().join("testpages/golden").join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
        image::save_buffer(
            &golden,
            &actual,
            W as u32,
            H as u32,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        return;
    }
    let expected = image::open(&golden)
        .unwrap_or_else(|e| panic!("{}: {e} (run with UPDATE_GOLDENS=1)", golden.display()))
        .to_rgb8();
    assert_eq!(expected.dimensions(), (W as u32, H as u32));
    let mut mask = vec![0u8; W * H];
    let mut differing = 0;
    for (i, (a, e)) in actual
        .chunks_exact(3)
        .zip(expected.chunks_exact(3))
        .enumerate()
    {
        if a.iter()
            .zip(e)
            .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE)
        {
            differing += 1;
            mask[i] = 255;
        }
    }
    let share = differing as f64 / (W * H) as f64;
    if share > PIXEL_BUDGET {
        let out = root().join("target/golden-diff");
        std::fs::create_dir_all(&out).unwrap();
        let (w, h) = (W as u32, H as u32);
        image::save_buffer(
            out.join(format!("{name}-actual.png")),
            &actual,
            w,
            h,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
        image::save_buffer(
            out.join(format!("{name}-diff.png")),
            &mask,
            w,
            h,
            image::ExtendedColorType::L8,
        )
        .unwrap();
        panic!(
            "{name}: {differing} pixels ({:.3} %) differ from the golden; see {}",
            share * 100.0,
            out.display()
        );
    }
}

#[test]
fn dashboard() {
    check("dashboard");
}

#[test]
fn article() {
    check("article");
}

#[test]
fn links() {
    check("links");
}
