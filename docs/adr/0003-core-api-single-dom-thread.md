# 0003. Core API on a single DOM thread

Status: Proposed (becomes Accepted in M1; M0 found Blitz exposes restyle damage, see ADR 0002)
Date: 2026-11-27

## Context
The core must run on a device with one engine thread and no blocking I/O in the render path, and leave room for a JS engine later without an API break. The trait shapes may change after M0 results, so they are written in M1.

## Decision
- `Browser<N: Net, S: BlobStore = NoStore>` with `load`, `touch`, `tick` and `render`.
- `Net` is non-blocking: `fetch` returns a request id, completions are drained in `tick()`. The same trait serves subresources and a later JS fetch or WebSocket.
- Generational `NodeHandle`. An `EventSink` runs first, then default actions (links, forms, checkboxes, text edit).
- A `ScriptHost::on_script` hook, with no implementation in v1.
- Optional `BlobStore` for cache, history and cookies; `Limits` caps that give placeholders instead of OOM aborts.
- Banded rendering with re-raster limited to damaged bands, and a PSRAM strip cache for scrolling, rasterized time-sliced in `tick()`.

## Consequences
- JS (v2) plugs in through the sink and script hook without breaking the API.
- No locking in the engine; slow rasterization is bounded per tick.
- Damage comes from Blitz restyle damage plus an old/new layout-bounds diff (M0 finding).
