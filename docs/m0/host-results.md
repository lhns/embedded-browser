# M0 host results: peak heap and raster estimate

Date: 2026-11-27. Roadmap nodes `eng-peak-heap` and `eng-raster-estimate`. Tool: `tools/measure`.

## Verdict

| Metric | Result | M0 table | Verdict |
|---|---|---|---|
| Peak heap, P4 (32-bit, engine + 32px bands) | 1.1–3.6 MB on the synthetic pages, 10.5 MB on a real Wikipedia article with its images | go ≤12 MB | **go** on every page; Wikipedia has ~1 MB margin once allocator overhead is added |
| Peak heap, P4, with a full 1024x600 RGBA frame (vello_cpu) | 4.0–6.5 MB synthetic, 13.4 MB Wikipedia | go ≤12 MB, fallback 12–20 MB | **go** on the synthetic pages; **fallback** on Wikipedia, so the full-frame design is out for long pages |
| Peak heap, S3 tier (≤4 MB) | dashboard 1.1, links 1.7, HN 2.1, article 3.6 MB; Wikipedia 10.5 MB | ≤4 MB | **go** for the dashboard and small pages; **no** for long real articles |
| Sequential styling spawns threads | none | — | confirmed (native evidence) |
| Full frame on P4, vello_cpu (provisional) | ~0.2–0.7 s before any PSRAM penalty | H1: go ≤1 s | probably **go**; the dashboard is the risk once PSRAM is counted |
| Full frame on P4, tiny-skia (provisional) | ~0.4–1.3 s on text pages, 3.4–4.8 s on the dashboard | H1: stop >3 s | **stop** for the dashboard, fallback on the rest |
| Damage-limited highlight re-raster on P4 (provisional) | ~0.27–0.45 s for the 8 full-width bands of one card | H1: go ≤100 ms, fallback ≤300 ms | **fallback/stop border**; contradicts the planned band design unless bands are clipped in x and the per-band walk gets cheaper |
| Banded `paint_scene` | correct via viewport scroll: ≤6 differing pixels per page with vello_cpu | — | works |
| Damage source | `RestyleDamage` per element, readable between `resolve_stylist` and `resolve`; misses relayout side effects | — | exists; ADR 0002/0003 need correcting |

Per-page heap verdicts, using the 32-bit engine plus 32px vello_cpu bands, which is the planned device setup:

| Page | Peak MB | P4 (≤12 go, 12–20 fallback, >20 stop) | S3 tier (≤4 MB) |
|---|---|---|---|
| dashboard (HA-like, synthetic) | 1.10 | go | go |
| article (synthetic) | 3.58 | go | go, tight |
| links (synthetic) | 1.72 | go | go |
| hn-front (fetched) | 2.14 | go | go |
| wikipedia-home-automation (fetched, with `<img>` images) | 10.52 | go (≈11.3 with allocator overhead) | no |

## Setup

- **Blitz:** `blitz-dom`, `blitz-html`, `blitz-paint` and `blitz-traits` `=0.3.0-beta.6`, all with `default-features = false`.
  - `floats` is enabled; it is not in Blitz's defaults, and articles need it.
  - The rasterizers are `anyrender_vello_cpu` 0.19.0 and `anyrender_tiny_skia` 0.1.1. Both use `anyrender` 0.15.1, the version `blitz-paint` requires.
- **Fonts:** Noto Sans Regular and Bold from the `ttf-noto-sans` crate (the crate ships the fonts with an Apache-2.0 licence file), registered as `&'static` blobs. No system fonts.
  - The 400 KB of font data stays out of the heap, as it would when mapped from a flash partition.
- **Net:** a local `NetProvider` serves only `data:` and `file:` URLs, synchronously.
  - `image` is added with `png`, `jpeg` and `gif`, because Blitz pulls it in with no codecs.
- **Styling:** `StyleThreading::Sequential` (the 0.3.0-beta.6 default). Viewport 1024x600 at scale 1.
- **Corpus:**
  - Synthetic: `testpages/dashboard.html`, `article.html` (~32 KB of text, floats, tables, three PNG data URIs) and `links.html` (forms).
  - Fetched, not committed: `testpages/fetch.sh` gets a Wikipedia article and the HN front page with their stylesheets and `<img>` sources (25 for Wikipedia: PNG, JPEG, GIF; its 6 SVG images do not decode with `svg` off). CSS background images are not fetched.
