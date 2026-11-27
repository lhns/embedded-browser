//! M0 emulation check: draws a deterministic test pattern into a PSRAM back buffer through an
//! embedded-graphics `DrawTarget<Color = Rgb565>`, flushes it to Espressif's virtual QEMU RGB
//! panel and prints `READY` on the serial console.

use std::convert::Infallible;

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_idf_svc::sys::{self, qemu_rgb};

const W: u32 = 320;
const H: u32 = 240;

/// RGB565 framebuffer over a raw (PSRAM) allocation.
struct Framebuffer {
    px: &'static mut [u16],
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(W, H)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(p, c) in pixels {
            if (0..W as i32).contains(&p.x) && (0..H as i32).contains(&p.y) {
                self.px[(p.y as u32 * W + p.x as u32) as usize] = RawU16::from(c).into_inner();
            }
        }
        Ok(())
    }
}

/// Fixed pattern: 8 colour bars, a grey ramp, a frame and text. No clocks, so it is bit-exact.
fn draw_pattern(t: &mut Framebuffer) {
    let bars = [
        Rgb565::WHITE,
        Rgb565::YELLOW,
        Rgb565::CYAN,
        Rgb565::GREEN,
        Rgb565::MAGENTA,
        Rgb565::RED,
        Rgb565::BLUE,
        Rgb565::BLACK,
    ];
    let bw = W / 8;
    for (i, c) in bars.iter().enumerate() {
        let _ = Rectangle::new(Point::new(i as i32 * bw as i32, 0), Size::new(bw, 120))
            .into_styled(PrimitiveStyle::with_fill(*c))
            .draw(t);
    }
    for x in 0..W {
        let g = (x * 31 / (W - 1)) as u8;
        let c = Rgb565::new(g, g * 2, g);
        let _ = Rectangle::new(Point::new(x as i32, 120), Size::new(1, 40))
            .into_styled(PrimitiveStyle::with_fill(c))
            .draw(t);
    }
    let _ = Rectangle::new(Point::new(0, 160), Size::new(W, H - 160))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::new(2, 4, 8)))
        .draw(t);
    let _ = Rectangle::new(Point::zero(), Size::new(W, H))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
        .draw(t);
    let white = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let _ = Text::new("embedded-browser", Point::new(20, 190), white).draw(t);
    let _ = Text::new("ESP32-S3 QEMU test", Point::new(20, 215), white).draw(t);
}

/// Allocates from PSRAM, writes and verifies a pattern; returns the pointer.
fn psram_alloc(bytes: usize) -> *mut u8 {
    // SAFETY: plain heap_caps allocation, checked for null below.
    let p = unsafe { sys::heap_caps_malloc(bytes, sys::MALLOC_CAP_SPIRAM) } as *mut u8;
    assert!(!p.is_null(), "PSRAM allocation failed");
    p
}

fn main() {
    sys::link_patches();
    let t0 = unsafe { sys::esp_timer_get_time() };

    // PSRAM proof: size reported by the driver plus a write/readback over a 1 MiB block.
    let psram_size = unsafe { sys::esp_psram_get_size() };
    let probe = psram_alloc(1 << 20);
    let probe_ok = unsafe {
        let s = std::slice::from_raw_parts_mut(probe, 1 << 20);
        for (i, b) in s.iter_mut().enumerate() {
            *b = (i as u32).wrapping_mul(2654435761) as u8;
        }
        s.iter()
            .enumerate()
            .all(|(i, b)| *b == (i as u32).wrapping_mul(2654435761) as u8)
    };
    unsafe { sys::heap_caps_free(probe as *mut _) };
    println!(
        "PSRAM size={psram_size} probe_ok={probe_ok} free_spiram={}",
        unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM) }
    );
    assert!(psram_size > 0 && probe_ok);

    // Back buffer in PSRAM.
    let bytes = (W * H) as usize * 2;
    let buf = psram_alloc(bytes) as *mut u16;
    let mut fb = Framebuffer {
        px: unsafe { std::slice::from_raw_parts_mut(buf, (W * H) as usize) },
    };
    fb.px.fill(0);
    draw_pattern(&mut fb);

    // Panel: 16 bpp (RGB565), flush the whole back buffer.
    let mut panel: qemu_rgb::esp_lcd_panel_handle_t = std::ptr::null_mut();
    let cfg = qemu_rgb::esp_lcd_rgb_qemu_config_t {
        width: W,
        height: H,
        bpp: qemu_rgb::esp_lcd_rgb_qemu_bpp_t_RGB_QEMU_BPP_16,
    };
    unsafe {
        sys::esp!(qemu_rgb::esp_lcd_new_rgb_qemu(&cfg, &mut panel)).unwrap();
        sys::esp!(qemu_rgb::esp_lcd_panel_reset(panel)).unwrap();
        sys::esp!(qemu_rgb::esp_lcd_panel_init(panel)).unwrap();
        // QEMU's panel DMA cannot read PSRAM-mapped addresses ("Invalid color content address"), so
        // copy the PSRAM back buffer into the panel's dedicated framebuffer and trigger a refresh.
        let mut qfb: *mut core::ffi::c_void = std::ptr::null_mut();
        sys::esp!(qemu_rgb::esp_lcd_rgb_qemu_get_frame_buffer(panel, &mut qfb)).unwrap();
        std::ptr::copy_nonoverlapping(buf as *const u8, qfb as *mut u8, bytes);
        sys::esp!(qemu_rgb::esp_lcd_rgb_qemu_refresh(panel)).unwrap();
    }
    let ms = (unsafe { sys::esp_timer_get_time() } - t0) / 1000;
    println!("app_main to flush: {ms} ms");
    println!("READY");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
