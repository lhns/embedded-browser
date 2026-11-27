#!/usr/bin/env bash
# Regenerates testpages/reference/{dashboard,article,links}.png: viewport
# screenshots of the test pages from real headless Chromium, to compare our
# engine's rendering against it.
#
# Image: mcr.microsoft.com/playwright:v1.65.1-noble@sha256:1507cbfc550a63a6b7396791ca4f00df17beb56342d2447b3f106c306f525579
#   (its bundled chrome-headless-shell; chromedp/headless-shell hung on --screenshot here)
# Flags: --window-size=1024,600 --force-device-scale-factor=1 --hide-scrollbars
#        --disable-gpu --no-sandbox. --screenshot captures the viewport only.
#        --blink-settings=scriptEnabled=false makes --screenshot write nothing, so
#        JS stays enabled and the script refuses pages that contain scripts.
# Fonts: crates/embedded-browser-assets/fonts is mounted over /usr/share/fonts, so
#        the only fonts are NotoSans-Regular/Bold and serif/monospace fall back to
#        Noto Sans, as in our engine.
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=mcr.microsoft.com/playwright:v1.65.1-noble@sha256:1507cbfc550a63a6b7396791ca4f00df17beb56342d2447b3f106c306f525579
# Windows Git Bash: native paths for docker, and no mangling of container paths.
export MSYS_NO_PATHCONV=1
root=$(pwd -W 2>/dev/null || pwd)

if grep -qiE '<script| on[a-z]+=|javascript:' testpages/{dashboard,article,links}.html; then
  echo "test pages must not contain scripts (JS is enabled in the reference browser)" >&2
  exit 1
fi

mkdir -p testpages/reference
docker run --rm --name eb-m1-reference \
  -v "$root/crates/embedded-browser-assets/fonts:/usr/share/fonts:ro" \
  -v "$root/testpages:/pages:ro" \
  -v "$root/testpages/reference:/out" \
  "$IMAGE" sh -c '
    set -e
    chrome=$(ls /ms-playwright/chromium_headless_shell-*/*/chrome-headless-shell)
    for page in dashboard article links; do
      "$chrome" --no-sandbox --disable-gpu --hide-scrollbars \
        --force-device-scale-factor=1 --window-size=1024,600 \
        --screenshot=/out/$page.png file:///pages/$page.html 2>/dev/null
    done
    chown '"$(id -u):$(id -g)"' /out/*.png'