- **Host:** a QEMU virtual CPU (model hidden, 2.3 GHz, 16 vCPUs), Windows 11, Rust 1.95, Node 25.8.

### Features that stayed on

| Feature | Pulled in by | Why it can't be turned off |
|---|---|---|
| `stylo/servo` (serde, url, mime, encoding_rs, string_cache) | `blitz-dom`, `stylo_taffy` (`stylo` with default features) | hard-coded in Blitz's manifest |
| `icu_segmenter` `compiled_data` + `auto` + `lstm` | `stylo` | line breaking; LSTM data for SE-Asian scripts is in flash, not heap |
| `icu_properties`, `icu_normalizer` `compiled_data` | `blitz-dom`, `parley` | needed for text |
| `rayon` (linked, never started) | `blitz-dom`, `stylo` | hard dependency; only used for `Parallel` styling and `parallel-construct` |
| `tiny-skia/simd` | `anyrender_tiny_skia` (default features) | no way to drop it from outside |
| `vello_cpu` runtime SIMD detection | `fearless_simd` 1.3 `std` | `anyrender_vello_cpu` builds `RenderContext::new` itself and exposes no `Level` |

Features that were turned off: `system-fonts`, `text-transform-icu`, `svg` (without it inline SVG is expected not to paint, so the dashboard uses CSS icons; not tested), `woff`, `accessibility`, `custom-widget`, `file-input`, `complex-scripts`, `scrollbars` and vello_cpu `multithreading`. `tools/measure` has an `svg` feature for later delta measurements; it is not measured here.

## Peak heap by category

- **Method:** a counting `GlobalAlloc` (`tools/measure/src/alloc.rs`) with attribution by **phase**, not per allocation site.
  - Each phase sets the current category. Every block gets a 1-byte tag in a header, so a free is charged to the category that allocated it. A `realloc` moves the block to the current phase.
  - "Peak" is the highest live total. The table gives each category's live bytes at that moment.
- **Categories:**
  - **text/fonts:** the font collection only. Parley shaping and text layout run inside Blitz's `resolve`, so they count under **dom+style+layout**.
  - **images:** decoded images. The `NetProvider` sniffs PNG/JPEG/GIF/WebP magic and runs the handler's decode in this phase.
  - **raster:** scene building, rasterizer state, glyph outlines and pixel buffers.
  - **other:** the page source and the runtime. There is no network category: nothing goes over the network on the host.
- **Isolation:** each row is a fresh process, so one-time globals (UA stylesheet, Stylo statics) are counted.
- **Not counted:**
  - Allocator overhead: about 51 k live blocks on Wikipedia, about 8–16 B each on ESP-IDF, so roughly 0.4–0.8 MB.
  - Images are decoded at their intrinsic size; Wikipedia's 1.8 MB of images would shrink with decode-at-display-size.
  - Stacks, TLS and network buffers.

**32-bit target.** `i686-pc-windows-msvc` does not build. `anyrender` 0.15.1 asserts `size_of::<FilterEffect>() == 84` on 32-bit x86 (`src/filters.rs:25`), but MSVC aligns `u64` to 8, which gives 88. The gnu variant is expected to fail the same way (Windows aligns `u64` to 8 there too); not tried.

So the 32-bit numbers come from **`wasm32-wasip1`** run under Node's WASI. Its data layout (4-byte pointers, 8-byte-aligned `u64`) matches `riscv32imafc` better than i686 does anyway.

### 32-bit (wasm32-wasip1), MB

