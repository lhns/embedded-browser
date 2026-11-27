# 0005. Licensing and corpus policy

Status: Accepted
Date: 2026-11-27

## Context
The repository is public. Stylo is MPL-2.0 and the dependency tree mixes permissive licenses. Test pages could leak private data.

## Decision
- Our code is MIT OR Apache-2.0. Stylo stays unmodified under MPL-2.0. Fonts are OFL.
- `cargo-deny` allows only MIT, Apache-2.0, MPL-2.0, BSD-2/3, ISC, Zlib, Unicode-3.0 and OFL-1.1.
- Committed corpus pages are synthetic (an HA-like dashboard, an article page, the custom dashboard) or have a compatible license with attribution (Wikipedia, CC BY-SA). Other real sites are fetched at test time into `testpages/fetched/`, which is git-ignored.
- Never commit a capture of a real Home Assistant instance (entities, tokens).

## Consequences
- A forked Stylo would have to publish its changes under MPL-2.0.
- Golden images come from synthetic pages, so they are safe to publish.
- License checks run in CI via `deny.toml`.
