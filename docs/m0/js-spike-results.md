# M0 JS host spike: results

Roadmap node `js-host-spike`. Question: how far can on-device JS go, measured on the host
with the real corpus scripts? Date: 2026-11-27. Tool: `tools/js-spike`.

## Short answer

- **The J2 corpus fits, htmx close to the line.** The custom dashboard, Tasmota, ESPHome v1 and htmx (plus
  petite-vue) all run to completion in QuickJS-ng under a 1 MB cap on a 32-bit host.
  - Peak JS heap is 0.3–0.94 MB, including the runtime and context (152 KB).
  - The script raises no exceptions.
- **Parse speed is the risk, not memory.** Cold compile runs at about 3–6 KB/ms on the host.
  - With a provisional P4 derate of 20–75x, htmx (52 KB) needs about 0.4–1.9 s.
  - A 100 KB script needs about 0.7–2.5 s, against J1's 300 ms target.
  - Loading cached, stripped bytecode is 10–30x faster than compiling: htmx loads in
    0.9 ms on the host, so about 20–70 ms on the P4.
- **The J2 API set is about 140 logged paths on 35 interfaces.**
  - It needs no shadow DOM, custom elements or canvas.
  - The bulk is DOM core (78), XHR/fetch/WebSocket/EventSource (36) and events (15).
- **Home Assistant confirms remote mode.**
  - At start-up it imports 4.6 MB of modules (97 files), which take 1.5–2.2 s to compile
    on the host.
  - Heap reaches 34 MB (32-bit) before anything renders.
  - It defines custom elements, calls `attachShadow` and `CSSStyleSheet.replaceSync`, and
    fetches Intl polyfill data because QuickJS-ng has no `Intl`.
- **QuickJS-ng adds about 0.7 MB of code at `-Os`** (1.2 MB at `-O3`), measured as stripped
  i686 binaries with and without it. Its C objects compiled for rv32imafc are 528 KB, so I
  estimate 0.55–0.75 MB on the P4. That is within J1's 1.5 MB flash budget.
- **Modern syntax parses everywhere.** Probes for ES2020–ES2025 all pass, and no corpus
  script had a syntax error. The only gap is ECMA-402 `Intl`, which is missing.

## Method

- **Engine:** `rquickjs` 0.14.0 bundles QuickJS-ng 0.16.2 (`rquickjs-sys` vendors the
  quickjs-ng sources). It also ships pre-generated bindings for x86_64 Windows/Linux and i686
  Linux, so host builds and CI need no clang or bindgen.
- **Memory caps and peak heap:**
  - Each run gets a fresh `Runtime` with `set_memory_limit` and a counting allocator
    (`Runtime::new_with_alloc`) that records the peak.
  - The allocator sees QuickJS-ng's 4 KB arena pages, so its peak is the real footprint;
    `memory_usage()` is only a snapshot.
  - A capped run **passes** when it completes every phase with the same exceptions and the
    same API log as the uncapped run. An out-of-memory error swallowed by a page shows up as a
    different log.
- **Stub DOM:** `tools/js-spike/src/stub_dom.js` is a logging stub DOM in JS.
  - It builds real (small) node trees from the page HTML, so `querySelector`, `innerHTML`,
    `DOMParser` and `<template>` behave.
  - Every member read through Proxies is logged as `<interface>.<member>`.
  - Unknown members of DOM objects are logged as `(missing)` and return `undefined`, because
    pages probe expandos and feature-detect.
  - Known-but-unmodelled APIs (`animate`, `getContext`, observers, ...) return
    callable/constructible stubs whose every member is a further stub.
  - Unknown globals stay `undefined`, so UMD and `typeof` checks take the right branch.
- **Stub heap is excluded:**
  - The stub's own heap (392 KB at 32-bit) is a measurement artifact, since J2 would bind
    natively.
  - Heap figures subtract the stub's allocator share, and caps add the stub's
    QuickJS-accounted share on top of `set_memory_limit`.
  - The stub is loaded as stripped bytecode, and peaks are reset after it loads.
- **Phases per page:**
  1. Scripts run in document order, inline and external, as deferred. Each is compiled first
     (`JS_EVAL_FLAG_COMPILE_ONLY`) and then run, so compile and run times separate.
  2. `DOMContentLoaded` and `load` fire, followed by microtasks.
  3. An event loop runs on a virtual clock for 10 s. Timers fire, and
     fetch/XHR/WebSocket/EventSource get canned replies from the item's `fixtures.json`.
  4. A synthetic click fires on every element with a click handler (listener, property or
     inline `on*` attribute). Selects and inputs with change handlers get `input` and
     `change` instead. Each interaction is followed by 3 s of event loop.
