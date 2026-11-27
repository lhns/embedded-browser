# M0 host results: peak heap and raster estimate

Date: 2026-11-27. Roadmap nodes `eng-peak-heap` and `eng-raster-estimate`. Tool: `tools/measure`.

## Verdict

| Metric | Result | M0 table | Verdict |
|---|---|---|---|
| Peak heap, P4 (32-bit, engine + 32px bands) | 1.1–3.6 MB on the synthetic pages, 8.7 MB on a real Wikipedia article | go ≤12 MB | **go** on every page |
| Peak heap, P4, with a full 1024x600 RGBA frame (vello_cpu) | 4.0–6.5 MB synthetic, 11.5 MB Wikipedia | go ≤12 MB | **go**, with only 0.5 MB margin on Wikipedia |
| Peak heap, S3 tier (≤4 MB) | dashboard 1.1, links 1.7, HN 2.1, article 3.6 MB; Wikipedia 8.7 MB | ≤4 MB | **go** for the dashboard and small pages; **no** for long real articles |
| Sequential styling spawns threads | none | — | confirmed |
| Full frame on P4, vello_cpu (provisional) | ~0.1–0.7 s before any PSRAM penalty | H1: go ≤1 s | probably **go**; the dashboard is the risk |
| Full frame on P4, tiny-skia (provisional) | ~0.3–1.0 s on text pages, ~3.3 s on the dashboard | H1: stop >3 s | **stop** for the dashboard, fallback on the rest |
| Banded `paint_scene` | correct via viewport scroll: ≤3 differing pixels per page with vello_cpu | — | works |
| Damage source | Stylo `RestyleDamage` per element, readable between `resolve_stylist` and `resolve` | — | exists; ADR 0002/0003 need correcting |

Per-page heap verdicts, using the 32-bit engine plus 32px vello_cpu bands, which is the planned device setup:

| Page | Peak MB | P4 (≤12 go, 12–20 fallback, >20 stop) | S3 tier (≤4 MB) |
|---|---|---|---|
| dashboard (HA-like, synthetic) | 1.10 | go | go |
| article (synthetic) | 3.58 | go | go, tight |
| links (synthetic) | 1.72 | go | go |
| hn-front (fetched) | 2.13 | go | go |
| wikipedia-home-automation (fetched, no images) | 8.65 | go | no |

## Setup

- **Blitz:** `blitz-dom`, `blitz-html`, `blitz-paint` and `blitz-traits` `=0.3.0-beta.6`, all with `default-features = false`.
  - `floats` is enabled; it is not in Blitz's defaults, and articles need it.
  - The rasterizers are `anyrender_vello_cpu` 0.19.0 and `anyrender_tiny_skia` 0.1.1. Both use `anyrender` 0.15.1, the version `blitz-paint` requires.
- **Fonts:** Noto Sans Regular and Bold from the `ttf-noto-sans` crate (OFL), registered as `&'static` blobs. No system fonts.
  - The 400 KB of font data stays out of the heap, as it would when mapped from a flash partition.
- **Net:** a local `NetProvider` serves only `data:` and `file:` URLs, synchronously.
  - `image` is added with `png`, because Blitz pulls it in with no codecs.
- **Styling:** `StyleThreading::Sequential` (the 0.3.0-beta.6 default). Viewport 1024x600 at scale 1.
- **Corpus:**
  - Synthetic: `testpages/dashboard.html`, `article.html` (~32 KB of text, floats, tables, three PNG data URIs) and `links.html` (forms).
  - Fetched, not committed: `testpages/fetch.sh` gets a Wikipedia article and the HN front page with their stylesheets. Remote images are not fetched, so the Wikipedia image heap is missing.
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

Features that were turned off: `system-fonts`, `text-transform-icu`, `svg` (inline SVG does not paint, so the dashboard uses CSS icons), `woff`, `accessibility`, `custom-widget`, `file-input`, `complex-scripts`, `scrollbars` and vello_cpu `multithreading`. `tools/measure` has an `svg` feature for later delta measurements; it is not measured here.

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
  - Stacks, TLS and network buffers.

**32-bit target.** `i686-pc-windows-msvc` does not build. `anyrender` 0.15.1 asserts `size_of::<FilterEffect>() == 84` on 32-bit x86 (`src/filters.rs:25`), but MSVC aligns `u64` to 8, which gives 88. The gnu variant has the same Windows alignment.

So the 32-bit numbers come from **`wasm32-wasip1`** run under Node's WASI. Its data layout (4-byte pointers, 8-byte-aligned `u64`) matches `riscv32imafc` better than i686 does anyway.

### 32-bit (wasm32-wasip1), MB

