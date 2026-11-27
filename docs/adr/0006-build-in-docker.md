# 0006. Build firmware in Docker

Status: Accepted
Date: 2026-11-27

## Context
ESP-IDF Rust builds need a pinned nightly, rust-src, ldproxy and a specific IDF. On Windows, long paths and toolchain setup are fragile. The P4 has no official Rust image; the S3 has `espressif/idf-rust`.

## Decision
Firmware builds run in Docker: a custom P4 image (nightly, rust-src, ldproxy, python) and an S3 image based on `espressif/idf-rust` with QEMU 9.2.2. Local and CI builds use the same images. Host crates build natively.

## Consequences
- Reproducible builds, identical locally and in CI.
- Image build time and cache management in CI.
- Flashing and hardware gates stay manual, run by the user outside Docker.