| page | config | DOM nodes | engine peak | total peak | at peak: dom+style+layout / text / images / raster / other | blocks at peak |
|---|---|---|---|---|---|---|
| dashboard | engine only | 299 | 0.79 | 0.79 | 0.78 / 0.00 / 0.00 / 0.00 / 0.01 | 4410 |
| dashboard | vello_cpu full frame | 299 | 0.79 | 4.01 | 0.78 / 0.00 / 0.00 / 3.23 / 0.00 | 5176 |
| dashboard | vello_cpu 32px bands | 299 | 0.79 | 1.10 | 0.78 / 0.00 / 0.00 / 0.33 / 0.00 | 4599 |
| dashboard | tiny-skia full frame | 299 | 0.79 | 8.45 | 0.78 / 0.00 / 0.00 / 7.67 / 0.00 | 4669 |
| dashboard | tiny-skia 32px bands | 299 | 0.79 | 1.38 | 0.78 / 0.00 / 0.00 / 0.60 / 0.00 | 4507 |
| article | engine only | 607 | 3.21 | 3.21 | 2.97 / 0.00 / 0.20 / 0.00 / 0.04 | 6203 |
| article | vello_cpu full frame | 607 | 3.21 | 6.52 | 2.96 / 0.00 / 0.20 / 3.36 / 0.00 | 6929 |
| article | vello_cpu 32px bands | 607 | 3.21 | 3.58 | 2.96 / 0.00 / 0.20 / 0.42 / 0.00 | 6446 |
| article | tiny-skia full frame | 607 | 3.21 | 8.21 | 2.96 / 0.00 / 0.20 / 5.05 / 0.00 | 6567 |
| article | tiny-skia 32px bands | 607 | 3.21 | 3.72 | 2.96 / 0.00 / 0.20 / 0.56 / 0.00 | 6359 |
| links | engine only | 848 | 1.47 | 1.47 | 1.46 / 0.00 / 0.00 / 0.00 / 0.01 | 6504 |
| links | vello_cpu full frame | 848 | 1.47 | 4.59 | 1.42 / 0.00 / 0.00 / 3.17 / 0.00 | 7159 |
| links | vello_cpu 32px bands | 848 | 1.47 | 1.72 | 1.42 / 0.00 / 0.00 / 0.30 / 0.00 | 6774 |
| links | tiny-skia full frame | 848 | 1.47 | 6.35 | 1.42 / 0.00 / 0.00 / 4.93 / 0.00 | 6830 |
| links | tiny-skia 32px bands | 848 | 1.47 | 1.88 | 1.42 / 0.00 / 0.00 / 0.46 / 0.00 | 6720 |
| hn-front | engine only | 1216 | 1.86 | 1.86 | 1.83 / 0.00 / 0.00 / 0.00 / 0.03 | 9604 |
| hn-front | vello_cpu full frame | 1216 | 1.86 | 5.03 | 1.78 / 0.00 / 0.00 / 3.26 / 0.00 | 10377 |
| hn-front | vello_cpu 32px bands | 1216 | 1.86 | 2.14 | 1.78 / 0.00 / 0.00 / 0.36 / 0.00 | 9947 |
| hn-front | tiny-skia full frame | 1216 | 1.86 | 6.86 | 1.78 / 0.00 / 0.00 / 5.09 / 0.00 | 9988 |
| hn-front | tiny-skia 32px bands | 1216 | 1.86 | 2.37 | 1.78 / 0.00 / 0.00 / 0.59 / 0.00 | 9818 |
| wikipedia-home-automation | engine only | 6109 | 10.42 | 10.42 | 8.29 / 0.00 / 1.82 / 0.00 / 0.31 | 50978 |
| wikipedia-home-automation | vello_cpu full frame | 6109 | 10.42 | 13.41 | 8.29 / 0.00 / 1.82 / 3.29 / 0.00 | 51645 |
| wikipedia-home-automation | vello_cpu 32px bands | 6109 | 10.42 | 10.52 | 8.29 / 0.00 / 1.82 / 0.41 / 0.00 | 51178 |
| wikipedia-home-automation | tiny-skia full frame | 6109 | 10.42 | 17.44 | 8.29 / 0.00 / 1.82 / 7.32 / 0.00 | 51215 |
| wikipedia-home-automation | tiny-skia 32px bands | 6109 | 10.42 | 10.70 | 8.29 / 0.00 / 1.82 / 0.59 / 0.00 | 51033 |

### 64-bit (x86_64-pc-windows-msvc), MB, for comparison

| page | engine peak | vello full | vello bands | tiny full | tiny bands |
|---|---|---|---|---|---|
| dashboard | 1.01 | 4.27 | 1.34 | 8.68 | 1.60 |
| article | 3.55 | 6.93 | 3.95 | 8.56 | 4.06 |
| links | 1.81 | 4.99 | 2.07 | 6.68 | 2.22 |
| hn-front | 2.35 | 5.59 | 2.63 | 7.36 | 2.86 |
| wikipedia-home-automation | 12.56 | 15.58 | 12.67 | 19.58 | 12.84 |

