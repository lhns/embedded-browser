# Test pages

The repo is public, so committed pages are synthetic; real sites are fetched at test time.

| Page | What it stands for |
|---|---|
| `dashboard.html` | Home-Assistant-like dashboard: grid of cards, CSS-only icons, 1024x600 design. No real entities. |
| `article.html` | Encyclopedia-style article about a fictional town: floats, tables, lists, embedded PNG data URIs, ~32 KB of generated text. |
| `links.html` | Link-aggregator front page with login, comment and search forms. |

- `generate.py` regenerates the three pages: `python testpages/generate.py testpages`.
- `fetch.sh` downloads real pages and their stylesheets into `fetched/` (gitignored). Never commit them, and never commit a capture of anyone's own Home Assistant.
- `js/` belongs to the JS track.
