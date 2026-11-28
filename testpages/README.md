# Test pages

The repo is public, so committed pages are synthetic; real sites are fetched at test time.

| Page | What it stands for |
|---|---|
| `dashboard.html` | Home-Assistant-like dashboard: grid of cards, CSS-only icons, 1024x600 design. No real entities. |
| `article.html` | Encyclopedia-style article about a fictional town: floats, tables, lists, embedded PNG data URIs, ~32 KB of generated text. |
| `links.html` | Link-aggregator front page with login, comment and search forms. |

- `generate.py` regenerates the three pages: `python testpages/generate.py testpages`.
- `fetch.sh` downloads real pages and their stylesheets into `fetched/` (gitignored). Never commit them, and never commit a capture of anyone's own Home Assistant.
- `golden/` holds the engine's renders, checked with a tolerance by `cargo test -p embedded-browser --test golden` (`UPDATE_GOLDENS=1` rewrites them).
- `reference/` holds headless Chromium screenshots for comparison (`scripts/reference-screenshots.sh`); see docs/m1/side-by-side.md.
- `js/` belongs to the JS track.
- `reference-servo/` holds headless Servo screenshots, a second reference (`scripts/servo-screenshots.sh`).
