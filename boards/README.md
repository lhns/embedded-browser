# Boards

Firmware for each board lives in its own directory here (`p4-function-ev`, `s3-box3`, `s3-7in`).
Each board has its own target, toolchain (pinned nightly, ESP-IDF) and `sdkconfig`, so these crates are
excluded from the host workspace (`exclude = ["boards/*"]`) and built in Docker (see ADR 0006).
