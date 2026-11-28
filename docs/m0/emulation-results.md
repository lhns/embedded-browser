# M0 emulation check (node `ci-emulation-check`)

Date: 2026-11-27. Versions: Espressif QEMU `esp-develop-9.2.2-20261109`, `espressif/idf-rust:esp32s3_1.95.0.0`
(esp toolchain 1.95 nightly), ESP-IDF v5.5.5 (esp-idf-sys 0.39.0 default), esp-idf-svc 0.54.2,
`espressif/esp_lcd_qemu_rgb` 1.0.2.

## Verdict

| | CI can emulate | Cannot |
|---|---|---|
| ESP32-S3 | Boot, octal PSRAM (8 MB), serial, RGB framebuffer screenshot (`screendump`) compared with a golden, Ethernet (open_eth). Good for: the rendering pipeline on Xtensa, PSRAM allocation, golden screenshots. | I2C (so no touch controller), GP SPI, I2S, LEDC, Wi-Fi, SAR ADC (see quirk 2), real timing. The panel is a QEMU-only virtual RGB device, not the BOX-3 SPI ILI9341. |
| ESP32-P4 | Boots a plain IDF v5.5.5 C hello world in the pinned QEMU (`-machine esp32p4`, ROM `esp32p4-eco5`, chip rev v3.1, dual core 400 MHz). Runs the Rust hello-Blitz to the end (serial) with two emulator-only shims (PSRAM heap, lazy-FPU trap; see [Hello-Blitz on P4 QEMU](#hello-blitz-on-p4-qemu)). `-icount` gives deterministic instruction counts. Good for serial smoke tests and instruction-count timing. | The PSRAM chip probe (the window is RAM, the controller does not answer), MIPI-DSI, GT911/I2C. No display device is exposed in `-device help`. No screenshots. |

So the S3 gets a golden-screenshot job; the P4 gets a serial smoke test (`scripts/qemu-p4-hello-blitz.sh`, not yet in CI).
Touch cannot be emulated in QEMU for either chip. Wokwi is the only route to touch plus screenshot (cloud, token,
small free tier), and only for the S3 + ILI9341 + FT6206 config.

## S3 flow (works end to end)

```sh
scripts/qemu-s3-screenshot.sh                 # build in Docker, boot, wait READY, screendump, compare to golden
scripts/qemu-s3-screenshot.sh --update-golden # regenerate boards/s3-qemu-test/golden/s3-qemu-test.png
```

The script runs inside `docker/s3.Dockerfile` (named volume `eb-s3-cache` for target dir, ESP-IDF tools, cargo registry):

1. `cargo build --release` of `boards/s3-qemu-test` (Rust std, esp-idf-svc, target `xtensa-esp32s3-espidf`).
2. `scripts/qemu-patch-adc.py` patches an emulator-only copy of the ELF (quirk 2).
3. `espflash save-image --chip esp32s3 --merge --flash-size 8mb --bootloader ... --partition-table ... app.elf flash.bin`.
4. QEMU, headless:
   ```sh
   SDL_VIDEODRIVER=dummy qemu-system-xtensa -machine esp32s3 -m 8M \
     -global driver=ssi_psram,property=is_octal,value=true \
     -drive file=flash.bin,if=mtd,format=raw \
     -display sdl -serial file:serial.log -monitor unix:/tmp/qemu-mon.sock,server,nowait
   ```
5. Poll `serial.log` for `READY`, send `screendump screen.ppm` on the monitor socket, convert to PNG (ImageMagick),
   `compare -metric AE -fuzz 2%` against the golden; fail above 0.1 % differing pixels.

Firmware (`boards/s3-qemu-test`): embedded-graphics `DrawTarget<Color = Rgb565>` over a 320x240 buffer allocated with
`heap_caps_malloc(MALLOC_CAP_SPIRAM)`, colour bars, grey ramp and text, plus a 1 MiB PSRAM write/readback probe. The
buffer is then copied to the panel framebuffer of the `esp_lcd_qemu_rgb` component and flushed with
`esp_lcd_rgb_qemu_refresh`. Serial shows `PSRAM size=8388608 probe_ok=true free_spiram=8386156` and `READY`. The
component is added through `package.metadata.esp-idf-sys.extra_components` with `bindings_header`/`bindings_module`;
Rust sees it as `esp_idf_svc::sys::qemu_rgb`.

Measured (Windows host, Docker Desktop, host busy with other builds):

| | |
|---|---|
| QEMU start to `READY` | 5.6 s and 10.1 s wall-clock in two runs; about 1.1 s guest time to `app_main`, 163 ms guest time from `app_main` to flush |
| Firmware image | 413 KB app (39 % of a 1 MB partition) |
| First build | about 11 min of cargo plus IDF download and build; the cache volume is about 5 GB (`.embuild` 4.5 GB: IDF checkout and tools) |

### Quirks found

1. **`-display none` stalls the panel driver.** Use `-display sdl` with `SDL_VIDEODRIVER=dummy` (works headless, no X).
2. **The S3 image never reaches `app_main` once `esp_adc` is linked.** esp-idf-sys links every IDF component, and
   `esp_adc` has a constructor (`adc_hw_calibration`) that spins on the SAR ADC, which QEMU does not model. The log
   stops after `eFuse: calibration efuse version does not match` (found via QEMU monitor `info registers`, PC in
   `read_cal_channel`). Others hit the same problem (Hive-RSI-Lab/hive-fw issue 48, FastLED fbuild). `esp_lcd`
   requires `driver`, which requires `esp_adc`, so the component cannot be dropped. Workaround:
   `scripts/qemu-patch-adc.py` replaces that constructor's body with `retw.n` in the ELF used for the emulator image
   only. Hardware images are unaffected. The P4 Rust image will need the same if it links `esp_adc` and is emulated.
3. **QEMU's panel DMA cannot read PSRAM.** Passing a PSRAM buffer to `esp_lcd_panel_draw_bitmap` logs
   `[ESP RGB] Invalid color content address or length` and draws nothing. Copy to the dedicated framebuffer
   (`esp_lcd_rgb_qemu_get_frame_buffer`) and call `esp_lcd_rgb_qemu_refresh`.
4. The `-global driver=timer.esp32c3.timg,property=wdt_disable` option from the docs warns "invalid class name" in
   this release and is not needed.
5. On a Windows host, Git Bash needs `MSYS_NO_PATHCONV=1` and `pwd -W` for bind mounts. Builds never run on a Windows
   bind mount.
6. Bind-mounted output dirs owned by another uid: the script `chmod 777`s the output dir; CI chowns the cache dir to uid 1000.

## Hello-Blitz on S3 QEMU

One attempt, serial only (`scripts/qemu-s3-hello-blitz.sh`). Image: the S3 hello-Blitz firmware from `docs/m0/esp-build-results.md` (`boards/p4-function-ev`, default feature set, `xtensa-esp32s3-espidf` custom spec, 6.04 MiB), prebuilt, not rebuilt from this branch. Same QEMU and flags as the screenshot flow, plus a 16 MB flash image (`espflash save-image --merge --flash-size 16mb` with `boards/p4-function-ev/partitions.csv`) and `-nographic`. The ADC constructor patch (`scripts/qemu-patch-adc.py`) applies to this ELF too.

**Result: it boots and gets through parse, style and layout, then crashes in the first paint.** Same crash, same PC in two runs.

```
I (4741) esp_psram: Found 8MB PSRAM device
I (5062) main_task: Calling app_main()
eb-hello: atomics OK at 0x3c6109a0: u32 400000/400000, u64 400000/400000, arc count 1, 5672.1 ms
eb-hello: phase fonts: 1244.9 ms
eb-hello: phase parse: 932.7 ms
Resolve(1): 3.2s (style: 823ms, damage: 5.8ms, construct: 882ms, pconstruct: 311ms, layout: 1.0s, ...)
eb-hello: phase style+layout: 3177.8 ms
eb-hello: heap style+layout internal: used 446 KB, peak 447 KB, total 457 KB
eb-hello: heap style+layout psram: used 650 KB, peak 654 KB, total 8192 KB
Guru Meditation Error: Core  0 panic'ed (LoadProhibited).   PC 0x4247eb25  EXCVADDR 0x8037e980
```

- **Backtrace** (addr2line): `vello_cpu::fine::lowp::U8Kernel::alpha_composite_buffer` <- `Fine::indexed_fill` <- `paint_fill` <- `bucket_and_rasterize` <- `RenderContext::render_with` <- `run`. No "paint" phase line and no checksum line were printed; the firmware then reboots in a loop. The cause is undetermined: it could be a QEMU issue or a real Xtensa problem (stack, alignment, the pixel buffer). I stopped here as agreed. (Since found: an Xtensa LLVM codegen bug, see [below](#s3-paint-crash-cause-and-fix).)
- **Phase times are emulator wall-clock** and varied about 2x between two runs (style+layout 3.2 s and 1.9 s), so they say nothing about the device.
- **Heap:** internal RAM was at 446 of 457 KB after style+layout; PSRAM held only 650 KB. With `SPIRAM_MALLOC_ALWAYSINTERNAL=4096`, Stylo's many small allocations fill internal RAM, which leaves no room for Wi-Fi and DMA buffers on a real S3. This holds whatever caused the crash; lowering the threshold is a thing to try on hardware.
- **Atomics check:** `atomics OK` at `0x3c6109a0`. That address is past the end of the flash-mapped rodata segment (`3c4d0020` + 1,275,744 bytes), so the block is in PSRAM. **QEMU does not model the S32C1I-on-PSRAM hardware limitation, so the pass proves nothing about real S3 hardware** (see the S32C1I section in `docs/m0/esp-build-results.md`). It does show the software path (2 threads, `Arc`, 64-bit libcall atomics) runs.
- Threads: `tasks` was 6 from engine start through style+layout.
- **Script status:** `scripts/qemu-s3-hello-blitz.sh [build-out-dir]` runs the flow and exits 1 at the paint crash. It exits 0 once the firmware prints `eb-hello: done`.

## S3 paint crash: cause and fix

Node `plat-defect-s3-paint-crash`, 2026-11-27/28. **Cause: an Xtensa LLVM codegen bug in stack realignment, triggered
by fearless_simd's 32- and 64-byte-aligned vector types. Not a QEMU artefact, not the heap.** The P4 (RISC-V) runs the
same code to the end (see [Hello-Blitz on P4 QEMU](#hello-blitz-on-p4-qemu)).

### Fault details
Register dump of the crash (`EXCCAUSE 0x1c` LoadProhibited):
`PC 0x4247eb25`, `EXCVADDR 0x8037e980`, `A6 0x8037e979`, `A1 (SP) 0x3c68c180`.

- The faulting instruction is `l8ui a8, a6, 7` in `U8Kernel::alpha_composite_buffer`. `a6` is the `alphas: Option<&[u8]>`
  pointer argument, untouched since `entry`, so the function received a garbage pointer: `EXCVADDR = a6 + 7`.
- `0x8037e979` is not null, PSRAM or out of bounds. It is shaped like a windowed-ABI return address: top bits `10`
  (call8) over `0x4037e979`, which is in `multi_heap_internal_unlock`. So a stale stack word was read as the pointer.
- Backtrace (addr2line): `alpha_composite_buffer` <- `Fine::indexed_fill` <- `paint_fill` <- `bucket_and_rasterize`
  <- `RenderContext::render_with` <- `run` <- `pthread_task_func`.
- The caller, `Fine<Fallback, U8Kernel>::indexed_fill`, starts with:
  ```
  entry   a1, 0x9e0
  movi.n  a8, 63 ; movi.n a9, 64 ; and a8, a1, a8 ; sub a8, a9, a8
  add.n   a1, a1, a8          ; SP rounded *up* to 64 bytes, by 1..64, without movsp
  ...
  l32i.n  a13, [a1 + 0x9e0]   ; a stack-passed argument, read relative to the moved SP
  s32i.n  a13, [a1 + 0x47c]   ; later passed on as `alphas`
  ```
  The `alphas` slice arrives on the stack (argument 7+). Its address is computed from the realigned SP, so it is read
  1-64 bytes above where the caller stored it. The numbers fit: `paint_fill`'s SP is `0x3c68cd50`, minus `0x9e0` is
  `0x3c68c370`, realigned by +16 to `0x3c68c380` (`indexed_fill`'s SP in the backtrace), so the argument is read from
  `0x3c68cd60`, 16 bytes off. It is deterministic codegen, hence the same PC in every run.
- Why there are over-aligned locals: fearless_simd 1.3.0 declares its vector types `#[repr(C, align(32))]` (`u8x32`,
  `f32x8`, ...) and `align(64)` (`u8x64`, ...), also on the scalar `Fallback` level that Xtensa uses. vello_cpu's lowp
  kernels keep them on the stack. Xtensa's stack alignment is 16.
- Minimal repro, outside the firmware: a `#[repr(align(64))]` local plus 8 integer arguments
  (`cargo +esp build -Zbuild-std=core --target xtensa-esp32s3-none-elf`, `--emit asm`) gives the same prologue and
  reads the 8th argument at `l32i a7, a1, 224` after moving a1. `-C force-frame-pointers=yes` does not fix it (it
  then realigns using `a7`, an argument register). There is no LLVM option to turn realignment off.
- Upstream status (read only): espressif/llvm-project#140 (open, 2026-10-04) reports the same prologue, from the
  window-spill side; comments confirm it is still present in esp toolchain 1.97.0.0 (LLVM 21.1.3), with a fix
  proposed but not released. We use 1.95.0.0. Newer esp toolchains exist (rust-build v1.100.0.0, 2026-11-10); whether
  they carry the fix is not checked here. A toolchain upgrade is the other way out once a fixed one ships.

The whole old S3 image has exactly 4 realigning functions (`scripts/scan-xtensa-realign.py`), all vello_cpu over
fearless_simd `Fallback`: `Fine::indexed_fill` (frame 0x9e0), `lowp::blend::mix` (0x420), `GradientPainter::new`
(0x120) and `GradientPainter::paint_u8` (0x2a0). Stylo, std and the rest have none.

### Fix: cap fearless_simd's alignment to 16 on Xtensa (needs our fork)
In fearless_simd, every `#[repr(C, align(32))]` and `#[repr(C, align(64))]` becomes
```rust
#[cfg_attr(not(target_arch = "xtensa"), repr(C, align(32)))]   // resp. align(64)
#[cfg_attr(target_arch = "xtensa", repr(C, align(16)))]
```
in `src/generated/simd_types.rs` (20 vector types; it is generated, so the generator `fearless_simd_gen` needs the
same change) and `src/support.rs` (`Aligned256`, `Aligned512`, `Compact32StitchControls`); 23 sites in all. The crate's
only alignment checks are `const` asserts that a cast's source is at least as aligned as its target, which still hold
when everything is 16. The `Fallback` level is plain arrays, so nothing needs more than natural alignment.

Validated with an uncommitted copy of fearless_simd 1.3.0 under `[patch.crates-io]`, made with
```sh
for n in 32 64; do sed -i "s/^#\[repr(C, align($n))\]$/#[cfg_attr(not(target_arch = \"xtensa\"), repr(C, align($n)))]\n#[cfg_attr(target_arch = \"xtensa\", repr(C, align(16)))]/" \
  src/generated/simd_types.rs src/support.rs; done
```
and a `[patch.crates-io] fearless_simd = { path = ... }` entry in the board's `.cargo/config.toml` (build without
`--locked`):
- **0 realigning functions** in the rebuilt S3 ELF (was 4);
- **S3 QEMU runs to `eb-hello: done`**: both paints, frame checksum `a144357b` and 439,434 non-background pixels,
  identical to the host and the P4.

```
eb-hello: atomics OK at 0x3c610f58: u32 400000/400000, u64 400000/400000, arc count 1, 10210.7 ms
eb-hello: phase style+layout: 3418.8 ms
eb-hello: heap style+layout internal: used 310 KB, peak 405 KB, total 489 KB
eb-hello: heap style+layout psram: used 817 KB, peak 823 KB, total 8192 KB
eb-hello: phase paint (cold): 37413.7 ms
eb-hello: heap paint (cold) internal: used 326 KB, peak 431 KB, total 489 KB
eb-hello: heap paint (cold) psram: used 3940 KB, peak 3994 KB, total 8192 KB
eb-hello: phase paint (warm): 27176.1 ms
eb-hello: frame 2457600 bytes at 0x3c6e165c, 439434 non-background pixels, checksum a144357b
eb-hello: done
```

The fix is not in this repo: per ADR 0002 it goes into our fork of fearless_simd (`[patch.crates-io]` at a fixed
revision). Until then the S3 image still crashes at the first paint. Check any S3 image with:

```sh
xtensa-esp32s3-elf-objdump -d --no-show-raw-insn firmware.elf | python3 scripts/scan-xtensa-realign.py   # exit 1 if any
```

Any other crate with `align(>16)` stack values (std's `CachePadded` channels, tokio) hits the same bug on Xtensa, so the
scan belongs in the S3 build checks.

## P4 in QEMU 9.2.2 (esp-develop-9.2.2-20261109)

The `qemu-riscv32-softmmu` release (a separate tarball from the xtensa one) lists `esp32p4` among its machines
(`qemu-system-riscv32 -machine help`); the xtensa tarball only has esp32 and esp32s3. A plain C hello world from
`$IDF_PATH/examples/get-started/hello_world` in `espressif/idf:v5.5.5`:

```sh
docker run --rm -v "$PWD/out:/out" espressif/idf:v5.5.5 bash -c '
  cp -r $IDF_PATH/examples/get-started/hello_world /tmp/hello && cd /tmp/hello &&
  idf.py set-target esp32p4 && idf.py build && cd build &&
  esptool.py --chip esp32p4 merge_bin --fill-flash-size 16MB -o /out/p4_flash.bin @flash_args'
# QEMU from qemu-riscv32-softmmu-esp_develop_9.2.2_20261109-x86_64-linux-gnu.tar.xz
qemu-system-riscv32 -nographic -machine esp32p4 -drive file=p4_flash.bin,if=mtd,format=raw
```

Result: ROM `esp32p4-eco5-20250430` runs, the IDF 2nd stage bootloader and the app boot (chip revision v3.1, 2 cores,
400 MHz), `Hello world!` and the 10 s restart countdown print; the app is up about 1.8 s into the guest run. Notes:
IDF 5.5's `idf.py qemu` has no P4 entry, so the command line is manual; `-device help` shows no LCD, DSI or RGB device,
so there is no screenshot path for the P4; PSRAM (`-m`) and DSI were not tried.

## Hello-Blitz on P4 QEMU

Node `plat-defect-s3-paint-crash`, 2026-11-27. Serial only, `scripts/qemu-p4-hello-blitz.sh` (runs in the `eb-s3` image,
which now also carries `qemu-riscv32-softmmu` of the same release).

**Result: all phases run, paint included, and the frame matches the host** (439,434 non-background pixels, checksum
`a144357b`). So the S3 paint crash is Xtensa-specific (see [the cause](#s3-paint-crash-cause-and-fix)).

It needs two emulator-only shims; neither touches the hardware image (the hardware builds' generated sdkconfig has
`# CONFIG_SPIRAM_IGNORE_NOTFOUND is not set`):
1. **PSRAM.** QEMU maps a 64 MB plain-RAM window at `0x48000000` (`hw/misc/esp32p4_psram_mem.c`) but does not answer
   the hex-PSRAM chip probe, so IDF stops with `hex_psram: PSRAM chip is not connected` / `Failed to init external RAM!`.
   The `qemu` cargo feature plus `sdkconfig.defaults.qemu` (`CONFIG_SPIRAM_IGNORE_NOTFOUND=y`) boot on without it, and
   `main()` adds 32 MB of the window with `heap_caps_add_region` and calls `heap_caps_malloc_extmem_enable(256)`.
   A runtime-added heap has no minimum-free tracking, so the "psram peak" lines read 32768 KB and mean nothing.
2. **Lazy FPU trap.** The first `f64` operation (`Duration::as_secs_f64`, a ROM soft-double routine) panics with
   `Illegal instruction`, MTVAL `0x00202973` (`csrr s2, frm`). IDF enables the FPU on first use: its trap handler
   reads the EXT_ILL CSR (0x7F0) for an "FPU" reason. QEMU sets that reason for FP loads/stores and FP arithmetic
   only, not for `fflags`/`frm`/`fcsr` CSR accesses (`esp_cpu_insn_is_fpu` in `target/riscv/esp_cpu.c`).
   `scripts/qemu-patch-p4-fpu.py` patches the handler's `csrrw a0, 0x7f0, zero` into `li a0, 1`.
   That silicon v3+ sets the FPU reason for these CSR accesses is inferred, not observed: IDF's software decode for
   rev < 3 counts them as FPU use. **H1 watch item:** an `Illegal instruction` panic with MTVAL `0x00202973` on the
   board means the hardware behaves like QEMU here, and then it is an IDF problem for us, not a QEMU one.

The ADC constructor patch is not needed: `adc_hw_calibration` is not linked in the P4 image. (`qemu-patch-adc.py` has
a RISC-V branch now, untested for that reason.)

```sh
# Build (P4 image, same volumes as the hardware build; output in out/p4-function-ev-esp32p4/)
MSYS_NO_PATHCONV=1 docker run --rm -v "$(pwd -W):/repo" \
  -v eb-p4-cargo-registry:/opt/rust/cargo/registry -v eb-p4-target:/tmp/eb-target \
  -e FEATURES=qemu -e ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.defaults.qemu" \
  eb-p4-build bash /repo/scripts/build-firmware.sh
scripts/qemu-p4-hello-blitz.sh            # wall-clock run
ICOUNT=0 scripts/qemu-p4-hello-blitz.sh   # instruction-count run, see below
```

Wall-clock run (QEMU on a busy host, so the times say nothing about the device):

```
eb-hello: atomics OK at 0x48000e28: u32 400000/400000, u64 400000/400000, arc count 1, 15089.5 ms
eb-hello: phase fonts: 359.9 ms
eb-hello: phase parse: 485.8 ms
eb-hello: phase style+layout: 5275.7 ms
eb-hello: heap style+layout internal: used 213 KB, peak 213 KB, total 597 KB
eb-hello: heap style+layout psram: used 817 KB
eb-hello: phase paint (cold): 23775.5 ms
eb-hello: heap paint (cold) internal: used 229 KB, peak 239 KB, total 597 KB
eb-hello: heap paint (cold) psram: used 3940 KB
eb-hello: phase paint (warm): 28363.1 ms
eb-hello: frame 2457600 bytes at 0x480d153c, 439434 non-background pixels, checksum a144357b
eb-hello: tasks end: 5
eb-hello: done
```

- **Atomics check:** OK at `0x48000e28`, which is in the PSRAM window. QEMU models AMO and LR/SC as plain atomics, so
  this checks the code path, not the P4 silicon.
- **Heap** (with the lowered malloc threshold, `SPIRAM_MALLOC_ALWAYSINTERNAL=256`): internal RAM peaks at 239 of
  597 KB, PSRAM holds 817 KB after style+layout and 3.9 MB during paint (the 2.4 MB frame plus vello_cpu's buffers).
- **Threads:** `tasks` stays 6 from engine start through both paints.

### Instruction counts (`-icount shift=0`)

With `-icount shift=0`, QEMU advances guest time by 1 ns per executed instruction, so the firmware's own phase times
(esp_timer) read as instruction counts: 1 ms = 1 M instructions, summed over both cores (the second core idles in
`wfi`, which executes nothing; tick interrupts count). **Two runs printed byte-identical phase lines**, so the counts
are deterministic.

| Phase | Instructions | Time at 400 MHz, IPC 1.0 | IPC 0.5 |
|---|---|---|---|
| fonts | 5.4 M | 14 ms | 27 ms |
| parse | 3.5 M | 9 ms | 18 ms |
| style+layout (`Resolve` 7.4 M: style 2.0, construct 0.5, pconstruct 2.7, layout 1.8) | 7.5 M | 19 ms | 38 ms |
| paint, cold (1024x600, vello_cpu `Fallback` scalar kernels) | 383.1 M | 0.96 s | 1.9 s |
| paint, warm | 412.8 M | 1.03 s | 2.1 s |
| atomics check (2 x 200k rounds, for scale) | 83.9 M | | |

**Provisional estimate, not a measurement.** The IPC range is an assumption: the P4 HP core is an in-order RISC-V
pipeline, so about 1 instruction per cycle on cache hits is the optimistic bound, and 0.5 allows for PSRAM and flash
cache misses (the frame buffer alone is 2.4 MB of PSRAM, far beyond the cache). The count also includes no DSI or
display cost. So a full-frame repaint lands at **about 1-2 s** on the P4, inside the 0.2-2.5 s range from the host
ratio and toward its upper end, and resolve is cheap (tens of ms). Paint dominates by 50x, which favours banded,
damage-limited re-raster (ADR 0002). H1 replaces these numbers.

## Wokwi (read from docs; no sign-up, no token)

Sources: docs.wokwi.com (ESP32 guide, automation scenarios, CI getting started, CLI usage, part pages).

- **S3**: ESP32-S3-DevKitC-1 and XIAO ESP32-S3; GPIO, UART, SPI, I2C (master only), PSRAM (2/4/8 MB, quad or octal,
  default 4 MB quad), Wi-Fi; no I2S. The CPU clock is capped at about 8 MHz by default (`cpuFrequency` overrides), so
  rendering speed is not representative.
- **P4 (beta)**: `ESP32-P4-Preview-DevKitC-1` virtual board with GPIO, UART, SPI, I2C, PSRAM and Wi-Fi via ESP-Hosted.
  The ESP32 guide does not mention display, DSI or touch for the P4, so the 1024x600 DSI + GT911 config is not covered.
- **Display and touch parts for a BOX-3-like config**: `wokwi-ili9341` (240x320, SPI) and a combined
  `board-ili9341-cap-touch` (ILI9341 over SPI plus FT6206 over I2C, used in public Wokwi projects). Only
  `wokwi-ili9341.md` exists in the docs repo, so the cap-touch part type and its pins (VCC GND CS RST D/C MOSI SCK LED
  MISO SCL SDA) are taken from a public example diagram and are unverified. No BOX-3 board part was found (the scenario
  docs use `esp32s3box` as a part id example, unverified). The real BOX-3 is ILI9342C 320x240 plus GT911, so the FT6206
  config only approximates it. No 800x480 RGB panel and no GT911.
- **Scenarios** (alpha): `wait-serial`, `write-serial`, `delay`, `set-control`, `expect-pin`, `take-screenshot`
  (`part-id`, `save-to`, `compare-with`), `touch`, `touch-press`, `touch-move`, `touch-release` (x, y, duration).
  Touch coordinates are in the controller's space; for ILI9341 + FT6206 the origin is bottom-right, so firmware maps
  them. CLI: `wokwi-cli --scenario`, `--timeout` (simulation ms), `--screenshot-part`, `--screenshot-time`,
  `--screenshot-file`, `--serial-log-file`, `--expect-text`; token in `WOKWI_CLI_TOKEN`.
- **Free tier**: 50 simulation minutes per rolling 30 days, 5 minutes wall-clock per CI session. That is a handful of
  runs a month, so use it as a nightly or manual job.
- **Draft**: `wokwi/diagram.json`, `wokwi/scenario.test.yaml`, `wokwi/wokwi.toml` (placeholder firmware path), not
  wired into CI. It needs firmware that drives an ILI9341 over SPI and an FT6206 over I2C (M3 work).

## CI

Job `qemu-s3` in `.github/workflows/ci.yml` runs when `docker/s3*`, `boards/s3-*/**`, `scripts/qemu*` or the workflow
change. It builds the image with the buildx GHA cache, restores `.cache/eb-s3` with `actions/cache` (IDF tools, cargo
registry, target dir), runs the script and uploads `screen.png`, `diff.png` and `serial.log`.
