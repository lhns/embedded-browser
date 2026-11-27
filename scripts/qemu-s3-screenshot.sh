#!/usr/bin/env bash
# Build the s3-qemu-test firmware in Docker, boot it in Espressif's QEMU (headless SDL), wait for
# READY on serial, take a monitor screendump and compare it with the committed golden PNG.
#
#   scripts/qemu-s3-screenshot.sh                 build, run, compare
#   scripts/qemu-s3-screenshot.sh --update-golden overwrite the golden with the new screenshot
#
# Env: EB_S3_IMAGE (default eb-s3:dev; built from docker/s3.Dockerfile if missing),
#      OUT_DIR (default out/qemu-s3), FUZZ (per-channel tolerance %, default 2),
#      MAX_DIFF_PX (default 0.1% of pixels), READY_TIMEOUT (s, default 60).
# Docker volume eb-s3-cache (target dir, ESP-IDF tools, cargo registry); EB_S3_CACHE can name
# another volume or a host dir (CI uses a dir restored by actions/cache).
set -euo pipefail

IMAGE=${EB_S3_IMAGE:-eb-s3:dev}
FW=boards/s3-qemu-test
GOLDEN=$FW/golden/s3-qemu-test.png

if [[ ${1:-} != --inner ]]; then
  ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && { pwd -W 2>/dev/null || pwd; })
  OUT_DIR=${OUT_DIR:-$ROOT/out/qemu-s3}
  mkdir -p "$OUT_DIR" && chmod 777 "$OUT_DIR"
  if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    docker build -f "$ROOT/docker/s3.Dockerfile" -t "$IMAGE" "$ROOT/docker"
  fi
  # MSYS_NO_PATHCONV stops Git Bash on Windows from rewriting the container paths.
  MSYS_NO_PATHCONV=1 docker run --rm \
    -v "$ROOT":/src:ro -v "$OUT_DIR":/out -v "${EB_S3_CACHE:-eb-s3-cache}":/cache \
    ${CI:+-e CI=1} "$IMAGE" bash /src/scripts/qemu-s3-screenshot.sh --inner "${1:-}"
  status=$?
  # The golden lives in the repo; --update-golden hands the new one back via /out.
  if [[ ${1:-} == --update-golden && -f $OUT_DIR/screen.png ]]; then
    mkdir -p "$ROOT/$(dirname $GOLDEN)" && cp "$OUT_DIR/screen.png" "$ROOT/$GOLDEN"
    echo "golden updated: $GOLDEN"
  fi
  [[ -f $OUT_DIR/Cargo.lock ]] && cp "$OUT_DIR/Cargo.lock" "$ROOT/$FW/Cargo.lock" || true
  exit $status
fi

# ---- inside the container ----
UPDATE=${2:-}
FUZZ=${FUZZ:-2}
READY_TIMEOUT=${READY_TIMEOUT:-60}
export CARGO_TARGET_DIR=/cache/target
export CARGO_HOME=/cache/cargo   # registry cache; the toolchain proxies stay on PATH from ~/.cargo/bin
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
export ESP_IDF_TOOLS_INSTALL_DIR=custom:/cache/embuild
export CARGO_WORKSPACE_DIR=/cache/proj
export XDG_CACHE_HOME=/cache/xdg

rm -rf /cache/proj && mkdir -p /cache/proj
cp -r /src/$FW/. /cache/proj/ && rm -rf /cache/proj/golden
cd /cache/proj

t_build=$(date +%s)
cargo build --release
echo "build: $(( $(date +%s) - t_build )) s"
cp Cargo.lock /out/Cargo.lock

ELF=$CARGO_TARGET_DIR/xtensa-esp32s3-espidf/release/s3-qemu-test
BOOT=$(find "$CARGO_TARGET_DIR/xtensa-esp32s3-espidf/release/build" -path '*bootloader/bootloader.bin' | head -1)
PART=$(find "$CARGO_TARGET_DIR/xtensa-esp32s3-espidf/release/build" -name partition-table.bin | head -1)
echo "bootloader=$BOOT partition_table=$PART"
# QEMU does not model the SAR ADC; neutralise the esp_adc boot constructor in the emulator image only.
TOOLS=$(dirname "$(ls /home/esp/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/xtensa-esp32s3-elf-nm)")
python3 /src/scripts/qemu-patch-adc.py "$TOOLS/xtensa-esp32s3-elf-nm" "$ELF" /out/s3-qemu-test.qemu.elf
espflash save-image --chip esp32s3 --merge --flash-size 8mb \
  --bootloader "$BOOT" --partition-table "$PART" /out/s3-qemu-test.qemu.elf /out/flash.bin

# Headless: -display none stalls the RGB panel driver, so use SDL with the dummy video driver.
export SDL_VIDEODRIVER=dummy
MON=/tmp/qemu-mon.sock
rm -f "$MON" /out/serial.log
qemu-system-xtensa -machine esp32s3 -m 8M \
  -global driver=ssi_psram,property=is_octal,value=true \
  -drive file=/out/flash.bin,if=mtd,format=raw \
  -display sdl -serial file:/out/serial.log \
  -monitor unix:$MON,server,nowait &
QPID=$!
trap 'kill $QPID 2>/dev/null || true' EXIT

t_boot=$(date +%s.%N)
for ((i = 0; i < READY_TIMEOUT * 10; i++)); do
  grep -q '^READY' /out/serial.log 2>/dev/null && break
  kill -0 $QPID 2>/dev/null || { echo "QEMU exited early"; cat /out/serial.log; exit 1; }
  sleep 0.1
done
grep -q '^READY' /out/serial.log || { echo "timeout waiting for READY"; cat /out/serial.log; exit 1; }
echo "READY after $(awk "BEGIN{printf \"%.1f\", $(date +%s.%N) - $t_boot}") s wall-clock"
sleep 1 # let the display refresh run once more

python3 - "$MON" /out/screen.ppm <<'PY'
import socket, sys, time
s = socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); s.settimeout(5)
time.sleep(0.3)
s.sendall(f"screendump {sys.argv[2]}\n".encode()); time.sleep(1)
s.sendall(b"quit\n")
PY
convert /out/screen.ppm /out/screen.png
cat /out/serial.log | tr -d '\r' | grep -E 'PSRAM|app_main|READY'

if [[ $UPDATE == --update-golden ]]; then exit 0; fi
[[ -f /src/$GOLDEN ]] || { echo "no golden at $GOLDEN (run with --update-golden)"; exit 1; }
total=$(identify -format '%[fx:w*h]' /out/screen.png)
max=${MAX_DIFF_PX:-$(( total / 1000 ))}
set +e
diff_px=$(compare -metric AE -fuzz "${FUZZ}%" /out/screen.png /src/$GOLDEN /out/diff.png 2>&1)
set -e
echo "differing pixels: $diff_px / $total (max $max)"
[[ ${diff_px%% *} -le $max ]] && echo "screenshot matches golden" || { echo "screenshot differs from golden"; exit 1; }