| page | config | DOM nodes | engine peak | total peak | at peak: dom+style+layout / text / images / raster / other | blocks at peak |
|---|---|---|---|---|---|---|
| dashboard | engine only | 299 | 0.78 | 0.78 | 0.78 / 0.00 / 0.00 / 0.00 / 0.01 | 4396 |
| dashboard | vello_cpu full frame | 299 | 0.78 | 4.01 | 0.78 / 0.00 / 0.00 / 3.23 / 0.00 | 5162 |
| dashboard | vello_cpu 32px bands | 299 | 0.78 | 1.10 | 0.78 / 0.00 / 0.00 / 0.33 / 0.00 | 4585 |
| dashboard | tiny-skia full frame | 299 | 0.78 | 8.45 | 0.78 / 0.00 / 0.00 / 7.67 / 0.00 | 4655 |
| dashboard | tiny-skia 32px bands | 299 | 0.78 | 1.38 | 0.78 / 0.00 / 0.00 / 0.60 / 0.00 | 4493 |
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
| hn-front | engine only | 1216 | 1.86 | 1.86 | 1.83 / 0.00 / 0.00 / 0.00 / 0.03 | 9599 |
| hn-front | vello_cpu full frame | 1216 | 1.86 | 5.03 | 1.77 / 0.00 / 0.00 / 3.26 / 0.00 | 10384 |
| hn-front | vello_cpu 32px bands | 1216 | 1.86 | 2.13 | 1.77 / 0.00 / 0.00 / 0.36 / 0.00 | 9950 |
| hn-front | tiny-skia full frame | 1216 | 1.86 | 6.86 | 1.77 / 0.00 / 0.00 / 5.09 / 0.00 | 9969 |
| hn-front | tiny-skia 32px bands | 1216 | 1.86 | 2.38 | 1.77 / 0.00 / 0.00 / 0.61 / 0.00 | 9823 |
| wikipedia-home-automation | engine only | 6109 | 8.61 | 8.61 | 8.30 / 0.00 / 0.00 / 0.00 / 0.31 | 50947 |
| wikipedia-home-automation | vello_cpu full frame | 6109 | 8.61 | 11.54 | 8.30 / 0.00 / 0.00 / 3.24 / 0.00 | 51609 |
| wikipedia-home-automation | vello_cpu 32px bands | 6109 | 8.61 | 8.65 | 8.30 / 0.00 / 0.00 / 0.35 / 0.00 | 51136 |
| wikipedia-home-automation | tiny-skia full frame | 6109 | 8.61 | 15.62 | 8.30 / 0.00 / 0.00 / 7.32 / 0.00 | 51184 |
| wikipedia-home-automation | tiny-skia 32px bands | 6109 | 8.61 | 8.89 | 8.30 / 0.00 / 0.00 / 0.59 / 0.00 | 51002 |

### 64-bit (x86_64-pc-windows-msvc), MB, for comparison

| page | engine peak | vello full | vello bands | tiny full | tiny bands |
|---|---|---|---|---|---|
| dashboard | 1.01 | 4.27 | 1.34 | 8.68 | 1.60 |
| article | 3.55 | 6.93 | 3.95 | 8.56 | 4.06 |
| links | 1.81 | 4.99 | 2.07 | 6.68 | 2.22 |
| hn-front | 2.35 | 5.58 | 2.63 | 7.35 | 2.87 |
| wikipedia-home-automation | 10.74 | 13.71 | 10.80 | 17.76 | 11.02 |

### Observations

- **DOM, style and layout dominate.** Engine heap tracks the stylesheet more than the node count. Wikipedia's 208 KB stylesheet plus 6 k nodes reach 8.3 MB.
  - 64-bit adds about 20–30 %.
- **vello_cpu raster** in 32px bands costs 0.3–0.4 MB. A full frame costs about 3.2 MB, of which 2.4 MB is the RGBA buffer.
- **tiny-skia** needs 5–7.7 MB for a full frame, because of its layer pixmaps for shadows, gradients and clips. Even in bands it uses about twice vello's heap.
- **Synthetic images:** three 160x110 PNGs decode to 0.2 MB of RGBA. Decoded images will matter on real pages, which supports the plan's "decode at display size into a bounded cache".

## Threads

- **wasm32-wasip1:** every `heap` and `raster` run finished there, and `wasm32-wasip1` cannot start threads. Any rayon or Stylo pool use would have panicked, so sequential styling, layout and both rasterizers spawn no threads.
- **Native, rayon's global pool:** after each native run, `rayon::ThreadPoolBuilder::build_global()` returned `Ok` in all 25 runs, so the global pool was never initialized.
- **Native, OS thread count:** measured with Toolhelp32 before and after each run, it moved by ±1 in both directions. These are Windows loader and thread-pool workers that exist before Blitz runs, not engine threads.
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
| dashboard | vello_cpu | 33.2 | 128.4 | 110.2 |
| dashboard | tiny-skia | 279.1 | 620.7 | 625.4 |
| article | vello_cpu | 15.9 | 43.3 | 64.1 |
| article | tiny-skia | 32.7 | 61.8 | 105.1 |
| links | vello_cpu | 12.4 | 43.8 | 64.4 |
| links | tiny-skia | 34.1 | 54.5 | 90.1 |
| wikipedia-home-automation | vello_cpu | 10.2 | 21.5 | 37.1 |
| wikipedia-home-automation | tiny-skia | 25.7 | 118.7 | 191.8 |
| hn-front | vello_cpu | 15.9 | 48.8 | 79.7 |
| hn-front | tiny-skia | 42.4 | 83.9 | 166.0 |

