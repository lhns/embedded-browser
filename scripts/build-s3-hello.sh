#!/usr/bin/env bash
# Build the S3 hello-Blitz firmware (boards/p4-function-ev for esp32s3, software-atomics target) in the
# eb-s3 image and check it for stack-realigning functions (scripts/scan-xtensa-realign.py).
# Output for scripts/qemu-s3-hello-blitz.sh: out/p4-function-ev-esp32s3/{firmware.elf,bootloader.bin,...}.
#
#   scripts/build-s3-hello.sh
#
# Env: EB_S3_IMAGE (default eb-s3:dev; built from docker/s3.Dockerfile if missing),
#      EB_S3_CACHE (docker volume or host dir for target dir, ESP-IDF tools and cargo registry;
#      default volume eb-s3-cache; CI uses a dir restored by actions/cache), OUT_DIR (default out).
set -euo pipefail

IMAGE=${EB_S3_IMAGE:-eb-s3:dev}

if [[ ${1:-} != --inner ]]; then
  ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && { pwd -W 2>/dev/null || pwd; })
  OUT_DIR=${OUT_DIR:-$ROOT/out}
  mkdir -p "$OUT_DIR" && chmod 777 "$OUT_DIR"
  if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    docker build -f "$ROOT/docker/s3.Dockerfile" -t "$IMAGE" "$ROOT/docker"
  fi
  MSYS_NO_PATHCONV=1 docker run --rm \
    -v "$ROOT":/src:ro -v "$OUT_DIR":/out -v "${EB_S3_CACHE:-eb-s3-cache}":/cache \
    "$IMAGE" bash /src/scripts/build-s3-hello.sh --inner
  exit $?
fi

# ---- inside the container ----
export CARGO_TARGET_DIR=/cache/hello-target
export CARGO_HOME=/cache/cargo   # the toolchain proxies stay on PATH from ~/.cargo/bin
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
export ESP_IDF_TOOLS_INSTALL_DIR=custom:/cache/embuild
export XDG_CACHE_HOME=/cache/xdg
git config --global --add safe.directory '*'
[[ -f /home/esp/export-esp.sh ]] && . /home/esp/export-esp.sh

TOOLCHAIN=+esp CHIP=esp32s3 TARGET=targets/xtensa-esp32s3-espidf.json OUT_DIR=/out \
  bash /src/scripts/build-firmware.sh

OBJDUMP=$(ls /home/esp/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32s3-elf-objdump)
"$OBJDUMP" -d --no-show-raw-insn /out/p4-function-ev-esp32s3/firmware.elf | python3 /src/scripts/scan-xtensa-realign.py
