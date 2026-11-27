# M0 emulation check (node `ci-emulation-check`)

Date: 2026-11-27. Versions: Espressif QEMU `esp-develop-9.2.2-20261109`, `espressif/idf-rust:esp32s3_1.95.0.0`
(esp toolchain 1.95 nightly), ESP-IDF v5.5.5 (esp-idf-sys 0.39.0 default), esp-idf-svc 0.54.2,
`espressif/esp_lcd_qemu_rgb` 1.0.2.

## Verdict

| | CI can emulate | Cannot |
|---|---|---|
| ESP32-S3 | Boot, octal PSRAM (8 MB), serial, RGB framebuffer screenshot (`screendump`) compared with a golden, Ethernet (open_eth). Good for: the rendering pipeline on Xtensa, PSRAM allocation, golden screenshots. | I2C (so no touch controller), GP SPI, I2S, LEDC, Wi-Fi, SAR ADC (see quirk 2), real timing. The panel is a QEMU-only virtual RGB device, not the BOX-3 SPI ILI9341. |
| ESP32-P4 | Boots a plain IDF v5.5.5 C hello world in the pinned QEMU (`-machine esp32p4`, ROM `esp32p4-eco5`, chip rev v3.1, dual core 400 MHz). Good for "firmware boots and prints" smoke tests. | Not tried or likely absent: PSRAM, MIPI-DSI, GT911/I2C, the Rust `riscv32imafc` build. No display device is exposed in `-device help`. No screenshots. |

So the S3 gets a golden-screenshot job; the P4 gets at most a serial smoke test (once the P4 Rust firmware exists).
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

- **Backtrace** (addr2line): `vello_cpu::fine::lowp::U8Kernel::alpha_composite_buffer` <- `Fine::indexed_fill` <- `paint_fill` <- `bucket_and_rasterize` <- `RenderContext::render_with` <- `run`. No "paint" phase line and no checksum line were printed; the firmware then reboots in a loop. The cause is undetermined: it could be a QEMU issue or a real Xtensa problem (stack, alignment, the pixel buffer). I stopped here as agreed.
- **Phase times are emulator wall-clock** and varied about 2x between two runs (style+layout 3.2 s and 1.9 s), so they say nothing about the device.
- **Heap:** internal RAM was at 446 of 457 KB after style+layout; PSRAM held only 650 KB. With `SPIRAM_MALLOC_ALWAYSINTERNAL=4096`, Stylo's many small allocations fill internal RAM, which leaves no room for Wi-Fi and DMA buffers on a real S3. This holds whatever caused the crash; lowering the threshold is a thing to try on hardware.
- **Atomics check:** `atomics OK` at `0x3c6109a0`. That address is past the end of the flash-mapped rodata segment (`3c4d0020` + 1,275,744 bytes), so the block is in PSRAM. **QEMU does not model the S32C1I-on-PSRAM hardware limitation, so the pass proves nothing about real S3 hardware** (see the S32C1I section in `docs/m0/esp-build-results.md`). It does show the software path (2 threads, `Arc`, 64-bit libcall atomics) runs.
- Threads: `tasks` was 6 from engine start through style+layout.
- **Script status:** `scripts/qemu-s3-hello-blitz.sh [build-out-dir]` runs the flow and exits 1 at the paint crash. It exits 0 once the firmware prints `eb-hello: done`.

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
