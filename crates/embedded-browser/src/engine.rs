//! The Blitz wrapper. Nothing from Blitz, Stylo or anyrender leaves this module (ADR 0002).
//!
//! - **Net:** Blitz's `NetProvider` only queues `(request, handler)`. `tick` forwards the
//!   queue to the public [`Net`], keeps the handlers by [`ReqId`], and ends every handler
//!   in `bytes` or `error`; a dropped handler would leave a render-blocking stylesheet
//!   pending forever.
//! - **Damage:** Stylo's restyle damage is readable between `resolve_stylist()` and
//!   `resolve()`, which clears it. Relayout side effects carry no damage, so every
//!   resolve also diffs each box's absolute bounds against the previous resolve.
//! - **Raster:** each band is painted by scrolling the viewport to the band and painting
//!   a band-sized scene (`paint_scene`'s offsets only add, see docs/m0/host-results.md).
//!   Blitz culls to the band. Known limit: `position: fixed` children of the root cancel
//!   the viewport scroll, so they would repeat in every band.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{BaseDocument, DocumentConfig, FontContext, Node, NodeId, StyleThreading};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request as BlitzRequest};
use blitz_traits::shell::{ColorScheme, Viewport};
use parley::fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};

use crate::net::{Net, NetError, ReqId, Request, Response};
use crate::{Config, DamageRects, Rect};

type Queue = Arc<Mutex<Vec<(BlitzRequest, Box<dyn NetHandler>)>>>;

/// Blitz's side of the net seam: queue only, never block.
struct QueueNet(Queue);