### Observations

- **DOM, style and layout dominate.** Engine heap tracks the stylesheet more than the node count. Wikipedia's 208 KB stylesheet plus 6 k nodes reach 8.3 MB, plus 1.8 MB of decoded images.
  - 64-bit adds about 20–30 %.
- **vello_cpu raster** in 32px bands costs 0.3–0.4 MB. A full frame costs about 3.2 MB, of which 2.4 MB is the RGBA buffer.
- **tiny-skia** needs 5–7.7 MB for a full frame, because of its layer pixmaps for shadows, gradients and clips. Even in bands it uses about twice vello's heap.
- **Images:** three 160x110 PNGs decode to 0.2 MB on the article; Wikipedia's thumbnails take 1.8 MB. That supports the plan's "decode at display size into a bounded cache".

## Threads

- **Native, rayon's global pool:** after each native run, `rayon::ThreadPoolBuilder::build_global()` returned `Ok` in all 25 runs, so the global pool was never initialized.
- **Native, OS thread count:** measured with Toolhelp32 before and after each run, it moved by ±1 in both directions. These are Windows loader and thread-pool workers that exist before Blitz runs. A rayon pool on this 16-vCPU host would have added about 16 threads.
- **wasm32-wasip1:** every run finished there too. That shows the engine works without threads, but it is not proof on its own: rayon falls back to the current thread when spawning is unsupported (`rayon-core` `default_global_registry`).
- **Code:** Stylo's own pool is only touched for `Parallel` (`blitz-dom-0.3.0-beta.6/src/stylo/traversal.rs:129`).

## Raster time

The command is `measure raster --iters 7`. It reports the median of 7 warm frames; the cold column is the first frame.

- **paint** is Blitz's DOM walk into the renderer's `PaintScene`.
- **raster** is the rest of `ImageRenderer::render`.
- tiny-skia draws immediately, so nearly all of its time shows as paint. Compare totals.

### Forcing scalar code paths

On x86 neither rasterizer can be made scalar from outside:

- **vello_cpu:** `fearless_simd` 1.3 detects SSE4.2/AVX2 at runtime (`fearless_simd-1.3.0/src/lib.rs:361`), and `vello_cpu`'s default settings use it (`vello_cpu-0.4.0/src/render.rs:162`). `anyrender_vello_cpu` creates the context itself (`src/scene.rs:151`), so there is no way to pass `Level::fallback()`.
- **tiny-skia:** `anyrender_tiny_skia` enables `tiny-skia/simd`, which uses SSE2 on any x86 target.

The scalar numbers therefore come from **`wasm32-wasip1` without `simd128`**, where both libraries compile only their scalar paths, run in Node 25 (V8 TurboFan).

- **wasm cost:** that adds JIT/wasm overhead, typically 1.2–2x native. The scalar figures are therefore pessimistic for this host.
- **Lower bound:** the native x86_64 run (SSE4.2, no AVX2) gives a SIMD-assisted lower bound.
- **Profiles:** `opt-level=3` (the release default) and `opt-level=s` (size build) via `CARGO_PROFILE_RELEASE_OPT_LEVEL`. The repo had no size profile yet.

### Full frame 1024x600, warm median, ms

| page | rasterizer | native x86_64 SIMD, O3 | wasm32 scalar, O3 | wasm32 scalar, Os |
|---|---|---|---|---|
| dashboard | vello_cpu | 29.4 | 70.8 | 104.1 |
| dashboard | tiny-skia | 258.4 | 496.0 | 693.0 |
| article | vello_cpu | 15.8 | 42.1 | 65.8 |
| article | tiny-skia | 31.1 | 68.8 | 112.7 |
| links | vello_cpu | 24.0 | 46.9 | 75.8 |
| links | tiny-skia | 34.0 | 59.4 | 89.4 |
| wikipedia-home-automation | vello_cpu | 9.2 | 27.7 | 41.1 |
| wikipedia-home-automation | tiny-skia | 27.1 | 129.8 | 193.7 |
| hn-front | vello_cpu | 15.0 | 50.0 | 76.1 |
| hn-front | tiny-skia | 40.3 | 114.0 | 175.7 |

