# 0002. Blitz as the engine stack

Status: Accepted (owner decision after M0, 2026-11-27)
Date: 2026-11-27

## Context
Blitz (DioxusLabs, MIT/Apache, 0.3.0-beta.6) is Servo stripped down: html5ever, Stylo, Taffy and Parley, with CPU painting through anyrender. Nobody had run it on an MCU, so M0 measured it (docs/m0/):
- **Build:** it builds for the P4 (`riscv32imafc-esp-espidf`) and compiles for the S3 without source changes to Stylo or Blitz.
- **Size:** the P4 image is 6.3 MiB with the host-measured features. Estimated with Wi-Fi, TLS and HTTP: ~6.9 MiB.
- **Heap:** peak at 32-bit is 1.1–3.6 MB on synthetic pages and 10.5 MB on Wikipedia with images.
- **Raster:** vello_cpu beats tiny-skia on both time and memory. The P4 full-frame time is provisional at 0.2–2.5 s and is settled at gate H1.
- **Damage:** Blitz exposes restyle damage (`Node::damage()`, read between `resolve_stylist()` and `resolve()`). Layout shifts still need an old/new bounds diff.

## Decision
- Use Blitz pinned at `=0.3.0-beta.6` with `default-features = false`, without blitz-net or blitz-shell, behind a private `engine` module. The raster backend is vello_cpu.
- Build shims, kept in this repo:
  - a target spec per chip with `max-atomic-width: 64` (blitz-dom and anyrender use `AtomicU64`; ESP-IDF supplies the `__atomic_*_8` libcalls);
  - a `memmap2` stand-in via `[patch.crates-io]`, because fontique's std feature maps files;
  - a pinned nightly (2026-10-19).
- Source changes, if ever needed, go into **our own forks** (git fork + `[patch.crates-io]` at a fixed revision). We do not file issues or PRs upstream.
- The fallback engine (our own cascade) is dropped for now: it removes neither shim, and every metric came in under the go limits.

## Consequences
- tiny-skia and the full RGBA frame buffer are ruled out. Banded rendering with damage-limited, x-clipped re-raster is the only paint path.
- Wikipedia-class pages leave ~1 MB of heap margin on the P4. The S3 8 MB tier is limited to dashboard-class pages.
- Beta churn is contained by the exact pin and the private module. Nightly churn is contained by the pinned date.
