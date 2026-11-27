#!/usr/bin/env bash
# Build a board firmware inside its Docker image, write the app image and size reports.
# Runs inside the container (see docs/m0/esp-build-results.md for the docker run commands).
#
# Env: BOARD (default p4-function-ev), CHIP (esp32p4),
#      TARGET (default targets/riscv32imafc-esp-espidf.json, relative to the board dir),
#      TOOLCHAIN (empty = rust-toolchain.toml; "+esp" for the espup Xtensa toolchain),
#      FEATURES (cargo features, e.g. measured-set; default none),
#      CARGO_TARGET_DIR (default /tmp/eb-target), OUT_DIR (default <repo>/out).
# The source is copied to the container's Linux filesystem first: ESP-IDF's CMake build breaks on
# Windows bind-mount paths and is slow on them.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
board=${BOARD:-p4-function-ev}
chip=${CHIP:-esp32p4}
target=${TARGET:-targets/riscv32imafc-esp-espidf.json}
triple=$(basename "$target" .json)
toolchain=${TOOLCHAIN:-}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/tmp/eb-target}
export MCU=$chip
out=${OUT_DIR:-$repo/out}/$board-$chip
work=/tmp/eb-src

# Copy all of boards/: the board crates patch dependencies from boards/patches.
mkdir -p "$work" "$out"
cp -a "$repo/boards" "$work/"
cd "$work/boards/$board"

# shellcheck disable=SC2086 # $toolchain is empty or one word
cargo $toolchain build --release --locked --target "$target" ${FEATURES:+--features "$FEATURES"}
bin=$(cargo $toolchain metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["name"])')
elf=$CARGO_TARGET_DIR/$triple/release/$bin
cp "$elf" "$out/firmware.elf"
# The bootloader ESP-IDF built from our sdkconfig (chip revision, flash size); espflash would
# otherwise flash its bundled one.
cp "$CARGO_TARGET_DIR/$triple/release/bootloader.bin" "$out/bootloader.bin"

espflash save-image --chip "$chip" --flash-size 16mb --partition-table partitions.csv \
  "$elf" "$out/app.bin" | tee "$out/save-image.txt"

case $chip in
  esp32p4) tools=riscv32-esp-elf ;;
  *) tools=xtensa-$chip-elf ;;
esac
"$tools-size" -A "$elf" > "$out/sections.txt"
"$tools-nm" -S --size-sort -C "$elf" > "$out/nm.txt"
python3 "$repo/scripts/size-attribution.py" 30 < "$out/nm.txt" > "$out/attribution.txt"

echo "== $board ($chip): app image $(stat -c %s "$out/app.bin") bytes"
grep -E '^\.(flash\.text|flash\.rodata|iram0\.text|dram0\.data)|^Total' "$out/sections.txt" || true
sed -n '1,16p' "$out/attribution.txt"
