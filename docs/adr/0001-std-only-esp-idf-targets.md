# 0001. Engine targets ESP-IDF std only

Status: Accepted
Date: 2026-11-27

## Context
Full Servo cannot fit an MCU: SpiderMonkey, GPU-only WebRender and a process model. The reusable parts (html5ever, cssparser, selectors, Stylo) are all std-only. ESP-IDF provides std on the ESP32-P4 and S3.

## Decision
The engine targets ESP-IDF std (ESP32-P4 first). no_std MCUs are a non-goal for v1, as they would need a different engine. The core crate carries no ESP dependencies, so other std targets stay possible.

## Consequences
- Stylo, html5ever and friends are usable as is.
- Boards without PSRAM or std support are out of scope.
- ESP specifics live in a separate adapter crate and per-board firmware.
