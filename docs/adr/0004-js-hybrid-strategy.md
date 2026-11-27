# 0004. Hybrid JavaScript strategy

Status: Proposed
Date: 2026-11-27

## Context
Home Assistant and ESPHome v2 need shadow DOM and custom elements, which Blitz lacks. Small scripts (custom dashboard, Tasmota, ESPHome v1, htmx) are within reach: QuickJS-ng is an official ESP-IDF component. v1 ships without JS.

## Decision
JS is a parallel track after M0. QuickJS-ng on the device for small scripts (J1, J2), plus a VNC-style remote mode (J3: client plus a companion Chromium container) for shadow-DOM single-page apps such as Home Assistant. Auto mode per origin (J4) comes after. Shadow DOM in Blitz is an upstream contribution, tracked and not planned. This stays Proposed until the JS host spike and H1 numbers exist.

## Consequences
- Home Assistant works only in remote mode, which bypasses this renderer.
- The on-device JS work does not block v1.
- J1-J4 are parked in the roadmap until M0.
