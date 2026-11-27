# 0003. Core API on a single DOM thread

Status: Proposed (becomes Accepted at the M1 side-by-side review)
Date: 2026-11-27 (revised in M1 slice 1)

## Context
The core must run on a device with one engine thread and no blocking I/O in the render path, and leave room for a JS engine later without an API break. M0 (docs/m0/host-results.md) found:
- Blitz exposes restyle damage (`Node::damage()`), readable only between `resolve_stylist()` and `resolve()`. Relayout side effects (an insertion that grows a grid row) carry no damage.
- `paint_scene`'s offsets only add, so a band is selected by scrolling the viewport to it. Blitz then culls to the band.
- Per-band fixed cost makes full-width band re-raster too slow for H1's ≤100 ms highlight, so damaged bands are also clipped in x.

## Decision
- `Browser<N: Net>` with `new(Config, net)`, `load`, `tick() -> bool`, `is_idle`, `invalidate`, `render(&mut impl DrawTarget<Color = Rgb565>) -> Result<DamageRects, _>` and `render_raw(&mut [u16], stride) -> DamageRects`. The public API has no Blitz, Stylo or http types.
- `Net` is non-blocking: `fetch(Request) -> ReqId`, `poll() -> Option<(ReqId, Result<Response, NetError>)>`, `cancel`. `tick()` drains completions; the main document goes through it too, `data:` URLs are handled in the core. `Request::max_bytes` lets the implementation stop reading early. The same trait will serve JS fetch; WebSocket methods come with JS.
- `Limits` caps: document bytes, DOM nodes (both give a placeholder page), subresource bytes (dropped), image bytes and header-checked image pixels (a grey placeholder image), subresource count.
- Damage = restyle damage of each element (mapped to its inline root when it has no box), plus a diff of every box's absolute visual rect (border box grown by outset shadows and outline) against the previous resolve. A tick that delivers a resource redraws the full screen. `render` re-rasters, per band, one rect spanning the damage in that band, clipped in x and y, and returns the rects written.
- Still planned, shapes deferred to their slices: `touch`, generational `NodeHandle`, `EventSink` before default actions, `ScriptHost::on_script`, `BlobStore` (added as a defaulted type parameter `S = NoStore`, which is not a break), and the PSRAM strip cache for scrolling.

## Consequences
- JS (v2) plugs in through the sink, script hook and `Net` without breaking the API.
- No locking in the engine; slow rasterization is bounded per band.
- Measured on the dashboard: a class toggle re-rasters 9 % of the screen, a text edit 0.5 %, a list insertion that reflows the page 85 %. The incremental frame equals a full render in all three.
- Peak heap through this API on wasm32 (engine plus 32px bands, framebuffer excluded): dashboard 1.37 MB, article 3.90 MB, links 1.91 MB. The CI `heap-cap` job fails above 12 MB.
- Known limit: `position: fixed` children of the root cancel the viewport scroll, so banding repeats them in every band. None of the corpus uses it; the strip-cache slice has to handle it.
