#!/usr/bin/env bash
# Activate the preinstalled ESP-IDF (PATH to riscv32-esp-elf tools, IDF python env), then run the command.
set -e
. "$IDF_PATH/export.sh" >/dev/null 2>&1
exec "$@"
