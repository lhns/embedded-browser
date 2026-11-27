# JS spike corpus

Pages for `tools/js-spike` (results: `docs/m0/js-spike-results.md`). Each directory holds an
`index.html` and `fixtures.json` (canned HTTP, WebSocket and SSE replies).

| Directory | Contents | Licence |
|---|---|---|
| `dashboard/` | Synthetic hand-written smart-home panel (REST + HA-style WebSocket) | ours |
| `htmx/` | Synthetic page; `htmx.min.js` is htmx 2.0.11 from npm `htmx.org` | 0BSD, `htmx/LICENSE` |
| `petite-vue/` | Synthetic page; `petite-vue.iife.js` is petite-vue 0.4.1 from npm | MIT, `petite-vue/LICENSE` |
| `tasmota/` | Synthetic host markup; the script is fetched (Tasmota v16.0.0) | script GPL-3.0, not committed |
| `esphome-v1/` | Synthetic host markup; the script is fetched (esphome-webserver v1) | script not committed |
| `home-assistant/` | Fixtures only; the page and bundle are fetched (frontend 20261125.1) | not committed |

Fetched files land in the gitignored `testpages/fetched/`: `python tools/js-spike/fetch_corpus.py`.
