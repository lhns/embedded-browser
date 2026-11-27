# 0002. Blitz as the engine stack

Status: Proposed (becomes Accepted or superseded after M0)
Date: 2026-11-27

## Context
Blitz (DioxusLabs, MIT/Apache, 0.3.0-beta.6) is Servo stripped down: html5ever, Stylo, Taffy and Parley, with CPU painting via vello_cpu or tiny-skia and sequential styling possible. Nobody has run it on an MCU, and it has no shadow DOM and no damage tracking.

## Decision
Use Blitz pinned at `=0.3.0-beta.6`, `default-features = false`, without blitz-net or blitz-shell, behind a private `engine` module so it can be replaced. Pin or fork on breakage. M0 measures build, P4 image size and peak heap; the go/fallback/stop thresholds are in the roadmap. The fallback is our own subset cascade on html5ever, cssparser and selectors, keeping Taffy and Parley.

## Consequences
- Fast start with a maintained layout and style stack.
- Beta dependency churn, absorbed by the pin and the private module.
- The M0 numbers will be recorded here or in the superseding ADR.
