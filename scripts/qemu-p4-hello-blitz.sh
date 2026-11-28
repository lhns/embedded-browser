#!/usr/bin/env bash
# Boot the P4 hello-Blitz firmware (boards/p4-function-ev, CHIP=esp32p4) in Espressif's QEMU
# (qemu-system-riscv32 -machine esp32p4), serial only, and print the log. Needs the build output
# of scripts/build-firmware.sh (firmware.elf and bootloader.bin) for the emulator variant:
#   FEATURES=qemu ESP_IDF_SDKCONFIG_DEFAULTS="sdkconfig.defaults;sdkconfig.defaults.qemu"
# (QEMU fails the PSRAM chip probe; see docs/m0/emulation-results.md). Two ELF patches apply to
# the emulator image only: the ADC constructor and the lazy-FPU trap (qemu-patch-p4-fpu.py).
#
#   scripts/qemu-p4-hello-blitz.sh [build-out-dir]   default out/p4-function-ev-esp32p4
#
# Env: EB_S3_IMAGE (the emulator image, default eb-s3:dev; it carries both QEMU builds),
#      OUT_DIR (default out/qemu-p4-hello), TIMEOUT (s, default 300),
#      ICOUNT (e.g. 0: run with -icount shift=0, so guest time counts instructions; 1 insn = 1 ns).
# Exit 0 only if the firmware printed "eb-hello: done".
set -euo pipefail

IMAGE=${EB_S3_IMAGE:-eb-s3:dev}

if [[ ${1:-} != --inner ]]; then
  ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && { pwd -W 2>/dev/null || pwd; })
  IN=${1:-$ROOT/out/p4-function-ev-esp32p4}
  OUT_DIR=${OUT_DIR:-$ROOT/out/qemu-p4-hello}
  mkdir -p "$OUT_DIR" && chmod 777 "$OUT_DIR"
  if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    docker build -f "$ROOT/docker/s3.Dockerfile" -t "$IMAGE" "$ROOT/docker"
  fi
  MSYS_NO_PATHCONV=1 docker run --rm -v "$ROOT":/src:ro -v "$IN":/in:ro -v "$OUT_DIR":/out \
    -e TIMEOUT="${TIMEOUT:-300}" -e ICOUNT="${ICOUNT:-}" "$IMAGE" \
    bash /src/scripts/qemu-p4-hello-blitz.sh --inner
  exit $?
fi

# ---- inside the container ----
# Any nm reads the symbol table; the RISC-V ELF needs no target-specific one.
NM=$(ls /home/esp/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32s3-elf-nm)
python3 /src/scripts/qemu-patch-adc.py "$NM" /in/firmware.elf /out/firmware.qemu.elf
python3 /src/scripts/qemu-patch-p4-fpu.py "$NM" /out/firmware.qemu.elf /out/firmware.qemu.elf
espflash save-image --chip esp32p4 --merge --flash-size 16mb \
  --bootloader /in/bootloader.bin --partition-table /src/boards/p4-function-ev/partitions.csv \
  /out/firmware.qemu.elf /out/flash.bin

rm -f /out/serial.log
qemu-system-riscv32 -machine esp32p4 ${ICOUNT:+-icount shift=$ICOUNT} \
  -drive file=/out/flash.bin,if=mtd,format=raw \
  -nographic -serial file:/out/serial.log -monitor none &
QPID=$!
trap 'kill $QPID 2>/dev/null || true' EXIT
status=1
for ((i = 0; i < TIMEOUT * 10; i++)); do
  if grep -q '^eb-hello: done' /out/serial.log 2>/dev/null; then status=0; break; fi
  # A panic or abort reboots into a loop; stop at the first one.
  grep -qE 'Guru Meditation|abort\(\) was called' /out/serial.log 2>/dev/null && { sleep 3; break; }
  kill -0 $QPID 2>/dev/null || break
  sleep 0.1
done
tr -d '\r' < /out/serial.log | grep -E '^(eb-hello|Resolve|Render|Guru|Backtrace|MEPC|MTVAL|PC )|panic|abort|Rebooting|psram' || true
exit $status
