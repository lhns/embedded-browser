# embedded-browser

A web renderer for ESP32 touch panels, built on Servo components (Stylo, html5ever) via [Blitz](https://github.com/DioxusLabs/blitz). Rust, ESP-IDF std targets.

Status: **pre-M0 spike.** Nothing renders yet. This repository is the workspace skeleton, the design decisions and the roadmap. Nobody has run Blitz on a microcontroller, so the first milestone measures whether it fits at all.

## Scope of v1

- Server-rendered HTML/CSS pages render, with touch scrolling, taps, links and simple forms.
- No JavaScript. JS single-page apps will not work in v1.
- A custom dashboard that needs scripts depends on the on-device JS track (QuickJS-ng), which comes later.
- Home Assistant is planned through a remote mode (a VNC-style client talking to a companion browser), not on-device, because Blitz has no shadow DOM or custom elements. Remote mode bypasses this renderer.
- Excluded in v1: JS, canvas, video, shadow DOM.

## Target boards and memory tiers

| Tier | Board | Display | PSRAM |
|---|---|---|---|
| First | ESP32-P4 (Function-EV, Guition JC1060P470) | 1024x600 DSI, GT911 touch | 32 MB |
| Later | ESP32-S3-BOX-3 | SPI | 8 MB |
| Later | ESP32-S3 7" | 800x480 RGB, GT911 touch | 8 MB |

The S3 8 MB tier needs a peak heap of about 4 MB or less; if the engine does not fit, it gets a smaller fallback engine. No boards are on hand yet, so M0 uses host measurement and emulation.

## Layout

- `crates/embedded-browser`: engine-neutral core (no ESP dependencies)
- `crates/embedded-browser-assets`: fonts, UA stylesheet, ICU data
- `crates/embedded-browser-sim`: host simulator
- `tools/measure`: heap and raster measurement
- `boards/`: per-board firmware, outside the host workspace

## Docs

- [Architecture decisions](docs/adr/README.md)
- [Roadmap](docs/roadmap.md), generated from `docs/roadmap/nodes.yaml`

## License

MIT OR Apache-2.0. Stylo stays unmodified under MPL-2.0. See ADR 0005.
