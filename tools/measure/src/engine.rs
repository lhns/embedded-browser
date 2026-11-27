//! Minimal Blitz setup: embedded fonts, local-only net, sequential styling.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyrender::ImageRenderer;
use blitz_dom::{BaseDocument, DocumentConfig, FontContext, StyleThreading};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use parley::fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};

use crate::alloc::{Cat, in_phase};

pub const W: u32 = 1024;
pub const H: u32 = 600;

/// Font collection with system fonts off and Noto Sans registered for every generic family.
///
/// The font bytes stay `'static` (as they would in a flash-mapped partition on the
/// device); only the collection's index structures land on the heap.
pub fn font_ctx() -> FontContext {
    let mut ctx = FontContext {
        source_cache: SourceCache::new_shared(),
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
    };
    let mut families = Vec::new();
    for bytes in [ttf_noto_sans::REGULAR, ttf_noto_sans::BOLD] {
        let registered = ctx
            .collection
            .register_fonts(Blob::new(Arc::new(bytes) as _), None);
        families.extend(registered.iter().map(|(id, _)| *id));
    }
    families.dedup();
    for generic in [
        GenericFamily::SansSerif,
        GenericFamily::Serif,
        GenericFamily::Monospace,
        GenericFamily::SystemUi,
    ] {
        ctx.collection
            .append_generic_families(generic, families.iter().copied());
    }
    ctx
}

/// Serves `data:` and `file:` URLs synchronously; everything else fails (no network).
pub struct LocalNet;

const IMAGE_MAGIC: [&[u8]; 4] = [b"\x89PNG", b"\xff\xd8\xff", b"GIF8", b"RIFF"];

impl NetProvider for LocalNet {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url;
        let bytes = match url.scheme() {
            "data" => data_url::DataUrl::process(url.as_str())
                .ok()
                .and_then(|d| d.decode_to_vec().ok())
                .map(|(body, _)| body),
            "file" => url.to_file_path().ok().and_then(|p| std::fs::read(p).ok()),
            _ => None,
        };
        let Some(bytes) = bytes else {
            handler.error(url.to_string(), "not available offline".into());
            return;
        };
        // The handler decodes images synchronously; charge that work to `Images`.
        let cat = if IMAGE_MAGIC.iter().any(|m| bytes.starts_with(m)) {
            Cat::Images
        } else {
            Cat::Dom
        };
        in_phase(cat, || handler.bytes(url.to_string(), Bytes::from(bytes)));
    }
}

/// Parse a page and resolve style and layout at the 1024x600 viewport.
pub fn load(path: &Path) -> BaseDocument {
    let html = std::fs::read_to_string(path).expect("read page");
    let abs = std::path::absolute(path).expect("absolute path");
    let base_url = url::Url::from_file_path(&abs)
        .expect("file url")
        .to_string();
    let font_ctx = in_phase(Cat::Text, font_ctx);
    in_phase(Cat::Dom, || {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(W, H, 1.0, ColorScheme::Light)),
            base_url: Some(base_url),
            net_provider: Some(Arc::new(LocalNet)),
            font_ctx: Some(font_ctx),
            style_threading: StyleThreading::Sequential,
            ..Default::default()
        };
        let mut doc = HtmlDocument::from_html(&html, config).into_inner();
        // Resources answered during parse arrive as messages; the first resolve ingests
        // them, and the second picks up images requested from styles.
        doc.resolve(0.0);
        doc.resolve(0.0);
        doc
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raster {
    VelloCpu,
    TinySkia,
}

impl Raster {
    pub fn name(self) -> &'static str {
        match self {
            Raster::VelloCpu => "vello_cpu",
            Raster::TinySkia => "tiny-skia",
        }
    }
}

/// Seconds spent building the scene (Blitz paint walk) and rasterizing it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub paint: f64,
    pub raster: f64,
}

