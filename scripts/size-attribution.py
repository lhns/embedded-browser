#!/usr/bin/env python3
"""Attribute an ELF's code and read-only data to crates and groups (Stylo, ICU, fonts, ...).

Usage: <toolchain>-nm -S --size-sort -C firmware.elf | python3 size-attribution.py [top_n]

Heuristic: the crate is the first path segment of the demangled symbol (`<a::B as c::D>::f` counts
for `a`). Symbols without a Rust path (C code from ESP-IDF and newlib) count as `C`. Inlined code
is attributed to the function it was inlined into, so the split is approximate.
"""

import re
import sys
from collections import defaultdict

GROUPS = {
    "layout (taffy)": ("taffy", "stylo_taffy"),
    "stylo (style, selectors, cssparser)": (
        "style", "stylo", "selectors", "cssparser", "servo_arc", "style_traits",
        "malloc_size_of", "to_shmem", "precomputed_hash", "string_cache", "atomic_refcell",
        "app_units", "euclid",
    ),
    "icu (icu_*, data)": ("icu_", "zerovec", "zerotrie", "yoke", "tinystr", "writeable", "potential_utf"),
    "fonts and text (parley, skrifa, harfrust)": (
        "parley", "fontique", "skrifa", "read_fonts", "font_types", "harfrust", "swash", "unicode_",
    ),
    "html (html5ever, markup5ever)": ("html5ever", "markup5ever", "xml5ever", "tendril", "web_atoms"),
    "blitz (dom, html, paint)": ("blitz",),
    "raster (vello_cpu, anyrender, kurbo)": (
        "vello", "anyrender", "kurbo", "peniko", "fearless_simd", "color", "linebender",
    ),
    "image": ("image", "png", "zune", "fdeflate", "miniz"),
    "rust std (std, core, alloc)": ("std", "core", "alloc", "hashbrown", "compiler_builtins"),
    "esp-idf-sys": ("esp_idf_sys",),
}

KINDS = {"t": "text", "w": "text", "r": "rodata", "d": "data", "b": "bss", "v": "data"}


def crate_of(name: str) -> str:
    s = name.lstrip("<&*").removeprefix("mut ").removeprefix("dyn ").lstrip("<&*")
    m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)::", s)
    return m.group(1) if m else "C"


def group_of(crate: str) -> str:
    if crate == "C":
        return "C (ESP-IDF, newlib, libgcc)"
    for group, prefixes in GROUPS.items():
        if any(crate == p or crate.startswith(p.rstrip("_") + "_") for p in prefixes):
            return group
    return "other crates"


def main() -> None:
    top_n = int(sys.argv[1]) if len(sys.argv) > 1 else 25
    by_group = defaultdict(lambda: defaultdict(int))
    by_crate = defaultdict(lambda: defaultdict(int))
    for line in sys.stdin:
        parts = line.rstrip("\n").split(" ", 3)
        if len(parts) < 4:
            continue
        _, size, kind, name = parts
        kind = KINDS.get(kind.lower())
        if kind is None:
            continue
        size = int(size, 16)
        crate = crate_of(name)
        by_crate[crate][kind] += size
        by_group[group_of(crate)][kind] += size

    def table(title, rows, limit=None):
        print(f"\n{title}")
        print(f"{'':44} {'text KB':>9} {'rodata KB':>10} {'data+bss KB':>12}")
        ordered = sorted(rows.items(), key=lambda kv: -(kv[1]["text"] + kv[1]["rodata"]))
        for name, k in ordered[:limit]:
            print(
                f"{name[:44]:44} {k['text'] / 1024:9.0f} {k['rodata'] / 1024:10.0f}"
                f" {(k['data'] + k['bss']) / 1024:12.0f}"
            )
        tot = defaultdict(int)
        for k in rows.values():
            for kind, v in k.items():
                tot[kind] += v
        print(
            f"{'total (symbols with a size)':44} {tot['text'] / 1024:9.0f} {tot['rodata'] / 1024:10.0f}"
            f" {(tot['data'] + tot['bss']) / 1024:12.0f}"
        )

    table("By group", by_group)
    table(f"Top {top_n} crates", by_crate, top_n)


if __name__ == "__main__":
    main()
