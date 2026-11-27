#!/usr/bin/env bash
# Download real pages for local measurement into testpages/fetched/ (gitignored).
# Never commit the output: the repo is public and these pages carry their own licences.
# Stylesheets are downloaded next to each page and the links rewritten to local files,
# because the measure tool has no network. <img> sources are fetched the same way;
# CSS background images and scripts are not.
set -euo pipefail
cd "$(dirname "$0")"
out=fetched
mkdir -p "$out"
ua="embedded-browser-measure/0.0 (test corpus fetch)"

fetch() { # name url
  local name=$1 url=$2 origin html i=0 href abs pat
  origin=$(printf '%s' "$url" | sed -E 's#^(https?://[^/]+).*#\1#')
  html="$out/$name.html"
  curl -fsSL -A "$ua" "$url" -o "$html"
  local hrefs
  hrefs=$(grep -oE '<link[^>]*rel="stylesheet"[^>]*>' "$html" | grep -oE 'href="[^"]*"' |
    sed -E 's/^href="(.*)"$/\1/' || true)
  for href in $hrefs; do
    i=$((i + 1))
    abs=$(printf '%s' "$href" | sed 's/&amp;/\&/g')
    case $abs in
      //*) abs="https:$abs" ;;
      /*) abs="$origin$abs" ;;
      http*) ;;
      *) abs="${url%/*}/$abs" ;;
    esac
    curl -fsSL -A "$ua" "$abs" -o "$out/$name-$i.css" || { echo "skip $abs" >&2; continue; }
    # Replace this exact href with the local file name (sed-escape it first).
    pat=$(printf '%s' "$href" | sed -e 's/[]\/$*.^&[]/\\&/g')
    sed -i "s/href=\"$pat\"/href=\"$name-$i.css\"/" "$html"
  done
  # Same for <img src=...>: download each distinct image and point the page at the copy.
  local srcs n=0
  srcs=$(grep -oE '<img[^>]*src="[^"]*"' "$html" | grep -oE ' src="[^"]*"' |
    sed -E 's/^ src="(.*)"$/\1/' | sort -u || true)
  for href in $srcs; do
    case $href in data:*) continue ;; esac
    n=$((n + 1))
    abs=$(printf '%s' "$href" | sed 's/&amp;/\&/g')
    case $abs in
      //*) abs="https:$abs" ;;
      /*) abs="$origin$abs" ;;
      http*) ;;
      *) abs="${url%/*}/$abs" ;;
    esac
    local file="$name-img-$n"
    curl -fsSL -A "$ua" "$abs" -o "$out/$file" || { echo "skip $abs" >&2; continue; }
    pat=$(printf '%s' "$href" | sed -e 's/[]\/$*.^&[]/\\&/g')
    sed -i "s/src=\"$pat\"/src=\"$file\"/g" "$html"
  done
  echo "$html: $(wc -c < "$html") bytes, $i stylesheet(s), $n image(s)"
}

fetch wikipedia-home-automation "https://en.wikipedia.org/wiki/Home_automation"
fetch hn-front "https://news.ycombinator.com/"