### 19 bands of 1024x32 covering the frame, warm median, ms

| page | rasterizer | native O3 | wasm32 O3 | wasm32 Os |
|---|---|---|---|---|
| dashboard | vello_cpu | 46.6 | 92.3 | 154.1 |
| dashboard | tiny-skia | 359.3 | 632.2 | 910.1 |
| article | vello_cpu | 46.0 | 90.7 | 142.3 |
| article | tiny-skia | 56.4 | 118.1 | 185.1 |
| links | vello_cpu | 44.1 | 91.2 | 132.5 |
| links | tiny-skia | 41.1 | 78.4 | 117.7 |
| wikipedia-home-automation | vello_cpu | 25.7 | 56.2 | 74.9 |
| wikipedia-home-automation | tiny-skia | 35.0 | 153.0 | 196.4 |
| hn-front | vello_cpu | 44.3 | 99.0 | 145.6 |
| hn-front | tiny-skia | 57.4 | 138.0 | 208.2 |

**Variance.** The host is a shared VM and other workers were building; an earlier run of the same commands gave 128 ms instead of 71 ms for the dashboard with vello_cpu wasm O3. Treat single numbers as ±50 %.

**Band overhead.** Banding costs 1.1–3x a full frame. Each band starts its paint walk at the root and builds a fresh renderer; culling to the band clip exists (`render.rs:253`), yet each band still costs a fixed 2–5 ms in wasm. The split between walk and renderer setup was not profiled.

For time-sliced strips this is acceptable, but per-band fixed cost is the thing to optimise in M1, for example by reusing one renderer.

The cold-frame column is in the `raster` output; the commands are below.

### Provisional P4 estimate (CoreMark derate)

**Provisional, not a measurement.** The assumptions:

- **P4:** the datasheet (chip rev. v1.3, v1.2 header) gives **2489.62 CoreMark for the dual-core HP CPU at 360 MHz**, which is 6.92 CoreMark/MHz.
  - 360 MHz is the default clock; 400 MHz is available on request. The plan assumed 400.
  - Our engine is single-threaded. *Assumption:* single-core ≈ half the dual-core figure, so **1245 CoreMark** (3.46/MHz).
- **Host:** EEMBC CoreMark (commit `1f483d5`, `simple` port) built with MSVC `/O2` as 32-bit x86, single thread, three idle runs: 8015, 8625, 8796; median **8625**.
  - The x64 build of the `simple` port is invalid (its `ee_ptr_int` is 32-bit, so it truncates pointers and crashes intermittently); its earlier 6698 is discarded.
  - The host figure is MSVC; the datasheet figure is presumably a tuned GCC build, which flatters the P4 a little.
  - Derate factor: 8625 / 1245 = **6.9x** at 360 MHz, or 6.2x at 400 MHz.
- **Not modelled:**
  - CoreMark fits in cache; raster and the DOM walk on the P4 touch PSRAM. Expect another 1.5–3x on top.
  - Instruction-mix differences: x86 FP/branching vs RV32IMAFC. vello_cpu uses f32 throughout, and the P4 has a single-precision FPU.

| page | vello_cpu: native ×6.9 (lower bound) | vello_cpu: wasm scalar O3 / Os ×6.9 | tiny-skia: wasm scalar O3 / Os ×6.9 |
|---|---|---|---|
| dashboard | 0.20 s | 0.49 s / 0.72 s | 3.4 s / 4.8 s |
| article | 0.11 s | 0.29 s / 0.45 s | 0.47 s / 0.78 s |
| links | 0.17 s | 0.32 s / 0.52 s | 0.41 s / 0.62 s |
| wikipedia-home-automation | 0.06 s | 0.19 s / 0.28 s | 0.90 s / 1.3 s |
| hn-front | 0.10 s | 0.34 s / 0.53 s | 0.79 s / 1.2 s |