### 19 bands of 1024x32 covering the frame, warm median, ms

| page | rasterizer | native O3 | wasm32 O3 | wasm32 Os |
|---|---|---|---|---|
| dashboard | vello_cpu | 52.7 | 146.0 | 148.3 |
| dashboard | tiny-skia | 394.4 | 846.2 | 777.5 |
| article | vello_cpu | 46.9 | 94.9 | 121.3 |
| article | tiny-skia | 56.2 | 100.6 | 146.4 |
| links | vello_cpu | 36.8 | 81.7 | 113.6 |
| links | tiny-skia | 47.6 | 67.5 | 114.9 |
| wikipedia-home-automation | vello_cpu | 26.6 | 44.2 | 68.3 |
| wikipedia-home-automation | tiny-skia | 37.8 | 139.5 | 211.2 |
| hn-front | vello_cpu | 46.0 | 98.3 | 180.0 |
| hn-front | tiny-skia | 50.4 | 109.9 | 196.9 |

**Band overhead.** Banding costs 1.1–3x a full frame. Each band walks the paint tree from the root, which costs about 2–5 ms per band in wasm, and builds a fresh renderer. Culling to the band works (`render.rs:253`), but the walk does not skip whole subtrees above the band.

For time-sliced strips this is acceptable, but per-band fixed cost is the thing to optimise in M1: reuse one renderer, and start the walk at stacking contexts that intersect the band.

The cold-frame column is in the `raster` output; the commands are below.

### Provisional P4 estimate (CoreMark derate)

**Provisional, not a measurement.** The assumptions:

- **P4:** the datasheet (chip rev. v1.3, v1.2 header) gives **2489.62 CoreMark for the dual-core HP CPU at 360 MHz**, which is 6.92 CoreMark/MHz.
  - 360 MHz is the default clock; 400 MHz is available on request. The plan assumed 400.
  - Our engine is single-threaded. *Assumption:* single-core ≈ half the dual-core figure, so **1245 CoreMark** (3.46/MHz).
- **Host:** EEMBC CoreMark (commit `1f483d5`) built with MSVC `/O2` and run on this host, single thread: **6698** (x64) and **7272** (x86).
  - Derate factor: 6698 / 1245 = **5.4x** at 360 MHz, or 4.8x at 400 MHz.
- **Not modelled:**
  - CoreMark fits in cache; raster and the DOM walk on the P4 touch PSRAM. Expect another 1.5–3x on top.
  - Instruction-mix differences: x86 FP/branching vs RV32IMAFC. vello_cpu uses f32 throughout, and the P4 has a single-precision FPU.

| page | vello_cpu: native ×5.4 (lower bound) | vello_cpu: wasm scalar O3 ×5.4 | tiny-skia: wasm scalar O3 ×5.4 |
|---|---|---|---|
| dashboard | 0.18 s | 0.69 s | 3.3 s |
| article | 0.09 s | 0.23 s | 0.33 s |
| links | 0.07 s | 0.24 s | 0.29 s |
| wikipedia-home-automation | 0.06 s | 0.12 s | 0.64 s |
| hn-front | 0.09 s | 0.26 s | 0.45 s |

- **vello_cpu:** a full frame lands at 0.1–0.7 s before the PSRAM penalty, so it is likely inside the H1 "go ≤1 s" on text pages. The dashboard (box shadows, gradients) could reach 1–2 s with PSRAM.
- **tiny-skia:** about 5x slower on the dashboard, already over 3 s before PSRAM.
- **Recommendation:** vello_cpu as the default rasterizer. The H1 measurement on hardware decides.
- **Highlight re-raster (H1 ≤100 ms):** the damage test below re-rasters 8 full-width bands, about 8 × 7.7 ms in wasm = 62 ms, ×5.4 = **~0.33 s**. That is over the H1 fallback limit unless bands are also clipped in x and the per-band walk gets cheaper (M1 work).

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
| hn-front | vello_cpu | scroll | 2 | 3 | 3 | 196 |
| hn-front | vello_cpu | replay | 17 | 4015 | 455 | 196 |
| hn-front | tiny-skia | scroll | 11 | 171 | 168 | 31 |
| hn-front | tiny-skia | replay | 12 | 173 | 168 | 31 |
| wikipedia-home-automation | vello_cpu | scroll | 2 | 3 | 0 | 1 |
| wikipedia-home-automation | vello_cpu | replay | 6 | 1319 | 390 | 2 |
| wikipedia-home-automation | tiny-skia | scroll | 6 | 151 | 128 | 28 |
| wikipedia-home-automation | tiny-skia | replay | 6 | 151 | 128 | 28 |

**Result:** banding via viewport scroll works.

- **vello_cpu** matches the full frame to within 1/255 except for 3 pixels on HN, all in band edge rows (delta 196; cause not investigated).
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

**Gaps.** This is restyle damage only.

- DOM insertions and removals, image loads and text edits go through Blitz's own construct/relayout flags, not Stylo damage.
- Changes in layout position, such as siblings moving after a relayout, need a bounds diff before and after `resolve()`.

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
