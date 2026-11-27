# M0: Blitz cross-builds for the ESP32-P4 and S3

Roadmap node `eng-blitz-builds-esp`. Measured on 2026-11-27.

## Verdict against the M0 table

| Metric | Result | Band |
|---|---|---|
| Builds for P4 (S3 compile-only) | Yes. Needs one small dependency stand-in (memmap2) and a custom target spec (64-bit atomics as libcalls). **No change to Stylo or Blitz.** S3 compiles and links with the same shims (6.04 MiB image). | **needs patches** (shallow) |
| P4 image size | **6,081,152 bytes (5.80 MiB)**, 41% of the 14 MiB factory partition | **go** (≤8 MB) |

- **By the table's letter, the build row is in the "needs patches" (fallback) column, so it goes back to the owner.** My reading: the shims are build-level and touch neither Stylo nor Blitz.
- The size is a clear go.
- Strictly, the build lands in the "needs patches" band. The patches are build-level shims, not Stylo patching: about 40 lines, all listed under [What is not stock](#what-is-not-stock).
- The plan's "deep Stylo patching" stop case did not happen. Stylo 0.24 compiles unmodified for both targets.
- The S3 has a separate, larger problem: atomics in PSRAM. It is not a build problem, and the plan's hope that the risk "may be narrower than assumed" doesn't hold for Rust. See [the S32C1I lookup](#std--psram-malloc--arcatomics-on-the-s3-s32c1i).

## Atomics: `target_has_atomic`

- **P4: `riscv32imafc-esp-espidf`** (nightly-2026-10-19).
  - `target_has_atomic` = `8`, `16`, `32`, `ptr`. There is **no `64`**.
  - The features include `a`, `zaamo` and `zalrsc`, so AMO and LR/SC are native.
- **S3: `xtensa-esp32s3-espidf`** (esp toolchain 1.95.0.0, from the `espressif/idf-rust:esp32s3_1.95.0.0` image).
  - `target_has_atomic` = `8`, `16`, `32`, `ptr`. There is **no `64`**.
  - The features include `s32c1i`.
- The full outputs are reproducible with the commands below.

### Who needs 64-bit atomics?
- **Stylo itself: nothing.** I grepped the whole dependency graph for `AtomicU64` and `AtomicI64`.
- **fontique and linebender_resource_handle** already fall back to `AtomicU32` under `cfg(not(target_has_atomic = "64"))`.
- **Two crates use `std::sync::atomic::AtomicU64` unconditionally**, and they are the only build errors besides memmap2:
  - `blitz-dom` 0.3.0-beta.6 in `src/tree.rs`, a geometry generation counter.
  - `anyrender` 0.15.1 in `src/pattern.rs`, a pattern ID counter.

### The workaround
Each board crate ships a target spec under `targets/`, named like the stock target. It is the stock spec (`rustc --print target-spec-json`) with `"max-atomic-width": 64`.
- LLVM then lowers 64-bit atomics to `__atomic_*_8` libcalls.
- ESP-IDF already implements those calls for every chip without native 64-bit atomics, with a lock (`components/newlib/src/stdatomic.c`, `ATOMIC_FUNCTIONS(8, ...)` under `!HAS_ATOMICS_64`).
- 32-bit and smaller atomics are unchanged and stay native.
- The link resolves them to ESP-IDF: both ELFs define `__atomic_load_8`, `_store_8`, `_fetch_add_8`, `_fetch_or_8` and `_compare_exchange_8` (`T`, in IRAM).
- Cargo needs `-Zjson-target-spec`, which is set in `.cargo/config.toml`.
- The stem stays `riscv32imafc-esp-espidf`, so esp-idf-sys and embuild see the usual target name.

The alternative was patching both crates to use portable-atomic or `AtomicU32`. That would mean two `[patch.crates-io]` forks, including a vendored blitz-dom.
- Upstreaming a `cfg(target_has_atomic = "64")` fallback to blitz-dom and anyrender would remove the need for the spec. Both are one-line changes, the same pattern fontique already uses.

## P4 build: `boards/p4-function-ev`

### What the firmware does
The firmware is hello-Blitz: std on ESP-IDF v5.5.5, through esp-idf-sys 0.39.
1. It runs an **atomics check**. Two threads share an `Arc` of a block larger than 4 KB, so the block sits in PSRAM. They hammer `fetch_add` and `compare_exchange` on it, plus a 64-bit libcall atomic and `Arc` clones. Any lost update prints `atomics BROKEN`.
2. It spawns the engine thread, with a 512 KB stack allocated in PSRAM via `esp_pthread_set_cfg`.
3. The engine thread parses the embedded synthetic dashboard (`src/dashboard.html`) and resolves it: sequential Stylo plus Taffy at 1024x600.
4. It paints with `anyrender_vello_cpu` into a 2.4 MB RGBA buffer (PSRAM, by malloc policy). It does this twice, cold and warm.
5. After each phase it prints the time, the heap use and peak (high-water mark) for internal RAM and PSRAM separately, and the FreeRTOS task count.

Blitz's own `log-phase-times` and vello_cpu's `log_frame_times` add the split within each phase: style, construct, layout, and so on, and cmds versus render. The board needs no display.

On the Windows host, the same code drew 439,434 non-background pixels with checksum `a144357b`. The UART output should match that on the device.

**Sequential Stylo spawns no threads.** I confirmed this on a Linux host build of the same firmware (x86_64, in the P4 image), counting `/proc/self/task` after each phase:
- 1 thread at boot;
- 2 from the engine-thread start through fonts, parse, style+layout and both paints;
- 1 at the end.

So rayon's pool is never started. The device prints the same count from FreeRTOS.

### Blitz features
These are my choices; the host worker may pick differently, so reconcile at merge.
- `blitz-dom`, `blitz-html`, `blitz-paint` and `blitz-traits` `=0.3.0-beta.6`, all with `default-features = false`.
- The only features turned on are `blitz-dom/log-phase-times` and `anyrender_vello_cpu/log_frame_times`. They print timings and nothing else.
- So there is no svg, woff, accessibility, system-fonts, file-input, custom-widget or text-transform-icu.
- Fonts: Noto Sans Regular and Bold, subset to Latin-1 (about 19 KB each, OFL), registered from memory, plus Blitz's bullet font.
- Profile: `opt-level = "s"`, fat LTO, `codegen-units = 1`, `panic = abort` via build-std, and `CONFIG_COMPILER_OPTIMIZATION_SIZE`.

### Sizes
Sizes are from `riscv32-esp-elf-size -A` and `espflash save-image`.

| Section | Bytes |
|---|---|
| `.flash.text` | 4,762,404 (4.54 MiB) |
| `.flash.rodata` | 1,241,084 (1.18 MiB) |
| `.iram0.text` | 61,952 |
| `.dram0.data` / `.bss` | 14,480 / 6,296 |
| **App image** (`espflash save-image`) | **6,081,152 (5.80 MiB)**, 41.42% of the 14,680,064-byte factory partition |

### Where the size goes
This is `nm -S --size-sort -C` by crate (`scripts/size-attribution.py`). Inlined code counts for the function it was inlined into, so the split is approximate.

| Group | text KB | rodata KB (named) |
|---|---|---|
| Stylo (`style` 1331, cssparser 119, selectors 50, …) | 1528 | 21 |
| Fonts and shaping (harfrust 241, read-fonts 224, skrifa 161, parley 104, …) | 755 | 116 |
| Rust std (core 388, std 110, alloc 85, hashbrown 137) | 732 | 18 |
| Blitz (dom 287, paint 91, html) | 393 | 0 |
| Raster (vello_cpu 180, vello_common 68, kurbo 64, anyrender) | 366 | 0 |
| Taffy | 237 | 0 |
| ESP-IDF C code (FreeRTOS, newlib, drivers) | 207 | 12 |
| html5ever / xml5ever | 118 | 0 |
| encoding_rs | 22 | 125 |
| image + png | 53 | 0 |
| ICU (code; its data is unnamed rodata, see below) | 45 | 0 |

About 900 KB of `.flash.rodata` has no symbol, because LTO merges constants into anonymous blocks. Sampling the blocks shows four kinds of content:
- About 330 KB of strings: CSS property names, atom tables and panic messages. That includes about 48 KB of panic source paths.
- ICU segmenter and normalizer data. ICU code is 45 KB; its data can't be separated from other constants under fat LTO.
- The two fonts and the page, about 42 KB.
- Lookup tables.

Stylo is the largest single item at about 1.5 MB, a quarter of the image. Shaping and fonts come next at about 0.9 MB, and std at about 0.75 MB.

Cheap wins if the budget ever gets tight:
- `-Zbuild-std-features=panic_immediate_abort` plus `-Zlocation-detail=none` would drop the panic strings.
- Dropping `encoding_rs`'s CJK tables saves about 150 KB.
- harfrust's complex-script shapers could go if Blitz lets them be gated.

### Build time
- **Local:** on a 20-core Docker VM, with the target dir on a Windows bind mount, a cold build takes 30 min. The fat-LTO relink after a source change takes about 9 min.
- **CI:** the `cross-p4` job on a GitHub runner took 15 min in total. That covers the image build, about 8 min, and `cargo build`, 4 min 37 s.
- **Reproducible:** the CI app image was byte-for-byte the same size, 6,081,152 bytes.

## Size with the host-measured feature set

The 5.80 MiB P4 image above came from a build without what `tools/measure` (the source of the M0 heap numbers) has: `blitz-dom/floats`, the `image` PNG, JPEG and GIF codecs, and the full Noto Sans regular+bold instead of the Latin-1 subset. The board crate now has a cargo feature `measured-set` that turns on exactly those (`boards/p4-function-ev/Cargo.toml`); the default feature set is unchanged. `FEATURES=measured-set` makes `scripts/build-firmware.sh` build it.

| | Default set | `measured-set` | Delta |
|---|---|---|---|
| **App image** (`espflash save-image`) | 6,081,152 (5.80 MiB) | **6,608,816 (6.30 MiB)** | **+527,664 (+515 KiB, +8.7%)** |
| `.flash.text` | 4,762,404 | 4,900,892 | +138,488 |
| `.flash.rodata` | 1,241,084 | 1,630,268 | +389,184 |
| Share of the 14 MiB factory partition | 41.4% | 45.0% | |

`scripts/size-gate.sh p4-function-ev app.bin`: `app image 6.30 MiB (6608816 bytes) within budget 8.00 MiB (8388608 bytes)`. **Still a go**: 1.70 MiB (1,779,792 bytes) under the 8 MiB limit.

### Where the +515 KiB goes
Same method as above (`nm`, `scripts/size-attribution.py`; approximate under fat LTO).
1. **Full Noto Sans fonts: about +361 KiB (70%).** Regular is 203,084 bytes and Bold 204,464, against 18,792 + 18,760 for the Latin subset. This is almost all unnamed `.flash.rodata`. It is the biggest single lever: subsetting to what the product needs (Latin + a few symbols) brings it back to about 40 KB.
2. **Image codecs: about +94 KB of code.** The `image` group grew from 53 to 147 KB text. `zune-jpeg` is about 73 KB of that (zune_core adds more rodata and shared code), GIF plus `weezl` about 12 KB, PNG +6 KB, the `image` crate itself +13 KB. I checked in `nm` that the decoders are in the ELF (they would be dropped if unreachable): `zune_jpeg` 82 KB, `gif`+`weezl` 12 KB.
3. **`floats`: about +30 KB of code.** `blitz_dom` +13 KB, `taffy` +8 KB, core/std +10 KB (monomorphizations).
4. Stylo, shaping, raster, ESP-IDF and html5ever are unchanged to the KB.

The three items sum to about 485 KiB; the rest is rounding and codec tables in anonymous rodata.

**Still not counted: Wi-Fi, TLS and the HTTP client.** The hello firmware has no networking. This number is the engine plus the measured features only.

### Network stack addition (estimate)
**Measured (C, outside the Rust build):** two bare ESP-IDF v5.5.5 apps for `esp32p4` in the `eb-p4-build` image, `-Os`: an empty `app_main` (182,384 bytes) and one that calls `esp_http_client` with `esp_crt_bundle_attach` and starts `esp_netif` and the default event loop, which pulls in lwIP, esp-tls, mbedTLS and the certificate bundle (571,344 bytes). That is **+388,960 bytes (about 380 KiB) for HTTP client + TLS + lwIP**. It is an upper bound for us: the Rust image already links part of lwIP, newlib and the event loop.

**Estimate (not measured): Wi-Fi.**
- The P4 has no radio. On the Function-EV, Wi-Fi comes from an ESP32-C6 through ESP-Hosted and `esp_wifi_remote`, so the P4 image carries only the host side: roughly 100-200 KiB, from memory of typical sizes. I did not build it (it needs managed components from the registry).
- A native Wi-Fi stack (S3) is larger: a typical ESP-IDF Wi-Fi station example is about 0.4-0.5 MiB bigger than an empty app. Also from memory, unverified.

| Image | Estimate |
|---|---|
| P4: measured set + HTTP/TLS/lwIP (measured) + hosted Wi-Fi (estimate) | 6.30 + about 0.4 + 0.1-0.2 = **about 6.8-6.9 MiB** |
| S3: measured set (about 6.5 MiB, default-set S3 image was 6.04 MiB) + HTTP/TLS + native Wi-Fi | **about 7.3-7.5 MiB** |

Both stay under 8 MiB with a margin of about 0.5-1.2 MiB. The S3 is the tight one; subsetting the fonts (about -320 KiB) is the first lever. Re-measure when networking is in the firmware (M2); this section is not a gate result.

## S3 compile-only build

The S3 build is the same firmware, with nothing changed. Build details:
- Image: `espressif/idf-rust:esp32s3_1.95.0.0` (esp toolchain 1.95.0.0).
- ESP-IDF v5.5.5, fetched by esp-idf-sys.
- `MCU=esp32s3`, `sdkconfig.defaults.esp32s3` (octal PSRAM, 80 MHz).
- `targets/xtensa-esp32s3-espidf.json`, which is the esp toolchain's spec with `max-atomic-width` 64.

**It builds and links.**

| Section | Bytes |
|---|---|
| `.flash.text` | 4,987,120 (4.76 MiB) |
| `.flash.rodata` | 1,275,456 (1.22 MiB) |
| `.iram0.text` | 56,355 |
| **App image** (`espflash save-image`) | **6,336,288 (6.04 MiB)**, 43.16% of the factory partition |

- **Code split:** the same as on the P4, about 4% bigger. Stylo is 1503 KB of text, fonts and text 776 KB, std 724 KB, Blitz 432 KB, raster 414 KB, Taffy 282 KB.
- **The attribution script's rodata column is wrong on Xtensa:** nm reports DROM symbols as data there. Only the text column is meaningful for the S3.
- **This proves compile and link only.** The stock S3 target emits inline S32C1I for 32-bit atomics. `objdump -d` of the S3 ELF finds **2,787 `s32c1i` instructions, 2,764 of them in Rust functions** and 23 in ESP-IDF C. So `Arc` and `Mutex` on PSRAM-resident data are unsafe on the S3 hardware; see the doc lookups below.
- **H2 should run this image's atomics check.** It tests exactly that case. If it fails, try a software-atomics target (esp-rs/esp-idf-sys#406) before calling the S3 a fallback-engine board.

## What is not stock
Every deviation from the stock toolchain and crates:
1. **`boards/patches/memmap2`.** This is a 40-line stand-in for memmap2 0.9.12, applied with `[patch.crates-io]`.
   - fontique's `std` feature depends on memmap2, and parley's `std` and therefore Blitz require that feature.
   - memmap2's unix backend needs `mmap`, which ESP-IDF's libc doesn't have, so the build fails with about 34 errors.
   - fontique only calls `Mmap::map(&File)` to load font files. The stand-in reads the file into memory, which still works for fonts on an SD card later.
   - Upstream fix: memmap2 could treat `target_os = "espidf"` like its stub backend, or fontique could make memmap2 optional separately from `std`.
2. **`targets/*.json`**: the 64-bit atomics target specs described above. They need `-Zjson-target-spec`.
3. **Nightly pinned to `nightly-2026-10-19`.**
   - Newer nightlies can't build `std` for ESP-IDF. libc 0.2.191 renamed the `stat` time fields that `std::os::espidf::fs` reads, so you get `no field st_mtime` (rust-lang/rust#164555).
   - esp-rs/esp-idf CI pins the same nightly for the same reason.
4. **`CARGO_WORKSPACE_DIR`** in `.cargo/config.toml`. Without it, esp-idf-sys builds with a target dir outside the crate don't find `sdkconfig.defaults`. The S3 build then fails outright, because `cargo metadata` runs in `/tmp`.
5. **Partition table from espflash, not from the ESP-IDF build.** esp-idf-sys generates the CMake project inside the target dir, and a relative `CONFIG_PARTITION_TABLE_CUSTOM_FILENAME` is looked up there. So `partitions.csv` is passed to espflash instead, through `espflash.toml` and `--partition-table`.

None of these touches Stylo, Blitz, Taffy or Parley code.

## H1 readiness
This section is for when a P4 board arrives.

**1. Chip revision: check it first.**
- ESP-IDF v5.5 builds for **v3.01+ silicon** by default (`CONFIG_ESP32P4_REV_MIN_301`). Such an image does not boot on v0.x/v1.x chips, and a v1 image does not boot on v3.
- Run `espflash board-info` to read the revision.
- For v1.x, add the opt-in defaults file. It builds, with an image of 6,079,440 bytes, and the generated sdkconfig has `ESP32P4_SELECTS_REV_LESS_V3=y`, `REV_MIN_FULL=1` and `SPIRAM_SPEED_200M`. It has not been booted:
  `-e ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.defaults.p4-rev1"`

**2. Applied config.** I checked the generated `sdkconfig`. It has:
- `SPIRAM=y`, `SPIRAM_SPEED_200M`, `SPIRAM_USE_MALLOC`, `SPIRAM_MALLOC_ALWAYSINTERNAL=4096` and `SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY`;
- main and pthread stacks of 32 KB, and the engine thread at 512 KB in PSRAM;
- `ESPTOOLPY_FLASHSIZE=16MB`;
- the idle-task watchdog off on both cores (`# CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU0/1 is not set`), because one frame can take seconds.

**3. Flash and watch.** Run this from the repo root, outside Docker. `out/` holds the ELF and the bootloader that ESP-IDF built from our sdkconfig; espflash would otherwise flash its bundled bootloader.
`espflash flash --monitor --chip esp32p4 --flash-size 16mb --bootloader out/p4-function-ev-esp32p4/bootloader.bin --partition-table boards/p4-function-ev/partitions.csv out/p4-function-ev-esp32p4/firmware.elf`

The firmware has never booted on any ESP target, so H1 is its first boot.

**4. Read off H1.**
- **Full frame.** Read it as `phase style+layout` plus `phase paint (cold)`; the warm repaint shows the steady state.
- **PSRAM atomics.** The `atomics OK|BROKEN` line answers the H1 row "PSRAM atomics with Stylo". The line prints the block's address: check that it is in PSRAM (`0x48000000`-`0x4C000000` on the P4), otherwise "OK" proves nothing. The resolve and paint passes, which are `Arc`-heavy Stylo on PSRAM, finishing at all is the second half of that answer.
- **Threads.** The `tasks` count should stay the same across the engine phases. That confirms on the device that sequential Stylo spawns no threads; the two atomics-check workers have exited by then.
- **Heap.** The peak is printed per region.

## Doc lookups

These are the doc lookups the plan asks for before H1. They are from ESP-IDF v5.5 pages and the `release/v5.5` source, unless a line says otherwise. The atomics source was checked in the build image (`/opt/esp/idf`, v5.5.5).

### Flash MMU limit with PSRAM also mapped
- **S3: one shared 32 MB window.** Flash code, rodata and PSRAM all map into it, with 512 MMU entries of 64 KB each.
  - The docs say: "The 32 MB virtual addresses are shared with flash instructions and rodata." ([S3 external RAM](https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32s3/api-guides/external-ram.html))
  - The source gives `SOC_MMU_ENTRY_NUM 512` in `components/soc/esp32s3/include/soc/ext_mem_defs.h`.
  - So app image + PSRAM must be ≤ 32 MB. An 8 MB app with 8 MB PSRAM fits easily. A 14 MB app with 8 MB PSRAM still fits, at 22 MB.
- **P4: flash and PSRAM have separate virtual ranges.**
  - Flash is at `SOC_MMU_FLASH_VADDR_BASE 0x40000000` and PSRAM at `SOC_MMU_PSRAM_VADDR_BASE 0x48000000`. Each range is 64 MB, with 1024 entries (`components/soc/esp32p4/include/soc/ext_mem_defs.h`).
  - The docs say: "ESP32-P4 has the ability to use up to 64 MB of virtual addresses for external PSRAM." ([P4 external RAM](https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32p4/api-guides/external-ram.html))
  - So a 14 MB app with 32 MB PSRAM is not limited by the address space.
- **XIP from PSRAM doesn't help.** `CONFIG_SPIRAM_XIP_FROM_PSRAM` copies .text/.rodata into PSRAM, which costs PSRAM space and doesn't widen the window. We keep XIP from flash.

### mbedTLS RAM
- **Per connection:** about 42 KB of heap for the `https_request` example with server verification, in the default config. It is about 22 KB with `CONFIG_MBEDTLS_DYNAMIC_BUFFER` and the free-config/free-CA options. The 16 KB in and 4–16 KB out record buffers are most of it. ([mbedTLS, memory usage](https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32s3/api-reference/protocols/mbedtls.html))
- **PSRAM is possible.** `CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC` needs `SPIRAM_USE_CAPS_ALLOC || SPIRAM_USE_MALLOC`. Alternatively, `CONFIG_MBEDTLS_DEFAULT_MEM_ALLOC` follows the malloc policy, and our policy sends allocations over 4 KB to PSRAM.
  - The Kconfig help still recommends internal RAM "from a security perspective" (`components/mbedtls/Kconfig`).
- **For us:** a few parallel connections cost about 100–200 KB, which is negligible next to the engine. Use dynamic buffers plus PSRAM on the S3.
- **Not confirmed:** a measured per-connection peak with PSRAM allocation. Measure it at M2.

### Std + PSRAM malloc + `Arc`/atomics on the S3 (S32C1I)
- **The risk is real, and it is not narrower than assumed for Rust.** Xtensa `S32C1I` (compare-and-swap) does not work on PSRAM addresses on the ESP32 and the ESP32-S3.
  - ESP-IDF's own C code avoids it in software:
    - `esp_cpu_compare_and_set` takes a lock (`external_ram_cas_lock`) for addresses in external RAM (`components/esp_hw_support/cpu.c`).
    - `CONFIG_STDATOMIC_S32C1I_SPIRAM_WORKAROUND` (ESP32/S3 with SPIRAM) routes the `__atomic_*` libcalls through a lock for PSRAM addresses (`components/newlib/src/port/xtensa/stdatomic_s32c1i.c`).
  - The stock `xtensa-esp32s3-espidf` Rust target emits S32C1I **inline**, so it bypasses both. Every `Arc` refcount, `Mutex` and atomic in a PSRAM allocation is exposed.
  - With `CONFIG_SPIRAM_USE_MALLOC`, every allocation over 4 KB lands in PSRAM, and DOM/style arenas are such allocations.
- **Upstream status.**
  - [esp-rs/esp-idf-sys#406](https://github.com/esp-rs/esp-idf-sys/issues/406) (open, 2026-03-10) says: "Hardware atomics do not work on the s3 and the stock esp32 when emplaced inside PSRAM due to PSRAM/chip hardware limitations / hacks."
    - The issue proposes a JSON target that makes LLVM emit atomic libcalls, which ESP-IDF already implements.
    - It notes upstream would not accept that, because the libcalls take locks.
  - [esp-rs/esp-idf-sys#400](https://github.com/esp-rs/esp-idf-sys/issues/400) is a request for a PSRAM allocator. Its author reports corruption with SPIRAM in the global heap, and the maintainer replies that users must be told: "Do not place atomics in memory allocated in PSRAM". He suggests a compile-time error unless a special JSON target is used.
- **How narrow is it?**
  - It affects only the ESP32 and the S3. The S2, C-series and P4 are not affected.
  - On the S3 it affects every PSRAM-resident atomic, so all of Stylo's `Arc`-heavy data.
  - Two ways out:
    1. Allocate only plain data in PSRAM, which isn't practical for Stylo.
    2. Use a custom S3 target with software atomics, the #406 approach (lock-based, slower).
  - H2 must test that custom-target option, not just the stock target. Until then, the S3 stays at risk.

### P4 atomics (AMO, LR/SC) on PSRAM
- **No restriction is documented.** The P4 external-RAM and memory-allocation pages list none.
- **The source treats the P4 as plain hardware atomics.** ESP-IDF's `esp_stdatomic.h` sets `HAS_ATOMICS_32` to `(__riscv_atomic == 1)`, with no PSRAM carve-out. The SPIRAM workaround Kconfig is limited to ESP32/S3.
- **Cache coherence:** the P4 docs only discuss CPU vs DMA coherence ([mm_sync](https://docs.espressif.com/projects/esp-idf/en/v5.5/esp32p4/api-reference/system/mm_sync.html)). Coherence between the two HP cores on PSRAM is not documented either way. Assumption: the shared cache keeps them coherent.
- **Not confirmed:** nothing found in the TRM or in issues either way, which is absence of evidence. The H1 row "PSRAM atomics with Stylo" stays. The hello firmware runs a two-thread atomics stress test on a PSRAM allocation for it (see [H1 readiness](#h1-readiness)).

## Rerun commands
Everything runs in Docker (ADR 0006) from the repo root, in Git Bash. On Linux, drop `MSYS_NO_PATHCONV=1` and use `$PWD`.

```sh
# P4 image (about 15 min cold: ESP-IDF v5.5.5 preinstalled, nightly-2026-10-19, ldproxy, espflash 4.7)
docker build -f docker/p4.Dockerfile -t eb-p4-build .

# P4 firmware, app image, sizes, attribution -> out/p4-function-ev-esp32p4/
MSYS_NO_PATHCONV=1 docker run --rm -v "$(pwd -W):/repo" \
  -v eb-p4-cargo-registry:/opt/rust/cargo/registry -v eb-p4-target:/tmp/eb-target \
  eb-p4-build bash /repo/scripts/build-firmware.sh
scripts/size-gate.sh p4-function-ev out/p4-function-ev-esp32p4/app.bin

# Atomics cfg
docker run --rm eb-p4-build rustc --print cfg --target riscv32imafc-esp-espidf | grep atomic
docker run --rm espressif/idf-rust:esp32s3_1.95.0.0 rustc +esp --print cfg --target xtensa-esp32s3-espidf | grep atomic

# S3 compile-only (ESP-IDF fetched by esp-idf-sys into the eb-p4-s3-espressif volume)
MSYS_NO_PATHCONV=1 docker run --rm -u root -e HOME=/home/esp -v "$(pwd -W):/repo" \
  -v eb-p4-s3-espressif:/home/esp/.espressif -v eb-p4-s3-registry:/home/esp/.cargo/registry \
  -v eb-p4-s3-target:/tmp/eb-target -e ESP_IDF_TOOLS_INSTALL_DIR=global \
  -e TOOLCHAIN=+esp -e CHIP=esp32s3 -e TARGET=targets/xtensa-esp32s3-espidf.json \
  espressif/idf-rust:esp32s3_1.95.0.0 \
  bash -c "git config --global --add safe.directory '*' && . ~/export-esp.sh && bash /repo/scripts/build-firmware.sh"

# The same firmware on the host (sanity check of the pipeline; prints the timings)
cd boards/p4-function-ev && cargo +1.95 run --release --target x86_64-unknown-linux-gnu
```

The Docker VM disk on this host is 89 GB and shared with other projects. It filled up during this spike, so the measured builds used Windows bind mounts for the target dirs instead of volumes, which is slower.