- **vello_cpu:** a full frame lands at 0.2–0.7 s (scalar, O3 to Os) before the PSRAM penalty, so it is likely inside the H1 "go ≤1 s" on text pages. The dashboard (box shadows, gradients) could reach 1–2 s with PSRAM.
- **tiny-skia:** about 7x slower than vello_cpu on the dashboard, already over 3 s before PSRAM.
- **Recommendation:** vello_cpu as the default rasterizer. The H1 measurement on hardware decides.
- **Highlight re-raster (H1 ≤100 ms):** the damage test below re-rasters 8 full-width bands. In wasm that is 8 × 4.9 ms (O3) to 8 × 8.1 ms (Os) = 39–65 ms, ×6.9 = **~0.27–0.45 s**.
  - That is past the H1 fallback limit (≤300 ms) unless bands are also clipped in x to the damaged rect and the per-band walk gets cheaper (M1 work). It contradicts the plan's assumption that band-limited re-raster is enough.

## Banding

**Routes.** `paint_scene`'s `x_offset`/`y_offset` are `u32` and *added* to the transform (`blitz-paint-0.3.0-beta.6/src/render.rs:260`). They place content further down a larger target, so they cannot select a band lower in the page. Two other routes were tested:

1. **Viewport scroll + 32px viewport:** `doc.set_viewport_scroll(y)`, then `paint_scene(.., width 1024, height 32, 0, 0)`. Blitz culls to the 32px clip.
2. **Recorded scene replayed:** paint once into an `anyrender::Scene`, then replay it with `append_scene(scene, translate(0, -y))` into a 1024x32 renderer. There is no culling.

**Comparison.** Each band was diffed against rows `y..y+32` of the full frame, over 18 bands, using `measure band`.

| page | rasterizer | route | bands with differences | differing pixels | in band edge rows | max channel delta |
|---|---|---|---|---|---|---|
| dashboard | vello_cpu | scroll | 1 | 1 | 0 | 1 |
| dashboard | vello_cpu | replay | 1 | 1 | 0 | 1 |
| dashboard | tiny-skia | scroll | 15 | 980 | 723 | 47 |
| dashboard | tiny-skia | replay | 15 | 673 | 416 | 47 |
| article | vello_cpu | scroll | 1 | 1 | 0 | 1 |
| article | vello_cpu | replay | 1 | 1 | 0 | 1 |
| article | tiny-skia | scroll | 18 | 989 | 427 | 42 |
| article | tiny-skia | replay | 18 | 989 | 427 | 42 |
| links | vello_cpu | scroll | 0 | 0 | 0 | 0 |
| links | vello_cpu | replay | 0 | 0 | 0 | 0 |
| links | tiny-skia | scroll | 16 | 321 | 318 | 46 |
| links | tiny-skia | replay | 16 | 321 | 318 | 46 |
| hn-front | vello_cpu | scroll | 1 | 6 | 6 | 215 |
| hn-front | vello_cpu | replay | 17 | 4071 | 540 | 215 |
| hn-front | tiny-skia | scroll | 13 | 261 | 255 | 47 |
| hn-front | tiny-skia | replay | 14 | 263 | 255 | 47 |
| wikipedia-home-automation | vello_cpu | scroll | 2 | 3 | 0 | 1 |
| wikipedia-home-automation | vello_cpu | replay | 6 | 1319 | 390 | 2 |
| wikipedia-home-automation | tiny-skia | scroll | 6 | 151 | 128 | 28 |
| wikipedia-home-automation | tiny-skia | replay | 6 | 151 | 128 | 28 |

**Result:** banding via viewport scroll works.

- **vello_cpu** matches the full frame to within 1/255 except for 6 pixels on HN, all in band edge rows (delta 215; cause not investigated).
- **tiny-skia** differs by up to 47/255 in a few hundred pixels per page, mostly in the first and last two rows of a band, so seams are possible.
- **Replay route:** no better, and it loses culling.

## Damage source

**Code.** Blitz does track damage; it just doesn't hand it out after `resolve()`:

- Stylo writes `RestyleDamage` into each element's data during the style traversal.
- `Node::damage()` and `Node::has_damaged_descendants()` are public (`blitz-dom-0.3.0-beta.6/src/node/node.rs:692`, `:648`).
- `BaseDocument::resolve_stylist()` is public too (`src/stylo/traversal.rs:40`).
- `resolve()` runs the style traversal (`src/resolve.rs:79`) and uses the damage for incremental layout (`:91`). It then clears all of it (`:130`, `src/layout/damage.rs:297`).

**Workaround.** Call `resolve_stylist()`, read `damage()` on the nodes, then call `resolve()`.

**Experiment** (`measure damage`): set `class="card on"` on the second dashboard card.

