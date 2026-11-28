#!/usr/bin/env bash
# Regenerates testpages/reference-servo/{dashboard,article,links}.png: screenshots
# of the test pages from real Servo (servoshell, headless), a second reference next
# to Chromium (scripts/reference-screenshots.sh). Our engine (Blitz) shares only
# html5ever and Stylo with Servo, so this shows how close we are to "Servo".
#
# Servo: release v0.8.0-rc1 (servoshell "Servo 0.8.0-f10f711bc"), the official
#   linux x86_64 build from servo/servo-nightly-builds, pinned by URL + SHA-256
#   (the "latest nightly" URL moves, so it is not used).
# Container: ubuntu:24.04 plus the GStreamer/GL libs servoshell links against; Servo
#   falls back to Mesa software GL. Image and container are named eb-servo-* and the
#   image is removed on exit (KEEP_IMAGE=1 keeps it for faster reruns).
# Flags: -z (headless) -x (exit once the image is written) -o out.png
#        --window-size=1024x600 --screen-size=1024x600 --device-pixel-ratio=1.
# Fonts: crates/embedded-browser-assets/fonts is mounted over /usr/share/fonts, so
#        fontconfig sees only NotoSans-Regular/Bold, as in the Chromium script.
# JS:    Servo has no pref to switch it off, so the script refuses pages with scripts.
set -euo pipefail
cd "$(dirname "$0")/.."

URL=https://github.com/servo/servo-nightly-builds/releases/download/v0.8.0-rc1/servo-x86_64-linux-gnu.tar.gz
SHA256=c9e59ec372c8cc5dc58f863d879fcf3b0476f8be29ba80217bd8c8828ed6a0d7
BASE=ubuntu:24.04@sha256:562111b88caa79c71391c0c4d3c2114a7d5c35915c20561fd893cb1e07929e6b
IMAGE=eb-servo-runtime
# Windows Git Bash: native paths for docker, and no mangling of container paths.
export MSYS_NO_PATHCONV=1
root=$(pwd -W 2>/dev/null || pwd)

if grep -qiE '<script| on[a-z]+=|javascript:' testpages/{dashboard,article,links}.html; then
  echo "test pages must not contain scripts (JS cannot be disabled in Servo)" >&2
  exit 1
fi

[ -n "${KEEP_IMAGE:-}" ] || trap 'docker rmi -f $IMAGE >/dev/null 2>&1 || true' EXIT

docker build -q -t $IMAGE - <<DOCKERFILE
FROM $BASE
RUN apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
      curl ca-certificates fontconfig libegl1 libgl1-mesa-dri libgles2 libfreetype6 libssl3 \
      libxkbcommon0 libx11-6 libwayland-client0 libasound2t64 libgbm1 libglib2.0-0t64 \
      libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 libgstreamer-plugins-bad1.0-0 \
 && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL -o /tmp/servo.tgz $URL \
 && echo "$SHA256  /tmp/servo.tgz" | sha256sum -c - \
 && mkdir /opt/servo && tar xzf /tmp/servo.tgz -C /opt/servo --strip-components=1 && rm /tmp/servo.tgz
DOCKERFILE

mkdir -p testpages/reference-servo
docker run --rm --name eb-servo-run \
  -v "$root/crates/embedded-browser-assets/fonts:/usr/share/fonts:ro" \
  -v "$root/testpages:/pages:ro" \
  -v "$root/testpages/reference-servo:/out" \
  "$IMAGE" sh -c '
    set -e
    mkdir -p /run/xdg && chmod 700 /run/xdg && export XDG_RUNTIME_DIR=/run/xdg
    fc-cache -f
    echo "fonts:"; fc-list
    for f in sans-serif serif monospace "sans-serif:bold"; do echo "fc-match $f -> $(fc-match "$f")"; done
    /opt/servo/servoshell --version
    for page in dashboard article links; do
      timeout 120 /opt/servo/servoshell -z -x --window-size=1024x600 --screen-size=1024x600 \
        --device-pixel-ratio=1 -o /out/$page.png file:///pages/$page.html
    done
    chown '"$(id -u):$(id -g)"' /out/*.png'
