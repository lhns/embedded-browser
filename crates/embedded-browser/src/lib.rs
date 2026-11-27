//! Web renderer for ESP-IDF std targets (ESP32-P4 first, then S3), built on Servo components via Blitz.
//!
//! Scope: HTML/CSS rendering. No JavaScript in v1. Engine-neutral, no ESP dependencies.
//!
//! One [`Browser`] runs on one thread (ADR 0003): [`Browser::load`] starts a navigation,
//! [`Browser::tick`] drains network completions and restyles/relayouts, and
//! [`Browser::render`] re-rasterizes only what changed, in horizontal bands, as RGB565.
//!
//! ```no_run
//! use embedded_browser::{Browser, Config, FileNet};
//!
//! let mut fb = vec![0u16; 1024 * 600];
//! let mut browser = Browser::new(Config::default(), FileNet::default());
//! browser.load("file:///sd/index.html");
//! while !browser.is_idle() {
//!     browser.tick();
//! }
//! let damage = browser.render_raw(&mut fb, 1024);
//! for rect in damage.iter() {
//!     // push `rect` of `fb` to the panel
//! }
//! ```

mod engine;
mod net;

#[cfg(test)]
mod tests;

pub use embedded_graphics_core;
pub use net::{FileNet, Method, Net, NetError, ReqId, Request, Response};

use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::raw::RawU16;
use embedded_graphics_core::prelude::{Point, Size};
use embedded_graphics_core::primitives::Rectangle;

/// Screen and engine settings.
#[derive(Clone, Debug)]
pub struct Config {
    /// Screen width in pixels (CSS px at scale 1).
    pub width: u16,
    /// Screen height in pixels.
    pub height: u16,
    /// Height of a raster band. Each band is painted separately, so the raster buffer is
    /// `width * band_height` pixels instead of a full frame (ADR 0002).
    pub band_height: u16,
    /// Resource caps; content over a cap becomes a placeholder.
    pub limits: Limits,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 600,
            band_height: 32,
            limits: Limits::default(),
        }
    }
}

/// Caps that turn oversized content into placeholders instead of exhausting the heap.
#[derive(Clone, Debug)]
pub struct Limits {
    /// HTML document body. Over the cap, a placeholder page is shown.
    pub document_bytes: usize,
    /// Stylesheets, fonts and other non-image subresources. Over the cap, the resource is
    /// dropped (an oversized stylesheet does not apply).
    pub subresource_bytes: usize,
    /// Encoded image size. Over the cap, a grey placeholder image is used.
    pub image_bytes: usize,
    /// Decoded image size in pixels, checked from the image header before decoding.
    pub image_pixels: u64,
    /// Subresource requests per document; further requests fail.
    pub subresources: usize,
    /// DOM nodes after parsing. Over the cap, a placeholder page is shown.
    pub dom_nodes: usize,
}

impl Default for Limits {
    /// Sized for the P4 tier (≤12 MB peak heap). The M0 Wikipedia page, at 10.5 MB, fits:
    /// a 208 KB stylesheet, 6 k nodes and 1.8 MB of decoded images.
    fn default() -> Self {
        Self {
            document_bytes: 2 << 20,
            subresource_bytes: 512 << 10,
            image_bytes: 1 << 20,
            image_pixels: 1024 * 1024,
            subresources: 128,
            dom_nodes: 20_000,
        }
    }
}

/// A screen rectangle in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn area(&self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.width && y - self.y < self.height
    }
}

/// The screen rectangles a [`Browser::render`] call wrote, in top-to-bottom order and
/// non-overlapping. Only these need to go to the panel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DamageRects(Vec<Rect>);

impl DamageRects {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rect> {
        self.0.iter()
    }

    pub fn as_slice(&self) -> &[Rect] {
        &self.0
    }

    /// Total pixels written.
    pub fn area(&self) -> u32 {
        self.0.iter().map(Rect::area).sum()
    }

    /// Append a band's rect, merging it into the previous one when it continues it
    /// downwards with the same x span.
    fn push(&mut self, r: Rect) {
        if let Some(last) = self.0.last_mut()
            && last.x == r.x
            && last.width == r.width
            && last.y + last.height == r.y
        {
            last.height += r.height;
            return;
        }
        self.0.push(r);
    }
}

/// A web page on a screen, driven from one thread.
pub struct Browser<N: Net> {
    engine: engine::Engine,
    net: N,
}

impl<N: Net> Browser<N> {
    pub fn new(config: Config, net: N) -> Self {
        Self {
            engine: engine::Engine::new(config),
            net,
        }
    }

    /// Start loading `url`. Returns at once; the page appears over later [`tick`](Self::tick)s.
    /// `data:` URLs are handled internally, everything else goes through the [`Net`].
    pub fn load(&mut self, url: &str) {
        self.engine.load(&mut self.net, url);
    }

    /// Drain network completions, then restyle and relayout if anything changed.
    /// Returns whether [`render`](Self::render) has something to draw.
    pub fn tick(&mut self) -> bool {
        self.engine.tick(&mut self.net)
    }

    /// No requests in flight and nothing left to restyle or lay out.
    pub fn is_idle(&self) -> bool {
        self.engine.is_idle()
    }

    /// The URL of the current document, after redirects.
    pub fn url(&self) -> Option<&str> {
        self.engine.url()
    }

    /// Make the next render redraw the whole screen, e.g. after the panel lost its contents.
    /// The first render after a load is always full.
    pub fn invalidate(&mut self) {
        self.engine.invalidate();
    }

    /// Re-raster the damaged parts of the screen into `target`. Returns the rects written.
    pub fn render<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
    ) -> Result<DamageRects, D::Error> {
        self.engine.render(|r, px| {
            let area = Rectangle::new(
                Point::new(r.x.into(), r.y.into()),
                Size::new(r.width.into(), r.height.into()),
            );
            target.fill_contiguous(&area, px.iter().map(|&p| Rgb565::from(RawU16::new(p))))
        })
    }

    /// Like [`render`](Self::render), into a raw RGB565 framebuffer (native-endian `u16`,
    /// `r << 11 | g << 5 | b`) of `stride` pixels per row and at least `height` rows.
    pub fn render_raw(&mut self, fb: &mut [u16], stride: usize) -> DamageRects {
        let height = usize::from(self.engine.config().height);
        let width = usize::from(self.engine.config().width);
        assert!(stride >= width && fb.len() >= stride * (height - 1) + width);
        let result: Result<_, std::convert::Infallible> = self.engine.render(|r, px| {
            let (x, w) = (usize::from(r.x), usize::from(r.width));
            for (row, src) in px.chunks_exact(w).enumerate() {
                let start = (usize::from(r.y) + row) * stride + x;
                fb[start..start + w].copy_from_slice(src);
            }
            Ok(())
        });
        match result {
            Ok(d) => d,
        }
    }

    pub fn net(&self) -> &N {
        &self.net
    }

    pub fn net_mut(&mut self) -> &mut N {
        &mut self.net
    }
}
