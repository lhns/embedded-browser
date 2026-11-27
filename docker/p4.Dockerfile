# ESP32-P4 Rust firmware build image (ADR 0006).
# There is no official idf-rust image for the P4, so this one is built here:
# - ESP-IDF is preinstalled (not fetched by esp-idf-sys at build time), so CI caches it as an image
#   layer and firmware builds work offline. esp-idf-sys picks it up via ESP_IDF_TOOLS_INSTALL_DIR=fromenv.
# - The riscv32imafc-esp-espidf target is tier 3: a pinned nightly with rust-src builds std (build-std).
FROM ubuntu:24.04

ARG ESP_IDF_VERSION=v5.5.5
ARG RUST_NIGHTLY=nightly-2026-10-19
ARG LDPROXY_VERSION=0.3.5
ARG ESPFLASH_VERSION=v4.7.0

ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8
RUN apt-get update && apt-get install -y --no-install-recommends \
      git wget curl ca-certificates unzip flex bison gperf python3 python3-pip python3-venv \
      cmake ninja-build ccache libffi-dev libssl-dev dfu-util libusb-1.0-0 \
      build-essential pkg-config libclang-dev clang \
    && rm -rf /var/lib/apt/lists/*

# ESP-IDF and its P4 toolchain (riscv32-esp-elf gcc, binutils), installed into /opt/esp.
ENV IDF_PATH=/opt/esp/idf IDF_TOOLS_PATH=/opt/esp/tools
RUN git clone --depth 1 --branch ${ESP_IDF_VERSION} --recurse-submodules --shallow-submodules \
      https://github.com/espressif/esp-idf.git ${IDF_PATH} \
    && ${IDF_PATH}/install.sh esp32p4 \
    && rm -rf ${IDF_TOOLS_PATH}/dist

# Rust: pinned nightly with rust-src for build-std; ldproxy as the linker wrapper; espflash for images.
ENV RUSTUP_HOME=/opt/rust/rustup CARGO_HOME=/opt/rust/cargo PATH=/opt/rust/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${RUST_NIGHTLY} \
      --component rust-src,rustfmt \
    && cargo install ldproxy --version ${LDPROXY_VERSION} --locked \
    && curl -sSfL -o /tmp/espflash.zip \
      https://github.com/esp-rs/espflash/releases/download/${ESPFLASH_VERSION}/espflash-x86_64-unknown-linux-gnu.zip \
    && unzip /tmp/espflash.zip -d ${CARGO_HOME}/bin && rm /tmp/espflash.zip \
    && rm -rf ${CARGO_HOME}/registry ${CARGO_HOME}/git \
    && chmod -R a+rwX /opt/rust

# esp-idf-sys uses the activated ESP-IDF environment instead of installing its own.
ENV ESP_IDF_TOOLS_INSTALL_DIR=fromenv ESP_IDF_VERSION=${ESP_IDF_VERSION} MCU=esp32p4 \
    LIBCLANG_PATH=/usr/lib/llvm-18/lib
COPY docker/p4-entrypoint.sh /usr/local/bin/p4-entrypoint.sh
ENTRYPOINT ["/usr/local/bin/p4-entrypoint.sh"]
CMD ["bash"]
