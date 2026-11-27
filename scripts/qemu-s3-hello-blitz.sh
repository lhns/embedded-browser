#!/usr/bin/env bash
# Boot the S3 hello-Blitz firmware (boards/p4-function-ev built for esp32s3) in Espressif's QEMU,
# serial only, and print the log. Needs the build output of scripts/build-firmware.sh with
# CHIP=esp32s3 (firmware.elf and bootloader.bin), see docs/m0/esp-build-results.md.
#
#   scripts/qemu-s3-hello-blitz.sh [build-out-dir]   default out/p4-function-ev-esp32s3
#
# Env: EB_S3_IMAGE (default eb-s3:dev), OUT_DIR (default out/qemu-s3-hello), TIMEOUT (s, default 300).
# Exit 0 only if the firmware printed "eb-hello: done".
# QEMU does not model S32C1I-on-PSRAM, so "atomics OK" here says nothing about hardware.
set -euo pipefail

IMAGE=${EB_S3_IMAGE:-eb-s3:dev}

if [[ ${1:-} != --inner ]]; then
  ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && { pwd -W 2>/dev/null || pwd; })
  IN=${1:-$ROOT/out/p4-function-ev-esp32s3}
  OUT_DIR=${OUT_DIR:-$ROOT/out/qemu-s3-hello}
  mkdir -p "$OUT_DIR" && chmod 777 "$OUT_DIR"
  if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    docker build -f "$ROOT/docker/s3.Dockerfile" -t "$IMAGE" "$ROOT/docker"
  fi
  MSYS_NO_PATHCONV=1 docker run --rm -v "$ROOT":/src:ro -v "$IN":/in:ro -v "$OUT_DIR":/out \
    -e TIMEOUT="${TIMEOUT:-300}" "$IMAGE" bash /src/scripts/qemu-s3-hello-blitz.sh --inner
  exit $?
fi

# ---- inside the container ----
NM=$(ls /home/esp/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32s3-elf-nm)
# QEMU has no SAR ADC model: neutralise the esp_adc boot constructor in the emulator image only.
python3 /src/scripts/qemu-patch-adc.py "$NM" /in/firmware.elf /out/firmware.qemu.elf
espflash save-image --chip esp32s3 --merge --flash-size 16mb \
  --bootloader /in/bootloader.bin --partition-table /src/boards/p4-function-ev/partitions.csv \
  /out/firmware.qemu.elf /out/flash.bin

rm -f /out/serial.log
qemu-system-xtensa -machine esp32s3 -m 8M \
  -global driver=ssi_psram,property=is_octal,value=true \
  -drive file=/out/flash.bin,if=mtd,format=raw \
  -nographic -serial file:/out/serial.log -monitor none &
QPID=$!
trap 'kill $QPID 2>/dev/null || true' EXIT
status=1
for ((i = 0; i < TIMEOUT * 10; i++)); do
  if grep -q '^eb-hello: done' /out/serial.log 2>/dev/null; then status=0; break; fi
  # A panic reboots into a loop; stop at the first Guru Meditation.
  grep -q 'Guru Meditation' /out/serial.log 2>/dev/null && { sleep 3; break; }
  kill -0 $QPID 2>/dev/null || break
  sleep 0.1
done
tr -d '\r' < /out/serial.log | grep -E '^(eb-hello|Resolve|Guru|Backtrace|EXCVADDR|PC )|panic|Rebooting' || true
exit $status