/// Render a `w`x`h` window of the page whose top edge is at page `y` (viewport scroll).
pub fn render(doc: &mut BaseDocument, raster: Raster, w: u32, h: u32, y: u32) -> (Vec<u8>, Timing) {
    match raster {
        Raster::VelloCpu => render_with::<anyrender_vello_cpu::VelloCpuImageRenderer>(doc, w, h, y),
        Raster::TinySkia => render_with::<anyrender_tiny_skia::TinySkiaImageRenderer>(doc, w, h, y),
    }
}

fn render_with<R: ImageRenderer>(
    doc: &mut BaseDocument,
    w: u32,
    h: u32,
    y: u32,
) -> (Vec<u8>, Timing) {
    doc.set_viewport_scroll(blitz_dom::Point {
        x: 0.0,
        y: y as f64,
    });
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let mut renderer = R::new(w, h);
    let mut paint = 0.0;
    let start = Instant::now();
    renderer.render(
        |scene| {
            let t = Instant::now();
            blitz_paint::paint_scene(scene, doc, 1.0, w, h, 0, 0);
            paint = t.elapsed().as_secs_f64();
        },
        &mut buf,
    );
    let total = start.elapsed().as_secs_f64();
    (
        buf,
        Timing {
            paint,
            raster: total - paint,
        },
    )
}

/// Record the full viewport into an `anyrender::Scene`, then replay it into a band
/// renderer translated by `-y`: banding without relying on Blitz's own culling.
pub fn render_band_replay(doc: &mut BaseDocument, raster: Raster, h: u32, y: u32) -> Vec<u8> {
    doc.set_viewport_scroll(blitz_dom::Point { x: 0.0, y: 0.0 });
    let mut recorded = anyrender::Scene::new();
    blitz_paint::paint_scene(&mut recorded, doc, 1.0, W, H, 0, 0);
    let mut buf = vec![0u8; (W * h * 4) as usize];
    let shift = kurbo::Affine::translate((0.0, -(y as f64)));
    match raster {
        Raster::VelloCpu => {
            let mut r = anyrender_vello_cpu::VelloCpuImageRenderer::new(W, h);
            r.render(
                |s| anyrender::PaintScene::append_scene(s, recorded, shift),
                &mut buf,
            );
        }
        Raster::TinySkia => {
            let mut r = anyrender_tiny_skia::TinySkiaImageRenderer::new(W, h);
            r.render(
                |s| anyrender::PaintScene::append_scene(s, recorded, shift),
                &mut buf,
            );
        }
    }
    buf
}

/// Pixel difference between a band and rows `y..y+band_h` of a full RGBA frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Diff {
    pub differing: usize,
    pub max_delta: u8,
    /// Differing pixels in the band's first or last two rows.
    pub at_edges: usize,
}

pub fn diff_band(full: &[u8], band: &[u8], y: u32) -> Diff {
    let row = (W * 4) as usize;
    let start = y as usize * row;
    let reference = &full[start..start + band.len()];
    let mut d = Diff::default();
    let rows = band.len() / row;
    for (i, (a, b)) in reference
        .chunks_exact(4)
        .zip(band.chunks_exact(4))
        .enumerate()
    {
        let m = a
            .iter()
            .zip(b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        if m > 0 {
            let r = i / W as usize;
            d.at_edges += usize::from(r < 2 || r + 2 >= rows);
            d.differing += 1;
            d.max_delta = d.max_delta.max(m);
        }
    }
    d
}

/// Number of distinct RGBA colours in a frame, capped at `cap` (blank-frame guard).
pub fn distinct_colors(buf: &[u8], cap: usize) -> usize {
    let mut seen = std::collections::HashSet::new();
    for px in buf.chunks_exact(4) {
        seen.insert([px[0], px[1], px[2], px[3]]);
        if seen.len() >= cap {
            break;
        }
    }
    seen.len()
}

pub fn save_png(path: &Path, buf: &[u8], w: u32, h: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("create output dir");
    }
    image::save_buffer(path, buf, w, h, image::ExtendedColorType::Rgba8).expect("write png");
}
