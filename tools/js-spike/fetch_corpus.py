#!/usr/bin/env python3
"""Fetch the third-party corpus scripts for the JS host spike into testpages/fetched/.

They are not committed (the repo is public and their licences differ); this script pins
versions and checks SHA-256 digests. Run from anywhere:

    python tools/js-spike/fetch_corpus.py [--no-ha]

- Tasmota v16.0.0: the root page script, assembled from the C string literals in
  tasmota/html_uncompressed/ in the order HandleRoot() sends them (ESP32 variant), with the
  printf placeholders filled (%d -> 2345 ms refresh, %% -> %).
- ESPHome web_server v1: webserver-v1.js from esphome/esphome-webserver at a pinned commit
  (served to devices as www.js).
- Home Assistant frontend 20261125.1: the PyPI wheel; the modern (frontend_latest) entry
  point and the chunks it imports are extracted, plus a minimal index.html that loads them
  the way HA's index template does. Informational only.
"""

import hashlib
import io
import re
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "testpages" / "fetched"

TASMOTA_TAG = "v16.0.0"
TASMOTA_FILES = [  # in page order
    ("HTTP_HEADER1_ES6.h", "2a0513dadd9c02089d323481a7ad72694f3f5eec4bd4bf7e2fb7e47df7807a1b"),
    ("HTTP_SCRIPT_ROOT_NO_WEB_DISPLAY.h", "6e369f0e06cba0656d3c7187ac833b999a54f84edfd9385b4cf44ba8643e01d8"),
    ("HTTP_SCRIPT_ROOT_PART2.h", "c85b76468eb793a235944c16a2d986bce127bd4fc1b0690499b3b5b88ab4f70f"),
    ("HTTP_HEAD_LAST_SCRIPT32.h", "996873eafc91c9d084e3f68b60d32a37a27e91dd1acd0863fc1096119d8de6a2"),
]
ESPHOME_COMMIT = "619c8e9103e501c97831b2f8670fcd949b638645"
ESPHOME_SHA = "2fe4a8133e4c2e7610176597a9ccfc40f79a985b298348a7dd64d12879bf99dc"
HA_VERSION = "20261125.1"
HA_URL = (
    "https://files.pythonhosted.org/packages/ce/37/1813a704980b08728371cdacf41c954134a8c4b92ac5415973876d203879/"
    f"home_assistant_frontend-{HA_VERSION}-py3-none-any.whl"
)
HA_SHA = "56292fc59343a1e15e0eae2502c5443975f66ab085b12e7c0d68e3e72429df89"


def get(url: str, sha: str) -> bytes:
    print(f"fetch {url}")
    req = urllib.request.Request(url, headers={"User-Agent": "embedded-browser-js-spike"})
    with urllib.request.urlopen(req, timeout=300) as r:
        data = r.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha:
        sys.exit(f"sha256 mismatch for {url}: {digest}")
    return data


def c_literals(src: str) -> str:
    """Concatenates the C string literals of a PROGMEM definition, skipping comments."""
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            i = src.find("\n", i)
            i = n if i < 0 else i
        elif src.startswith("/*", i):
            i = src.find("*/", i) + 2
        elif c == '"':
            j, buf = i + 1, []
            while src[j] != '"':
                if src[j] == "\\":
                    nxt = src[j + 1]
                    buf.append({"n": "\n", "t": "\t", "r": "\r", "0": "\0"}.get(nxt, nxt))
                    j += 2
                else:
                    buf.append(src[j])
                    j += 1
            out.append("".join(buf))
            i = j + 1
        else:
            i += 1
    return "".join(out)


def printf(s: str) -> str:
    return re.sub(r"%(%|d|s|i|u)", lambda m: {"%": "%", "d": "2345", "i": "2345", "u": "2345"}.get(m.group(1), ""), s)


def tasmota() -> None:
    parts = []
    for name, sha in TASMOTA_FILES:
        url = f"https://raw.githubusercontent.com/arendst/Tasmota/{TASMOTA_TAG}/tasmota/html_uncompressed/{name}"
        parts.append(printf(c_literals(get(url, sha).decode("utf-8"))))
    head = parts[0]
    parts[0] = head[head.index("<script>") + len("<script>"):]
    parts[-1] = parts[-1].replace("</script>", "")
    d = OUT / "tasmota"
    d.mkdir(parents=True, exist_ok=True)
    (d / "root.js").write_text(
        f"// Tasmota {TASMOTA_TAG} root page script, extracted by tools/js-spike/fetch_corpus.py\n" + "\n".join(parts),
        encoding="utf-8",
    )


def esphome() -> None:
    url = f"https://raw.githubusercontent.com/esphome/esphome-webserver/{ESPHOME_COMMIT}/packages/v1/src/webserver-v1.js"
    d = OUT / "esphome-v1"
    d.mkdir(parents=True, exist_ok=True)
    (d / "www.js").write_bytes(get(url, ESPHOME_SHA))


def home_assistant() -> None:
    d = OUT / "home-assistant"
    cache = OUT / f"home_assistant_frontend-{HA_VERSION}.whl"
    if cache.exists() and hashlib.sha256(cache.read_bytes()).hexdigest() == HA_SHA:
        data = cache.read_bytes()
    else:
        data = get(HA_URL, HA_SHA)
        OUT.mkdir(parents=True, exist_ok=True)
        cache.write_bytes(data)
    z = zipfile.ZipFile(io.BytesIO(data))
    prefix = "hass_frontend/frontend_latest/"
    names = [n for n in z.namelist() if n.startswith(prefix) and n.endswith(".js")]
    d.mkdir(parents=True, exist_ok=True)
    for n in names:
        target = d / "frontend_latest" / n[len(prefix):]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(z.read(n))
    # HA's index template loads core.<hash>.js and app.<hash>.js as modules.
    entry = lambda stem: next(n[len(prefix):] for n in names if re.fullmatch(rf"{prefix}{stem}\.[^/]+\.js", n))
    core, app = entry("core"), entry("app")
    (d / "index.html").write_text(
        "<!doctype html>\n<!-- Minimal host for the Home Assistant frontend bundle (generated by fetch_corpus.py). -->\n"
        "<html><head><meta charset='utf-8'><title>Home Assistant</title>\n"
        f"<script type='module' src='/frontend_latest/{core}'></script>\n"
        f"<script type='module' src='/frontend_latest/{app}'></script>\n"
        "</head><body><home-assistant></home-assistant></body></html>\n",
        encoding="utf-8",
    )
    print(f"home-assistant: {len(names)} chunks, entry {core}, {app}")


def main() -> None:
    tasmota()
    esphome()
    if "--no-ha" not in sys.argv:
        home_assistant()
    print(f"done: {OUT}")


if __name__ == "__main__":
    main()