- **Stripped bytecode:** a second uncapped run round-trips each classic script through
  `JS_WriteObject(STRIP_SOURCE)` and `JS_ReadObject`.
  - QuickJS-ng otherwise keeps a copy of every function's source text for
    `Function.prototype.toString`.
  - This run reports bytecode size and load time.
- **Timings:** release build, minimum of 5 runs on x86_64 Windows. The tables show the i686
  run in Docker, and both full reports are linked below.
  - QuickJS-ng is plain C with no explicit SIMD. The compiler may still auto-vectorize on
    x86, so these are "scalar code" timings only in that sense.

## Results (i686, 32-bit; full report: [js-spike/report-i686.md](js-spike/report-i686.md))

| item | script bytes | compile ms | run ms | peak heap | heap at end | peak, stripped bytecode | 1 MB | 4 MB | 16 MB | top-level exceptions |
|---|---|---|---|---|---|---|---|---|---|---|
| custom dashboard | 3 718 | 0.72 | 0.41 | 309 KB | 299 KB | 299 KB | pass | pass | pass | 0 |
| htmx 2.0.11 | 52 182 | 25.8 | 1.3 | 936 KB | 792 KB | 798 KB | pass | pass | pass | 0 |
| petite-vue 0.4.1 | 16 901 | 8.8 | 22.0 | 414 KB | 382 KB | 414 KB | pass | pass | pass | 0 |
| Tasmota v16.0.0 root page | 1 997 | 0.59 | 0.09 | 368 KB | 283 KB | 362 KB | pass | pass | pass | 0 |
| ESPHome web_server v1 | 5 213 | 0.90 | 7.3 | 319 KB | 296 KB | 319 KB | pass | pass | pass | 0 |
| Home Assistant 20261125.1 (info) | 4 561 636 | 2 240 | 565 | 33.9 MB | 33.9 MB | n/a (modules) | FAIL | FAIL | FAIL | 4 |

- **What the heap includes:** heaps are allocator peaks over the whole run, including the
  runtime and a full context (152 KB) but not the stub DOM.
- **x86_64 comparison** ([js-spike/report-x86_64-windows.md](js-spike/report-x86_64-windows.md)):
  heaps are 1.15–1.3x larger, so htmx peaks at 1.21 MB and **fails the 1 MB cap at 64-bit**.
  Compile times there are about 1.6–2.5x faster (htmx 9.6–16 ms).
- **Timing noise:** the host is a shared VM, and other workers' Docker builds ran
  concurrently. The same i686 binary compiled htmx in 18.5 ms in one run and 25.8 ms in
  another (both the minimum of 5), so read timings as ±40%.
- **Bytecode:** stripped bytecode is 0.65–1.85x the source size. It loads in 0.04 ms
  (dashboard) to 0.88 ms (htmx) at 32-bit, against 0.6–26 ms to compile.
- **Exceptions:** none of the J2 items raised an exception, an unhandled rejection or a
  `console.error`.
  - While testing, an htmx `console.error` (`htmx:badResponseUrl`) exposed a stub bug: a
    relative `responseURL`. That bug is fixed.
  - The four HA exceptions are translation fetches that 404 against the fixtures.

What each item exercised:

- **Custom dashboard** (synthetic, `testpages/js/dashboard/`):
  - fetches `/api/states`;
  - runs the HA-style WebSocket auth and subscribe, and applies a `state_changed` event;
  - uses `<template>` cloning, `classList`, `dataset`, `style` and `localStorage`;
  - has click handlers that POST services, plus an inline `onclick`;
  - runs `setInterval`.
- **htmx:**
  - `hx-get` polling (`load, every 2s`), click-to-load with `outerHTML` swap, a form
    `hx-post` and an out-of-band swap;
  - XHR plus `DOMParser` and `<template>` fragment parsing.
- **Tasmota:** the real root-page script (`la()` polling over XHR with `setTimeout`, and
  `innerHTML` swaps); buttons with inline `onclick='la("&o=1")'`; a slider whose inline
  `onchange` uses element-scoped names (`value`).
- **ESPHome v1:** `EventSource('/events')` with `log` (ANSI-coloured), `state` (both id and
  `name_id` forms) and `ping` events; button and select handlers that POST over XHR.
  - It walks the table with `table.rows` and `row.children`.
  - It also uses `CSS.escape` and attribute selectors with `:not()`.
