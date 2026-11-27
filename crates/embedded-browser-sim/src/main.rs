//! Host simulator for `embedded-browser`.
//!
//! ```text
//! embedded-browser-sim --headless <url-or-file> --out page.png [--size 1024x600]
//! embedded-browser-sim <url-or-file> [--size 1024x600]     (window; build with --features window)
//! ```

mod net;

use std::time::{Duration, Instant};

use embedded_browser::{Browser, Config, Net};

use crate::net::SimNet;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        eprintln!(
            "usage: embedded-browser-sim --headless <url-or-file> --out page.png [--size WxH]\n       \
             embedded-browser-sim <url-or-file> [--size WxH]   (needs --features window)"
        );
        std::process::exit(2);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let mut config = Config::default();
    if let Some(size) = opt("--size") {
        let (w, h) = size
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            .ok_or_else(|| format!("bad --size {size}"))?;
        config.width = w;
        config.height = h;
    }
    if let Some(target) = opt("--headless") {
        let out = opt("--out").ok_or("--headless needs --out")?;
        return headless(config, &to_url(target)?, out);
    }
    let target = args
        .iter()
        .enumerate()
        .find(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--size"))
        .map(|(_, a)| a.as_str())
        .ok_or("no page given")?;
    window(config, &to_url(target)?)
}

/// URLs pass through; anything else is a local path.
fn to_url(target: &str) -> Result<String, String> {
    if target.contains("://") || target.starts_with("data:") {
        return Ok(target.to_owned());
    }
    let abs = std::path::absolute(target).map_err(|e| e.to_string())?;
    url::Url::from_file_path(&abs)
        .map(String::from)
        .map_err(|()| format!("not a file path: {target}"))
}

/// Tick until the page has loaded (or `timeout`), sleeping while requests are in flight.
fn settle<N: Net>(browser: &mut Browser<N>, timeout: Duration) {
    let start = Instant::now();
    while !browser.is_idle() && start.elapsed() < timeout {
        browser.tick();
        if !browser.is_idle() {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn headless(config: Config, url: &str, out: &str) -> Result<(), String> {
    let (w, h) = (usize::from(config.width), usize::from(config.height));
    let mut browser = Browser::new(config, SimNet::default());
    browser.load(url);
    settle(&mut browser, Duration::from_secs(60));
    if !browser.is_idle() {
        eprintln!("warning: page still loading after 60 s; rendering what is there");
    }
    let mut fb = vec![0u16; w * h];
    browser.render_raw(&mut fb, w);
    save_png(out, &fb, w, h)
}

/// Write an RGB565 framebuffer as an 8-bit RGB PNG.
pub fn save_png(path: &str, fb: &[u16], w: usize, h: usize) -> Result<(), String> {
    let rgb: Vec<u8> = fb.iter().flat_map(|&p| rgb565_to_rgb888(p)).collect();
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    image::save_buffer(
        path,
        &rgb,
        w as u32,
        h as u32,
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| e.to_string())
}

pub fn rgb565_to_rgb888(p: u16) -> [u8; 3] {
    let (r, g, b) = ((p >> 11) as u8, ((p >> 5) & 0x3f) as u8, (p & 0x1f) as u8);
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

#[cfg(feature = "window")]
fn window(config: Config, url: &str) -> Result<(), String> {
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::Size;
    use embedded_graphics_simulator::{
        OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    };

    let size = Size::new(config.width.into(), config.height.into());
    let mut display = SimulatorDisplay::<Rgb565>::new(size);
    let mut window = Window::new("embedded-browser", &OutputSettingsBuilder::new().build());
    let mut browser = Browser::new(config, SimNet::default());
    browser.load(url);
    loop {
        browser.tick();
        browser.render(&mut display).map_err(|e| format!("{e:?}"))?;
        // The SDL window is created by the first update, before events can be read.
        window.update(&display);
        // Mouse input becomes touch in a later slice; only quitting is handled.
        if window.events().any(|e| matches!(e, SimulatorEvent::Quit)) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(not(feature = "window"))]
fn window(_config: Config, _url: &str) -> Result<(), String> {
    Err("window mode needs SDL2: build with `--features window`, or use --headless".into())
}
