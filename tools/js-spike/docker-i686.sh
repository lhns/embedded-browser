#!/bin/sh
# 32-bit measurements for the JS host spike, in Docker (see docs/m0/js-spike-results.md).
# Usage, from the repo root:
#   docker run --rm -v "$PWD":/src -v js-spike-target:/target -w /src rust:1.95-bookworm \
#     sh tools/js-spike/docker-i686.sh
set -eu
export CARGO_TARGET_DIR=/target RUSTUP_TOOLCHAIN=1.95
apt-get update -qq >/dev/null
apt-get install -y -qq gcc-multilib gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf >/dev/null
rustup target add i686-unknown-linux-gnu >/dev/null 2>&1
T=i686-unknown-linux-gnu

echo "== corpus run, $T release"
cargo build -q --release --locked -p js-spike --target $T
/target/$T/release/js-spike --root /src --out /src/target/js-spike-i686.md "$@"

echo "== QuickJS-ng code size, $T, stripped binaries"
for opt in 3 s; do
  CARGO_PROFILE_RELEASE_OPT_LEVEL=$opt CARGO_PROFILE_RELEASE_STRIP=true CARGO_PROFILE_RELEASE_LTO=true \
    cargo build -q --release --locked -p js-spike --target $T --examples
  with=$(stat -c %s /target/$T/release/examples/size_with_qjs)
  without=$(stat -c %s /target/$T/release/examples/size_without_qjs)
  echo "opt-level=$opt: with $with B, without $without B, QuickJS-ng + rquickjs $((with - without)) B"
done

echo "== QuickJS-ng C objects for rv32imafc (ESP32-P4 ISA), riscv64-unknown-elf-gcc -Os, text+data"
Q=$(ls -d "$CARGO_HOME"/registry/src/*/rquickjs-sys-0.14.0/quickjs)
O=$(mktemp -d)
for f in quickjs libregexp libunicode dtoa; do
  riscv64-unknown-elf-gcc -march=rv32imafc -mabi=ilp32f -Os -ffunction-sections -fdata-sections \
    --specs=picolibc.specs -D_GNU_SOURCE -c "$Q/$f.c" -o "$O/$f.o"
done
riscv64-unknown-elf-size -t "$O"/*.o