- **petite-vue** (extra, not counted for J2): `v-scope`, `v-for`, `v-if`, `v-model`,
  `v-show`, `@click`; it compiles expressions with `new Function`.

## Web APIs touched

Full per-item lists, with `(missing)` probes and selector strings, are in the reports.

J2 union (dashboard, Tasmota, ESPHome v1, htmx) at 32-bit is **143 API paths on 35
interfaces and globals**, by group:

| group | count | members |
|---|---|---|
| DOM core | 78 | `Document`: `body`, `head`, `title`, `readyState`, `activeElement`, `getElementById`, `querySelector(All)`, `createElement`, `createDocumentFragment`. `Element`: `id`, `className`, `tagName`, `classList` (`add`/`remove`/`toggle`/`contains`/`length`), `attributes`, `get/has/removeAttribute`, `children`, `matches`, `closest`, `querySelector(All)`, `innerHTML` get/set, `insertAdjacentHTML`, `remove`. `Node`: `appendChild`, `insertBefore`, `removeChild`, `cloneNode`, `childNodes`, `first/next/previousSibling`, `parentNode`/`parentElement`, `nodeType`, `isConnected`, `getRootNode`, `textContent` set. `HTMLElement`: `style`, `dataset`, `hidden`, `innerText` set. Form controls: `name`, `value`, `form`; `HTMLFormElement.elements`/`name`; htmx also probes the validation API (`willValidate`, `noValidate`, `formNoValidate`, logged as missing). `Node.isSameNode`. `HTMLTableElement.rows`, `HTMLTemplateElement.content`, `DOMParser.parseFromString`, `CSSStyleDeclaration` property set |
| events | 15 | `addEventListener`, `dispatchEvent`, `preventDefault`, `CustomEvent`, `Event` `type`/`target`/`detail`/`data`, `ProgressEvent` `loaded`/`total`/`lengthComputable`, inline `onclick`/`onchange` attributes, `window.addEventListener`, `onpopstate` |
| timers | 3 | `setTimeout`, `clearTimeout`, `setInterval` |
| network | 36 | `XMLHttpRequest` (`open`, `send`, `abort`, `setRequestHeader`, `getResponseHeader`, `overrideMimeType`, `readyState`, `status`, `response(Text/URL)`, `timeout`, `withCredentials`, `upload`, `on*` handlers), `fetch` with `Response.ok`/`json`, `WebSocket` (`send`, `onclose`, message events), `EventSource`, `FormData` (`append`, `delete`, `forEach`, `keys`), `URL` |
| storage | 3 | `localStorage` `getItem`, `setItem`, `removeItem` |
| custom elements / shadow DOM | 0 | — |
| canvas | 0 | — |
| other | 8 | `location` (`href`, `origin`, `pathname`, `protocol`, `search`), `CSS.escape`, `XPathEvaluator` (htmx looks up `hx-on:` attributes with XPath; an empty result is enough) |

- **Selectors used:**
  - `#id`, `.class` and tag selectors;
  - attribute selectors (`[a]`, `[a="v"]`) with quoting and escapes;
  - compound and descendant selectors, comma lists and `:not()`.
  - Blitz's `selectors` crate covers all of these.

### Minimal set for J2, and its size

- **DOM bindings** over Blitz's DOM, roughly 75 members:
  - the `Node`, `Element`, `HTMLElement` and `Document` members above;
  - `classList` and `dataset` views, `style` property writes and form-control `value`;
  - `table.rows`;
  - `innerHTML`, `insertAdjacentHTML` and `DOMParser`, which reuse html5ever fragment parsing;
  - `<template>.content`;
  - `querySelector`, `matches` and `closest`, through the `selectors` crate already in Blitz.