| node | damage | y range |
|---|---|---|
| `<section>` (card) | REPAINT | 110..345 |
| `<span>` (bulb icon) | REPAINT | 122..144 |
| `<span>` (toggle) | REPAINT, REBUILD_STACKING_CONTEXT, RECALCULATE_OVERFLOW, RELAYOUT | 123..143 |
| `::after` (toggle knob) | REPAINT, REBUILD_STACKING_CONTEXT, RECALCULATE_OVERFLOW, RELAYOUT | 125..141 |

- **Coverage:** bands from the damaged nodes' old and new bounds were 3–10. Bands with changed pixels in a before/after diff were exactly 3–10, and no changed band was missed.
- **After `resolve()`:** 0 nodes still carry damage, which confirms the clearing.

**DOM mutations.** The same experiment with a text change and an insertion:

| change | damaged nodes | bands from their bounds | bands with changed pixels | missed |
|---|---|---|---|---|
| toggle a card's class | 4 (above) | 3–10 | 3–10 | none |
| `set_node_text` on a temperature | the parent `<span>` (RELAYOUT, REPAINT, construct bits) | 12–13 | 12–13 | none |
| append an `<li>` to the status list | the `<ul>` and the new `<li>` | 0, 7–14 | 3–18 | 3–6, 15–18 |

- Text edits and insertions do mark damage on the parent element, so the source is not restyle-only.
- **Gap:** the insertion grew the sidebar, which made the grid row taller and resized the cards and the chart. Those nodes moved or resized without carrying any damage.
  - Relayout effects therefore need a bounds diff: absolute bounds of laid-out nodes before and after `resolve()`.
- Image loads were not tested.

**M1 recommendation:** take damage from Stylo per element, plus old/new absolute bounds of nodes whose layout changed. Or upstream a small Blitz hook that returns the damaged rects from `resolve()`.

**Contradiction:** ADR 0002 and 0003 say Blitz has no damage tracking. That should be corrected when ADR 0002 is accepted or superseded.

## Reproduce

From the repo root, on Windows (Git Bash) or Linux:

```sh
# Corpus
python testpages/generate.py testpages     # synthetic pages (already committed)
bash testpages/fetch.sh                    # real pages into testpages/fetched/ (not committed)

# Builds
cargo build -p measure --release                                  # native 64-bit
rustup target add wasm32-wasip1
cargo build -p measure --release --target wasm32-wasip1           # 32-bit, scalar
CARGO_TARGET_DIR=target/opt-s CARGO_PROFILE_RELEASE_OPT_LEVEL=s \
  cargo build -p measure --release --target wasm32-wasip1         # size profile

# Heap: one process per page and config (corpus + fetched pages)
./target/release/measure heap-all
MSYS_NO_PATHCONV=1 ./target/release/measure heap-all --wasm target/wasm32-wasip1/release/measure.wasm

# Raster timing (Git Bash needs MSYS_NO_PATHCONV=1 so /repo paths reach Node unchanged)
P="testpages/dashboard.html testpages/article.html testpages/links.html testpages/fetched/wikipedia-home-automation.html testpages/fetched/hn-front.html"
./target/release/measure raster --iters 7 $P
MSYS_NO_PATHCONV=1 node --no-warnings tools/measure/wasi-run.mjs target/wasm32-wasip1/release/measure.wasm raster --iters 7 $(printf '/repo/%s ' $P)
MSYS_NO_PATHCONV=1 node --no-warnings tools/measure/wasi-run.mjs target/opt-s/wasm32-wasip1/release/measure.wasm raster --iters 7 $(printf '/repo/%s ' $P)

# Banding, damage, PNGs
./target/release/measure band
./target/release/measure damage
./target/release/measure png target/measure-png
```

CoreMark on the host: clone `https://github.com/eembc/coremark` (commit `1f483d5`), then in a VS 2022 x64 (or x86) developer prompt run:

```bat
cl /O2 /I. /Isimple /DFLAGS_STR=\"/O2\" /DITERATIONS=0 /DPERFORMANCE_RUN=1 core_list_join.c core_main.c core_matrix.c core_state.c core_util.c simple\core_portme.c /Fe:coremark.exe
coremark.exe
```

Host numbers vary by a few percent between runs, because the host is a shared VM and other builds ran at the same time.