impl NetProvider for QueueNet {
    fn fetch(&self, _doc_id: usize, request: BlitzRequest, handler: Box<dyn NetHandler>) {
        lock(&self.0).push((request, handler));
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

enum Pending {
    Document,
    Resource(String, Box<dyn NetHandler>),
}

/// A rectangle in page coordinates (CSS px), half-open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl PageRect {
    fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// Each box's visual rect from the last resolve, indexed by the node's slot.
#[derive(Default)]
struct Bounds(Vec<Option<(NodeId, PageRect)>>);

impl Bounds {
    fn slot(id: NodeId) -> usize {
        id.as_u64() as u32 as usize
    }

    fn get(&self, id: NodeId) -> Option<PageRect> {
        match self.0.get(Self::slot(id)) {
            Some(Some((i, r))) if *i == id => Some(*r),
            _ => None,
        }
    }

    fn insert(&mut self, id: NodeId, r: PageRect) {
        let s = Self::slot(id);
        if self.0.len() <= s {
            self.0.resize(s + 1, None);
        }
        self.0[s] = Some((id, r));
    }

    /// Rects of boxes that appeared, disappeared, moved or resized.
    fn diff(&self, new: &Bounds, out: &mut Vec<PageRect>) {
        let n = self.0.len().max(new.0.len());
        for s in 0..n {
            let a = self.0.get(s).copied().flatten();
            let b = new.0.get(s).copied().flatten();
            match (a, b) {
                (Some((ia, ra)), Some((ib, rb))) if ia == ib => {
                    if ra != rb {
                        out.extend([ra, rb]);
                    }
                }
                _ => out.extend(a.into_iter().chain(b).map(|(_, r)| r)),
            }
        }
    }
}

pub(crate) struct Engine {
    config: Config,
    doc: Option<BaseDocument>,
    url: Option<String>,
    queue: Queue,
    pending: HashMap<ReqId, Pending>,
    /// Subresource requests issued for the current document.
    subresources: usize,
    /// Restyle/relayout needed.
    dirty: bool,
    /// The next render redraws the whole screen.
    full: bool,
    /// Damaged page rects since the last render.
    damage: Vec<PageRect>,
    bounds: Bounds,
    start: Instant,
    renderer: Option<(u16, u16, VelloCpuImageRenderer)>,
    rgba: Vec<u8>,
    px: Vec<u16>,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            doc: None,
            url: None,
            queue: Arc::default(),
            pending: HashMap::new(),
            subresources: 0,
            dirty: false,
            full: true,
            damage: Vec::new(),
            bounds: Bounds::default(),
            start: Instant::now(),
            renderer: None,
            rgba: Vec::new(),
            px: Vec::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn is_idle(&self) -> bool {
        self.pending.is_empty() && lock(&self.queue).is_empty() && !self.dirty
    }

    pub fn invalidate(&mut self) {
        self.full = true;
    }

    pub fn load(&mut self, net: &mut dyn Net, url: &str) {
        // Responses for the old document are no longer wanted. Dropping a handler
        // uncalled is fine here: its document goes away with it.
        for (id, _) in self.pending.drain() {
            net.cancel(id);
        }
        lock(&self.queue).clear();
        self.url = Some(url.to_owned());
        if url.starts_with("data:") {
            match decode_data_url(url) {
                Some(body) => self.open_document(url, &body),
                None => self.open_placeholder(url, "This data: URL could not be decoded."),
            }
        } else {
            let id = net.fetch(Request::get(url, self.config.limits.document_bytes));
            self.pending.insert(id, Pending::Document);
        }
    }

    pub fn tick(&mut self, net: &mut dyn Net) -> bool {
        let mut delivered = false;
        while let Some((id, result)) = net.poll() {
            match self.pending.remove(&id) {
                Some(Pending::Document) => self.on_document(result),
                Some(Pending::Resource(url, handler)) => {
                    self.deliver(&url, handler, result);
                    delivered = true;
                }
                None => {}
            }
        }
        delivered |= self.forward_requests(net);
        if delivered {
            // M0 never tested what damage an image or stylesheet load marks; redraw all.
            self.full = true;
            self.dirty = true;
        }
        if self.dirty {
            self.resolve();
        }
        self.has_damage()
    }

    fn has_damage(&self) -> bool {
        let Some(doc) = &self.doc else { return false };
        !doc.has_pending_critical_resources() && (self.full || !self.damage.is_empty())
    }

    /// Hand Blitz's queued requests to the `Net`. Returns whether any completed inline
    /// (`data:` URLs, limit rejections).
    fn forward_requests(&mut self, net: &mut dyn Net) -> bool {
        let queued = std::mem::take(&mut *lock(&self.queue));
        let mut delivered = false;
        let limits = self.config.limits.clone();
        for (request, handler) in queued {
            let url = request.url.as_str();
            if url.starts_with("data:") {
                let result = decode_data_url(url)
                    .map(|body| Response {
                        url: url.to_owned(),
                        status: 200,
                        content_type: None,
                        body,
                    })
                    .ok_or_else(|| NetError::Failed("bad data: URL".into()));
                self.deliver(url, handler, result);
                delivered = true;
            } else if self.subresources >= limits.subresources {
                handler.error(url.to_owned(), "subresource limit reached".into());
                delivered = true;
            } else {
                self.subresources += 1;
                let max = limits.subresource_bytes.max(limits.image_bytes);
                let id = net.fetch(Request::get(url, max));
                self.pending
                    .insert(id, Pending::Resource(url.to_owned(), handler));
            }
        }
        delivered
    }

    /// Pass a subresource to Blitz, applying the size caps. Images over a cap become a
    /// grey placeholder; anything else over its cap is dropped.
    fn deliver(
        &mut self,
        url: &str,
        handler: Box<dyn NetHandler>,
        result: Result<Response, NetError>,
    ) {
        let limits = &self.config.limits;
        let response = match result {
            Ok(r) if r.status < 400 => r,
            Ok(r) => return handler.error(r.url, format!("HTTP {}", r.status)),
            Err(e) => return handler.error(url.to_owned(), e.to_string()),
        };
        let body = response.body;
        if is_image(&body) {
            let too_big = body.len() > limits.image_bytes
                || image_pixels(&body).is_none_or(|px| px > limits.image_pixels);
            let body = if too_big { placeholder_image() } else { body };
            handler.bytes(response.url, Bytes::from(body));
        } else if body.len() > limits.subresource_bytes {
            handler.error(response.url, "over the subresource size limit".into());
        } else {
            handler.bytes(response.url, Bytes::from(body));
        }
    }

    fn on_document(&mut self, result: Result<Response, NetError>) {
        let url = self.url.clone().unwrap_or_default();
        match result {
            Ok(r) => {
                self.url = Some(r.url.clone());
                self.open_document(&r.url, &r.body);
            }
            Err(NetError::TooLarge) => self.open_placeholder(
                &url,
                &format!(
                    "The page is larger than the {} KB limit.",
                    self.config.limits.document_bytes >> 10
                ),
            ),
            Err(e) => self.open_placeholder(&url, &format!("The page could not be loaded ({e}).")),
        }
    }

    fn open_document(&mut self, url: &str, body: &[u8]) {
        if body.len() > self.config.limits.document_bytes {
            let msg = format!(
                "The page is larger than the {} KB limit.",
                self.config.limits.document_bytes >> 10
            );
            return self.open_placeholder(url, &msg);
        }
        let html = String::from_utf8_lossy(body);
        let doc = self.parse(url, &html);
        let nodes = doc.tree().len();
        if nodes > self.config.limits.dom_nodes {
            drop(doc);
            let msg = format!(
                "The page has {nodes} elements, more than the {} limit.",
                self.config.limits.dom_nodes
            );
            return self.open_placeholder(url, &msg);
        }
        self.install(doc);
    }

    fn open_placeholder(&mut self, url: &str, message: &str) {
        let html = format!(
            "<!doctype html><html><body style=\"font-family:sans-serif;margin:32px;color:#333\">\
             <h2>Page not shown</h2><p>{}</p><p style=\"color:#777\">{}</p></body></html>",
            escape(message),
            escape(url)
        );
        let doc = self.parse(url, &html);
        self.install(doc);
    }

    fn parse(&mut self, url: &str, html: &str) -> BaseDocument {
        // Requests queued by an earlier document are stale.
        lock(&self.queue).clear();
        let config = DocumentConfig {
            viewport: Some(Viewport::new(
                self.config.width.into(),
                self.config.height.into(),
                1.0,
                ColorScheme::Light,
            )),
            base_url: Some(url.to_owned()),
            net_provider: Some(Arc::new(QueueNet(self.queue.clone()))),
            font_ctx: Some(font_ctx()),
            style_threading: StyleThreading::Sequential,
            ..Default::default()
        };
        HtmlDocument::from_html(html, config).into_inner()
    }

    fn install(&mut self, doc: BaseDocument) {
        self.doc = Some(doc);
        self.subresources = 0;
        self.dirty = true;
        self.full = true;
        self.damage.clear();
        self.bounds = Bounds::default();
    }

    /// Restyle and relayout, collecting damage unless a full redraw is due anyway.
    fn resolve(&mut self) {
        let Some(doc) = self.doc.as_mut() else {
            self.dirty = false;
            return;
        };
        // `resolve()` would ingest loaded resources itself, after the point where damage
        // is read; ingest them first so their restyle is seen.
        doc.handle_messages();
        if doc.has_pending_critical_resources() {
            return;
        }
        let now = self.start.elapsed().as_secs_f64();
        let track = !self.full;
        let mut restyled = Vec::new();
        if track {
            doc.resolve_stylist(now);
            for (id, node) in doc.tree().iter() {
                if node.damage().is_some_and(|d| !d.is_empty()) {
                    restyled.push(box_owner(doc, id));
                }
            }
            self.damage
                .extend(restyled.iter().filter_map(|id| self.bounds.get(*id)));
        }
        doc.resolve(now);
        let bounds = collect_bounds(doc);
        if track {
            self.damage
                .extend(restyled.iter().filter_map(|id| bounds.get(*id)));
            self.bounds.diff(&bounds, &mut self.damage);
            self.damage.retain(|r| !r.is_empty());
        }
        self.bounds = bounds;
        self.dirty = doc.is_animating();
    }

    /// Re-raster the damaged screen area band by band, passing each written rect and its
    /// RGB565 pixels (row-major, `rect.width` per row) to `sink`.
    pub fn render<E>(
        &mut self,
        mut sink: impl FnMut(Rect, &[u16]) -> Result<(), E>,
    ) -> Result<DamageRects, E> {
        let mut out = DamageRects::default();
        let Some(doc) = self.doc.as_mut() else {
            return Ok(out);
        };
        if doc.has_pending_critical_resources() {
            return Ok(out);
        }
        let (w, h) = (f32::from(self.config.width), f32::from(self.config.height));
        let scroll = doc.viewport_scroll();
        let screen = PageRect {
            x0: 0.0,
            y0: 0.0,
            x1: w,
            y1: h,
        };
        let regions: Vec<PageRect> = if self.full {
            vec![screen]
        } else {
            self.damage
                .iter()
                .map(|r| PageRect {
                    x0: r.x0 - scroll.x as f32,
                    y0: r.y0 - scroll.y as f32,
                    x1: r.x1 - scroll.x as f32,
                    y1: r.y1 - scroll.y as f32,
                })
                .collect()
        };
        self.full = false;
        self.damage.clear();

        let band = self.config.band_height.max(1);
        let mut result = Ok(());
        for y0 in (0..self.config.height).step_by(band.into()) {
            let y1 = (y0 + band).min(self.config.height);
            let Some(r) = band_span(&regions, y0, y1, self.config.width) else {
                continue;
            };
            raster(
                doc,
                &mut self.renderer,
                &mut self.rgba,
                &mut self.px,
                scroll,
                r,
            );
            if let Err(e) = sink(r, &self.px) {
                // The target is now partly stale; redraw everything next time.
                self.full = true;
                result = Err(e);
                break;
            }
            out.push(r);
        }
        doc.set_viewport_scroll(scroll);
        result.map(|()| out)
    }

    #[cfg(test)]
    pub fn doc_mut(&mut self) -> &mut BaseDocument {
        self.dirty = true;
        self.doc.as_mut().expect("document loaded")
    }
}

/// The part of band `y0..y1` covered by `regions`: one rect spanning the union of their
/// x ranges, clipped to the band and the screen.
fn band_span(regions: &[PageRect], y0: u16, y1: u16, width: u16) -> Option<Rect> {
    let (fy0, fy1) = (f32::from(y0), f32::from(y1));
    let mut span: Option<(f32, f32, f32, f32)> = None;
    for r in regions {
        if r.y1 <= fy0 || r.y0 >= fy1 || r.x1 <= 0.0 || r.x0 >= f32::from(width) {
            continue;
        }
        let s = (r.x0.max(0.0), r.y0.max(fy0), r.x1, r.y1.min(fy1));
        span = Some(match span {
            None => s,
            Some(a) => (a.0.min(s.0), a.1.min(s.1), a.2.max(s.2), a.3.max(s.3)),
        });
    }
    let (x0, sy0, x1, sy1) = span?;
    let x0 = x0.floor() as u16;
    let x1 = (x1.ceil() as u16).min(width);
    let sy0 = sy0.floor() as u16;
    let sy1 = (sy1.ceil() as u16).min(y1);
    (x1 > x0 && sy1 > sy0).then_some(Rect {
        x: x0,
        y: sy0,
        width: x1 - x0,
        height: sy1 - sy0,
    })
}

/// Paint screen rect `r` into `px` as RGB565, composited over white.
fn raster(
    doc: &mut BaseDocument,
    renderer: &mut Option<(u16, u16, VelloCpuImageRenderer)>,
    rgba: &mut Vec<u8>,
    px: &mut Vec<u16>,
    scroll: blitz_dom::Point<f64>,
    r: Rect,
) {
    let (w, h) = (r.width, r.height);
    doc.set_viewport_scroll(blitz_dom::Point {
        x: scroll.x + f64::from(r.x),
        y: scroll.y + f64::from(r.y),
    });
    let renderer = match renderer {
        Some((rw, rh, rend)) if *rw == w && *rh == h => rend,
        slot => {
            &mut slot
                .insert((w, h, VelloCpuImageRenderer::new(w.into(), h.into())))
                .2
        }
    };
    ImageRenderer::reset(renderer);
    let n = usize::from(w) * usize::from(h);
    rgba.clear();
    rgba.resize(n * 4, 0);
    renderer.render(
        |scene| blitz_paint::paint_scene(scene, doc, 1.0, w.into(), h.into(), 0, 0),
        rgba,
    );
    px.clear();
    px.extend(rgba.chunks_exact(4).map(|p| {
        // Premultiplied RGBA over an opaque white canvas.
        let over = |c: u8| u32::from(c) + 255 - u32::from(p[3]);
        to_rgb565(over(p[0]), over(p[1]), over(p[2]))
    }));
}

fn to_rgb565(r: u32, g: u32, b: u32) -> u16 {
    let q = |c: u32, max: u32| (c.min(255) * max + 127) / 255;
    ((q(r, 31) << 11) | (q(g, 63) << 5) | q(b, 31)) as u16
}

/// The node whose box carries `id`'s paint: inline spans and text have no box of their
/// own; their inline root does.
fn box_owner(doc: &BaseDocument, mut id: NodeId) -> NodeId {
    while let Some(node) = doc.get_node(id) {
        if !(node.is_text_node() || node.is_non_atomic_inline()) {
            break;
        }
        match node.parent {
            Some(p) => id = p,
            None => break,
        }
    }
    id
}

fn has_box(node: &Node) -> bool {
    (node.is_element() || node.is_anonymous()) && !node.is_non_atomic_inline()
}

fn collect_bounds(doc: &BaseDocument) -> Bounds {
    let mut bounds = Bounds::default();
    for (id, node) in doc.tree().iter() {
        if has_box(node) {
            bounds.insert(id, visual_rect(node));
        }
    }
    bounds
}

/// Border box in page coordinates, grown by outset shadows and the outline.
fn visual_rect(node: &Node) -> PageRect {
    let p = node.absolute_position(0.0, 0.0);
    let s = node.final_layout().size;
    let mut r = PageRect {
        x0: p.x,
        y0: p.y,
        x1: p.x + s.width,
        y1: p.y + s.height,
    };
    if r.is_empty() {
        return r;
    }
    let mut grow = 1.0f32; // antialiased edges
    if let Some(style) = node.primary_styles() {
        for shadow in style.get_effects().box_shadow.0.iter() {
            let b = &shadow.base;
            // A gaussian with std dev blur/2 is visible to about 3 std devs.
            let reach = b.horizontal.px().abs().max(b.vertical.px().abs())
                + shadow.spread.px().max(0.0)
                + 1.5 * b.blur.px();
            grow = grow.max(reach + 1.0);
        }
        let outline = style.get_outline();
        let reach =
            outline.outline_width.0.to_f32_px() + outline.outline_offset.to_f32_px().max(0.0);
        grow = grow.max(reach + 1.0);
    }
    r.x0 -= grow;
    r.y0 -= grow;
    r.x1 += grow;
    r.y1 += grow;
    r
}

fn font_ctx() -> FontContext {
    let mut ctx = FontContext {
        source_cache: SourceCache::new_shared(),
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
    };
    let mut families = Vec::new();
    for bytes in embedded_browser_assets::FONTS {
        let registered = ctx
            .collection
            .register_fonts(Blob::new(Arc::new(*bytes) as _), None);
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

fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    data_url::DataUrl::process(url)
        .ok()?
        .decode_to_vec()
        .ok()
        .map(|(body, _)| body)
}

fn is_image(bytes: &[u8]) -> bool {
    const MAGIC: [&[u8]; 4] = [b"\x89PNG", b"\xff\xd8\xff", b"GIF8", b"RIFF"];
    MAGIC.iter().any(|m| bytes.starts_with(m))
}

/// Width times height from the image header, without decoding.
fn image_pixels(bytes: &[u8]) -> Option<u64> {
    let (w, h) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some(u64::from(w) * u64::from(h))
}

/// A 1x1 light grey PNG that stands in for an image over the caps.
fn placeholder_image() -> Vec<u8> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(&[0xd0, 0xd0, 0xd0], 1, 1, image::ExtendedColorType::Rgb8)
        .expect("encode placeholder");
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
