#!/bin/sh
# 32-bit measurements for the JS host spike, in Docker (see docs/m0/js-spike-results.md).
# Usage, from the repo root:
#   docker run --rm -v "$PWD":/src -v js-spike-target:/target -w /src rust:1.95-bookworm \
#     sh tools/js-spike/docker-i686.sh
# SKIP_CORPUS=1 runs only the size measurements.
set -eu
export CARGO_TARGET_DIR=/target RUSTUP_TOOLCHAIN=1.95
apt-get update -qq >/dev/null
apt-get install -y -qq gcc-multilib gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf >/dev/null
rustup target add i686-unknown-linux-gnu >/dev/null 2>&1
T=i686-unknown-linux-gnu

if [ -z "${SKIP_CORPUS:-}" ]; then
  echo "== corpus run, $T release"
  cargo build -q --release --locked -p js-spike --target $T
  /target/$T/release/js-spike --root /src --out /src/target/js-spike-i686.md "$@"
fi

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
# picolibc has no pthread.h (ESP-IDF does); declarations are enough to compile with -c.
mkdir -p "$O/inc"
cat > "$O/inc/pthread.h" <<'H'
#include <time.h>
typedef int pthread_once_t, pthread_mutex_t, pthread_cond_t, pthread_condattr_t, pthread_attr_t;
typedef unsigned long pthread_t;
#define PTHREAD_ONCE_INIT 0
#define PTHREAD_CREATE_DETACHED 1
#ifndef CLOCK_MONOTONIC
#define CLOCK_MONOTONIC 4
int clock_gettime(int, struct timespec *);
#endif
int pthread_once(pthread_once_t *, void (*)(void));
int pthread_mutex_init(pthread_mutex_t *, const void *);
int pthread_mutex_destroy(pthread_mutex_t *);
int pthread_mutex_lock(pthread_mutex_t *);
int pthread_mutex_unlock(pthread_mutex_t *);
int pthread_cond_init(pthread_cond_t *, const pthread_condattr_t *);
int pthread_cond_destroy(pthread_cond_t *);
int pthread_condattr_init(pthread_condattr_t *);
int pthread_condattr_destroy(pthread_condattr_t *);
int pthread_condattr_setclock(pthread_condattr_t *, int);
int pthread_cond_signal(pthread_cond_t *);
int pthread_cond_broadcast(pthread_cond_t *);
int pthread_cond_wait(pthread_cond_t *, pthread_mutex_t *);
int pthread_cond_timedwait(pthread_cond_t *, pthread_mutex_t *, const struct timespec *);
int pthread_create(pthread_t *, const pthread_attr_t *, void *(*)(void *), void *);
int pthread_join(pthread_t, void **);
int pthread_detach(pthread_t);
int pthread_attr_init(pthread_attr_t *);
int pthread_attr_destroy(pthread_attr_t *);
int pthread_attr_setstacksize(pthread_attr_t *, unsigned);
int pthread_attr_setdetachstate(pthread_attr_t *, int);
H
for f in quickjs libregexp libunicode dtoa; do
  riscv64-unknown-elf-gcc -march=rv32imafc -mabi=ilp32f -Os -ffunction-sections -fdata-sections \
    --specs=picolibc.specs -D_GNU_SOURCE -I"$O/inc" -c "$Q/$f.c" -o "$O/$f.o"
done
riscv64-unknown-elf-size -t "$O"/*.o