- **Events:** `EventTarget` with capture/bubble (the plan's `EventSink` path).
  - It needs `Event`, `CustomEvent`, `MessageEvent` and `ProgressEvent` fields.
  - Handler properties and inline `on*` attributes are compiled as functions with the
    element in scope (`with`-style, as browsers do; Tasmota relies on it).
- **Host globals that QuickJS-ng lacks:**
  - QuickJS-ng has no `setTimeout` family, `console`, `URL`/`URLSearchParams`,
    `structuredClone`, `TextEncoder` or `Intl`. It does have `atob`, `queueMicrotask`,
    `performance`, `WeakRef` and `FinalizationRegistry`.
  - J2 needs timers, `console`, `URL` and `URLSearchParams`, and `CSS.escape`.
- **Network:** `XMLHttpRequest`, `fetch` with `Response`/`Headers`, `WebSocket`,
  `EventSource` and `FormData`. All of them sit on the plan's non-blocking `Net` trait.
- **Also needed:** `localStorage` on `BlobStore`, `location` (read-only plus navigation
  hooks), and a no-op `XPathEvaluator` or a guard for it.

**Size:**
- **Members:** about 140 logged paths on 35 interfaces, which is roughly 100 distinct members once `(set)`, `new X` and `(missing)` variants are merged.
- **Reference implementation:** the stub implements a superset (plus logging and the HA
  extras) in about 1 750 lines of JS.
- **Native estimate:** a native Rust binding layer over Blitz is my estimate of roughly
  3–5 k lines, given that selectors and HTML fragment parsing come from Blitz's crates. The
  stub is not a model for J2's memory, since it costs 392 KB of JS heap by itself.

## Home Assistant (informational)

- **Corpus:** frontend 20261125.1 from the PyPI wheel. We ran `frontend_latest` `core` plus
  `app` as ES modules, the way HA's `index.html` loads them, with a module loader that
  resolves the chunk files.
- **Code size:**
  - Start-up imports 95 further chunks, 4.56 MB in all. Most arrive by dynamic import
    during the load phase: peak during the script phase is 8.9 MB against 33.9 MB overall.
  - Compile time is 1.5–2.2 s at 32-bit and 1.0–1.3 s at 64-bit on the host, roughly 30–170 s at the
    P4 derate.
- **Heap:** 33.9 MB at 32-bit (41 MB at 64-bit), with no login and nothing rendered. Every cap
  fails.
  - Modules keep their source text, since the stripped path only covers classic scripts.
- **APIs it reached:**
  - `customElements.define`/`get`, `new HTMLElement`, `attachShadow`, `shadowRoot`;
  - `CSSStyleSheet.prototype.replaceSync`, `MutationObserver`, `createTreeWalker`,
    `MessageChannel`, `matchMedia`, `History`;
  - Intl polyfill loading (`Intl.DateTimeFormat`, `NumberFormat`, `PluralRules`, ...), whose
    locale data it then fetches.
- **Where it stops:** translation fetches 404 against the fixtures, and it stops at the
  auth/onboarding stage before opening the WebSocket.
- **Conclusion:** this confirms ADR 0004. HA needs shadow DOM, custom elements, Intl and
  tens of MB of heap, so it goes to remote mode.

## QuickJS-ng size estimate

- **Method:** stripped, LTO, release i686 Linux binaries.
  - `examples/size_with_qjs` (a runtime plus a full context evaluating stdin) against
    `examples/size_without_qjs` (the same I/O).
- **Results** (QuickJS-ng plus rquickjs):

  | opt-level | QuickJS-ng + rquickjs | i686 binary | baseline |
  |---|---|---|---|
  | 3 | 1 209 128 B | 1 524 340 B | 315 212 B |
  | s | 716 104 B | 1 021 732 B | 305 628 B |

- **rv32imafc:** the QuickJS-ng C objects were compiled for the P4's ISA with
  `riscv64-unknown-elf-gcc -march=rv32imafc -mabi=ilp32f -Os` against picolibc headers, with a
  pthread shim. RV32 code is usually denser than i686, so the figure above is an upper bound.
  - Result: **528 KB** of text, rodata and data (`quickjs.o` 447 KB, `libunicode.o` 53 KB,
    `libregexp.o` 20 KB, `dtoa.o` 7 KB), before linker section GC.
  - Adding the rquickjs Rust glue and the libm/libc functions it pulls in gives an estimated
    **0.55–0.75 MB of P4 flash** for QuickJS-ng at `-Os`.
- **`riscv32imafc-esp-espidf` on paper:**
  - `rquickjs-sys` ships no binding for it, so it needs the `bindgen` feature. That means
    libclang plus the ESP-IDF sysroot headers in `BINDGEN_EXTRA_CLANG_ARGS`.
  - `cc` must use the esp gcc (`CC_riscv32imafc_esp_espidf`), which esp-idf-sys/embuild
    already provides in the P4 Docker image.
  - QuickJS-ng needs `pthread` (ESP-IDF has it) and `malloc_usable_size` (not in newlib).
    `rquickjs` passes its own allocator, so the latter should only matter for the default
    allocator.
  - The fallback in the plan, Espressif's QuickJS component, avoids the bindgen step.
  - The full build is J1.
- **Budget:** both figures sit inside J1's flash budget of +1.5 MB.

## P4 derate (provisional)

- **The P4:** Espressif's preliminary datasheet gives 6.92 CoreMark/MHz for the dual-core P4,
  about 1 380 CoreMark per core at 400 MHz.
- **The host:** a "QEMU Virtual CPU" at 2.3 GHz. I assume 20 000–35 000 single-thread
  CoreMark for it, which I did not measure here. On CoreMark alone that is a 15–25x ratio.
- **Memory penalty:** the parser and GC touch a working set far beyond the P4's caches, with
  heap in PSRAM. I add 1.5–3x for that.
- **Provisional derate: 20–75x.**

| workload | i686 host | P4 at 20–75x |
|---|---|---|
| htmx compile (52 KB) | 18.5–25.8 ms | 0.37–1.9 s |
| 100 KB compile (at 3 KB/ms) | ~33 ms | 0.7–2.5 s |
| htmx stripped-bytecode load | 0.88 ms | 18–66 ms |
| custom dashboard compile | 0.64–0.72 ms | 13–54 ms |
| Tasmota / ESPHome compile | 0.6–0.9 ms | 12–68 ms |

H1 should measure both compile and bytecode load directly.

## Against the plan and ADR 0004

- **Confirms ADR 0004:** the small-script corpus fits in QuickJS-ng (under 1 MB at 32-bit,
  htmx at 0.94 MB), and HA does not.
- **J1's "100 KB script parsed in ≤300 ms" probably fails for cold source parsing** at this
  derate.
  - Suggestion: make J1's criterion "first load ≤ ~2 s, cached load ≤300 ms" and cache
    stripped bytecode per script URL in `BlobStore`. Bytecode load is 10–30x faster.
  - This is a design input for J1/J2, not a blocker.
- **QuickJS-ng keeps every function's source text in RAM.** This adds 15–20% to htmx's peak
  (936 KB, against 759–798 KB stripped over two runs).
  - J2 should load page scripts as stripped bytecode. The catch is that it breaks
    `Function.prototype.toString`, which none of the J2 corpus used.
- **QuickJS-ng ships fewer host globals than the plan's J2 list implies.**
  - J2 must add `URL`, `URLSearchParams`, `console` and timers.
  - `Intl` is absent too, and providing it is out of scope for J2.
- **htmx adds three requirements to the plan's J2 list:** `DOMParser` and `<template>`
  fragment parsing, `FormData`, and an `XPathEvaluator` stub. ESPHome adds `CSS.escape` and
  `table.rows`.
- **Inline handlers need element scope.** Tasmota's slider passes `value` as a bare name.
- **JS stack ≥256 KB.** The plan gives the engine thread ≥256 KB.
  - With `JS_SPIKE_STACK_KB=256` (release, x86_64), every J2 item still passes with no
    exceptions.
  - At 128 KB, htmx and petite-vue hit "Maximum call stack size exceeded".
  - The QuickJS thread on the P4 therefore needs at least 256 KB of stack. H1 should check
    this on RV32, whose frames differ.
  - Debug builds of QuickJS-ng's C exceed even 512 KB. The workspace therefore builds
    `rquickjs-sys` at `opt-level = 2` in the dev profile.

## Rerun

On the host, from the repo root:

```sh
python tools/js-spike/fetch_corpus.py                  # Tasmota, ESPHome v1, HA wheel -> testpages/fetched/ (gitignored)
cargo run --release -p js-spike -- --out target/js-spike-host.md         # all items; --item NAME, --repeat N
cargo test -p js-spike                                 # committed items; fetched ones are skipped when absent
```

32-bit corpus run and size measurements, in Docker:

```sh
docker run --rm -v "$PWD":/src -v js-spike-target:/target -w /src rust:1.95-bookworm \
  sh tools/js-spike/docker-i686.sh                     # writes target/js-spike-i686.md, prints sizes
```

## Caveats

- **Not every API use is logged:**
  - Collections the stub returns as plain arrays or objects are not logged member by
    member: `querySelectorAll` and `children` results, `attributes` entries and `dataset` keys.
  - Neither are value globals such as `innerWidth`.
  - Read the J2 set as a floor, not a complete list.
- **Fixtures:** the canned replies decide how deep each page goes. The tool logs only paths
  actually exercised, so a branch the fixtures never take is not in the API lists.
- **Probe noise:** `(missing)` entries include library expandos, such as petite-vue's
  `composing` and htmx probing `selectionStart`. They are not all web APIs.
- **Host:** the machine is a shared VM, so timings carry noise, and the derate is a range on
  purpose.
- **Tasmota and ESPHome pages:** their host markup is synthetic and approximates what the
  firmware serves. The scripts are the real ones at pinned versions (`fetch_corpus.py`).
