# ESP32-S3 firmware build + Espressif QEMU fork (ADR 0006).
# The idf-rust image has the esp Rust toolchain, espflash and ldproxy; ESP-IDF itself is
# fetched by esp-idf-sys/embuild into the build volume on first build.
FROM espressif/idf-rust:esp32s3_1.95.0.0

ARG QEMU_TAG=esp-develop-9.2.2-20261109
ARG QEMU_VER=esp_develop_9.2.2_20261109

USER root
# Runtime libs for QEMU (SDL2 for the dummy-driver display, slirp, gcrypt) and for embuild
# (python venv, cmake prerequisites, libusb for ESP-IDF tools).
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils git pkg-config libssl-dev \
      python3-venv python3-pip python3-dev libffi-dev \
      libsdl2-2.0-0 libslirp0 libpixman-1-0 libglib2.0-0 libgcrypt20 libnuma1 libusb-1.0-0 \
      imagemagick \
    && rm -rf /var/lib/apt/lists/*

# Pinned QEMU release; the tarball ships qemu-system-xtensa plus its ESP ROM images.
RUN curl -fsSL -o /tmp/qemu.tar.xz \
      "https://github.com/espressif/qemu/releases/download/${QEMU_TAG}/qemu-xtensa-softmmu-${QEMU_VER}-x86_64-linux-gnu.tar.xz" \
    && mkdir -p /opt/qemu && tar -xJf /tmp/qemu.tar.xz -C /opt/qemu --strip-components=1 \
    && rm /tmp/qemu.tar.xz
ENV PATH="/opt/qemu/bin:${PATH}"
# The RISC-V build of the same release has the esp32p4 machine (scripts/qemu-p4-hello-blitz.sh).
# It needs the same runtime libs, so the P4 serial smoke test runs in this image too.
RUN curl -fsSL -o /tmp/qemu.tar.xz \
      "https://github.com/espressif/qemu/releases/download/${QEMU_TAG}/qemu-riscv32-softmmu-${QEMU_VER}-x86_64-linux-gnu.tar.xz" \
    && mkdir -p /opt/qemu-riscv32 && tar -xJf /tmp/qemu.tar.xz -C /opt/qemu-riscv32 --strip-components=1 \
    && rm /tmp/qemu.tar.xz \
    && ln -s /opt/qemu-riscv32/bin/qemu-system-riscv32 /usr/local/bin/qemu-system-riscv32

# /cache holds build state (target dir, embuild tools, cargo registry) as named volumes so that
# nothing is built on a bind mount (slow on Windows, path issues).
RUN mkdir -p /cache /out && chown esp:esp /cache /out

USER esp
WORKDIR /work
