# 0007. Xtensa: fearless_simd fork and software atomics

Status: Accepted
Date: 2026-11-28

## Context
- Espressif's LLVM miscompiles functions that realign the stack: stack-passed arguments are read from the moved stack pointer (espressif/llvm-project#140, open). fearless_simd 1.3.0's `align(32)` and `align(64)` vector types trigger it in vello_cpu on Xtensa, and the S3 crashed in the first paint (docs/m0/emulation-results.md). The fix is a source change, so ADR 0002's rule applies: our own fork, no upstream issues or PRs.
- The S3's inline S32C1I atomics do not work on PSRAM. With the malloc threshold at 256 bytes, `Arc`s over that size now live in PSRAM, so the stock S3 target is expected to fail on hardware (docs/m0/esp-build-results.md).

## Decision
- **Fork:** `lhns/fearless_simd`, branch `xtensa-align16`, patch commit `f7044ee879ad754aa93cac9fa90b3b3b99e49279` on base `2ac6acdd13efb7689a6e80049a4933516a57ec20` (the release commit of 1.3.0, taken from the crate's `.cargo_vcs_info.json`). It caps the vector alignment at 16 on `target_arch = "xtensa"`, in the generator and in the generated code. `boards/p4-function-ev/Cargo.toml` pins it with `[patch.crates-io]` at that revision; the host workspace stays on crates.io. Nothing goes upstream.
- **Software atomics are the default S3 target:** `targets/xtensa-esp32s3-espidf.json` has `-s32c1i`, so every atomic is an `__atomic_*` libcall, which ESP-IDF routes to a lock for PSRAM. The stock-atomics spec stays as an opt-in in `targets/stock-atomics/`.
- CI builds the S3 hello-Blitz with this spec, runs `scripts/scan-xtensa-realign.py` (0 realigning functions required) and boots it in QEMU to `eb-hello: done`.

## Consequences
- The S3 paints in QEMU with the host's checksum. Realigning code from any other dependency is caught by the scan.
- We carry one more fork. Drop it, and the `-s32c1i` default if hardware allows, once a fixed esp toolchain ships (check the rust-build releases) and the scan passes with stock fearless_simd.
- Atomics cost a libcall on the S3; H2 measures it on hardware.
