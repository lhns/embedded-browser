//! M0 JS host spike: runs corpus page scripts in QuickJS-ng under memory caps with a logging
//! stub DOM and prints a Markdown report (peak heap, compile/run time, cap pass/fail,
//! exceptions, web APIs touched).
//!
//! Usage: `js-spike [--root DIR] [--item NAME]... [--repeat N] [--out FILE]`

mod engine;
mod html;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use engine::{Item, Outcome};

const CAPS: [usize; 3] = [1 << 20, 4 << 20, 16 << 20];

/// Corpus: (name, page relative to the repo root, fixtures relative to the repo root,
/// counts toward the J2 API set).
const CORPUS: &[(&str, &str, &str, bool)] = &[
    (
        "dashboard",
        "testpages/js/dashboard/index.html",
        "testpages/js/dashboard/fixtures.json",
        true,
    ),
    (
        "htmx",
        "testpages/js/htmx/index.html",
        "testpages/js/htmx/fixtures.json",
        true,
    ),
    (
        "petite-vue",
        "testpages/js/petite-vue/index.html",
        "testpages/js/petite-vue/fixtures.json",
        false,
    ),
    (
        "tasmota",
        "testpages/js/tasmota/index.html",
        "testpages/js/tasmota/fixtures.json",
        true,
    ),
    (
        "esphome-v1",
        "testpages/js/esphome-v1/index.html",
        "testpages/js/esphome-v1/fixtures.json",
        true,
    ),
    (
        "home-assistant",
        "testpages/fetched/home-assistant/index.html",
        "testpages/js/home-assistant/fixtures.json",
        false,
    ),
];

/// Pages whose scripts are fetched at test time (testpages/fetched/).
fn fetched_dependency(name: &str) -> Option<&'static str> {
    match name {
        "tasmota" => Some("testpages/fetched/tasmota/root.js"),
        "esphome-v1" => Some("testpages/fetched/esphome-v1/www.js"),
        "home-assistant" => Some("testpages/fetched/home-assistant/index.html"),
        _ => None,
    }
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn corpus_item(root: &Path, name: &str) -> Option<Item> {
    let (n, page, fixtures, _) = CORPUS.iter().find(|c| c.0 == name)?;
    if let Some(dep) = fetched_dependency(n)
        && !root.join(dep).exists()
    {
        return None;
    }
    Some(Item {
        name: n.to_string(),
        page: root.join(page),
        fixtures: std::fs::read_to_string(root.join(fixtures)).unwrap_or_else(|_| "{}".into()),
    })
}

/// ES2020+ syntax and built-ins, each compiled and run on its own.
pub const SYNTAX_PROBES: &[(&str, &str)] = &[
    (
        "optional chaining ?. (ES2020)",
        "const o = {a: {b: 1}}; if (o?.a?.b !== 1 || o.x?.y !== undefined) throw 0;",
    ),
    ("nullish ?? (ES2020)", "if ((null ?? 2) !== 2) throw 0;"),
    (
        "BigInt (ES2020)",
        "if (2n ** 64n !== 18446744073709551616n) throw 0;",
    ),
    (
        "Promise.allSettled, globalThis (ES2020)",
        "Promise.allSettled([]); globalThis.x1 = 1;",
    ),
    (
        "logical assignment ||= &&= ??= (ES2021)",
        "let a = null; a ??= 1; a ||= 2; a &&= 3; if (a !== 3) throw 0;",
    ),
    (
        "numeric separators (ES2021)",
        "if (1_000_000 !== 1e6) throw 0;",
    ),
    (
        "String.replaceAll, Promise.any (ES2021)",
        "if ('aa'.replaceAll('a', 'b') !== 'bb') throw 0; Promise.any([1]);",
    ),
    (
        "class fields, private methods, static blocks (ES2022)",
        "class A { #x = 1; static s; static { A.s = 2; } #m() { return this.#x; } get v() { return this.#m(); } static has(o) { return #x in o; } } if (new A().v !== 1 || A.s !== 2 || !A.has(new A())) throw 0;",
    ),
    (
        "Array.at, Object.hasOwn, Error cause (ES2022)",
        "if ([1, 2].at(-1) !== 2 || !Object.hasOwn({a: 1}, 'a') || new Error('x', {cause: 1}).cause !== 1) throw 0;",
    ),
    (
        "RegExp d flag, named groups, lookbehind",
        "const m = /(?<y>\\d{4})/d.exec('in 2024'); if (m.groups.y !== '2024' || !m.indices || !/(?<=a)b/.test('ab')) throw 0;",
    ),
    (
        "findLast, toSorted, Array.with (ES2023)",
        "if ([1, 2, 3].findLast((x) => x < 3) !== 2 || [3, 1].toSorted()[0] !== 1 || [1, 2].with(0, 5)[0] !== 5) throw 0;",
    ),
    (
        "Object.groupBy, Promise.withResolvers (ES2024)",
        "if (Object.groupBy([1, 2], (x) => x % 2 ? 'o' : 'e').o[0] !== 1) throw 0; Promise.withResolvers();",
    ),
    (
        "RegExp v flag (ES2024)",
        "if (!/[\\p{L}--[a-z]]/v.test('A')) throw 0;",
    ),
    (
        "Set methods, Iterator helpers (ES2025)",
        "if (new Set([1, 2]).union(new Set([3])).size !== 3 || [1, 2].values().map((x) => x * 2).toArray()[1] !== 4) throw 0;",
    ),
    (
        "async generators, for await (ES2018)",
        "(async () => { for await (const x of (async function* () { yield 1; })()) {} })();",
    ),
    (
        "Intl (ECMA-402)",
        "if (typeof Intl !== 'object' || typeof Intl.NumberFormat !== 'function') throw 'Intl missing';",
    ),
];

fn kb(b: usize) -> String {
    format!("{:.0} KB", b as f64 / 1024.0)
}

fn ms(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64() * 1e3)
}

/// Web API groups for the report.
pub fn group(api: &str) -> &'static str {
    let a = api;
    let has = |p: &[&str]| p.iter().any(|x| a.contains(x));
    if a.starts_with("selector") {
        "selectors"
    } else if has(&[
        "CustomElementRegistry",
        "ShadowRoot",
        "attachShadow",
        "shadowRoot",
        "new HTMLElement",
        "HTMLSlotElement",
        "adoptedStyleSheets",
        "customElements",
        "assignedSlot",
        "CSSStyleSheet",
    ]) {
        "custom elements / shadow DOM"
    } else if has(&[
        "canvas",
        "Canvas",
        "Path2D",
        "ImageData",
        "WebGL",
        "OffscreenCanvas",
    ]) {
        "canvas"
    } else if has(&[
        "fetch",
        "XMLHttpRequest",
        "WebSocket",
        "EventSource",
        "Response",
        "Headers",
        "Request",
        "Abort",
        "FormData",
        "URL",
        "navigation",
        "form submission",
        "sendBeacon",
    ]) {
        "network"
    } else if has(&[
        "setTimeout",
        "setInterval",
        "clearTimeout",
        "clearInterval",
        "AnimationFrame",
        "IdleCallback",
        "queueMicrotask",
        "performance",
    ]) {
        "timers"
    } else if has(&[
        "Storage",
        "localStorage",
        "sessionStorage",
        "indexedDB",
        "caches",
        "Document.cookie",
    ]) {
        "storage"
    } else if has(&[
        "Event",
        "addEventListener",
        "removeEventListener",
        "dispatchEvent",
        "inline on",
    ]) || is_handler_prop(a)
    {
        "events"
    } else if has(&[
        "Node.",
        "Element.",
        "Document.",
        "DocumentFragment",
        "Text.",
        "Comment",
        "CharacterData",
        "HTML",
        "DOMTokenList",
        "CSSStyleDeclaration",
        "TreeWalker",
        "DOMParser",
        "getComputedStyle",
        "MutationObserver",
        "new Text",
        "createEvent",
        "createElementNS",
        "document.",
        "DOMImplementation",
    ]) {
        "DOM core"
    } else {
        "other"
    }
}

fn is_handler_prop(a: &str) -> bool {
    a.split('.').nth(1).is_some_and(|m| {
        m.starts_with("on")
            && m[2..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
    })
}

pub const GROUPS: &[&str] = &[
    "DOM core",
    "events",
    "timers",
    "network",
    "storage",
    "custom elements / shadow DOM",
    "canvas",
    "other",
    "selectors",
];

struct ItemResult {
    name: String,
    j2: bool,
    best: Outcome,
    stripped: Outcome,
    caps: Vec<Outcome>,
}

/// Heap without the stub DOM's own share (a device would implement the DOM natively).
fn page_heap(o: &Outcome, v: usize) -> usize {
    v.saturating_sub(o.baseline.saturating_sub(o.context))
}

/// A capped run passes when it completes all phases with the same exceptions and the same
/// API log as the uncapped run (an out-of-memory error swallowed by a page shows up as a
/// different API log).
fn cap_verdict(free: &Outcome, capped: &Outcome) -> String {
    let oom = capped
        .errors
        .iter()
        .any(|e| e.contains("out of memory") || e.contains("InternalError"));
    if capped.completed() && capped.errors == free.errors && capped.apis == free.apis && !oom {
        "pass".into()
    } else {
        let phase = capped.phases.last().copied().unwrap_or("start");
        format!("FAIL after {phase}{}", if oom { " (OOM)" } else { "" })
    }
}

fn run_item(item: &Item, repeat: usize) -> (Outcome, Outcome, Vec<Outcome>) {
    // Timings: the minimum over `repeat` uncapped runs.
    let mut best = engine::run(item, None, false);
    for _ in 1..repeat {
        let o = engine::run(item, None, false);
        for (b, s) in best.scripts.iter_mut().zip(o.scripts.iter()) {
            b.compile = b.compile.min(s.compile);
            b.run = b.run.min(s.run);
        }
        best.events_time = best.events_time.min(o.events_time);
    }
    let stripped = engine::run(item, None, true);
    let caps = CAPS
        .iter()
        .map(|&c| engine::run(item, Some(c), false))
        .collect();
    (best, stripped, caps)
}

fn report(
    results: &[ItemResult],
    base: (usize, usize, usize),
    probes: &[(String, bool, String)],
) -> String {
    let mut r = String::new();
    let ptr = std::mem::size_of::<usize>() * 8;
    let _ = writeln!(
        r,
        "Target: {} ({ptr}-bit), {}; QuickJS-ng {}.\n",
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "DEBUG build (timings not meaningful)"
        } else {
            "release build"
        },
        quickjs_version()
    );
    let _ = writeln!(
        r,
        "Baseline: runtime + full context {}; with the stub DOM loaded {} (stub share {}). \
         Heaps below are allocator peaks over the run including runtime and context but \
         excluding the stub DOM's share; caps are QuickJS `set_memory_limit` values with the \
         stub's QuickJS-accounted share added on top.\n",
        kb(base.0),
        kb(base.1),
        kb(base.1.saturating_sub(base.0)),
    );
    let _ = base.2;
    let _ = writeln!(
        r,
        "| item | script bytes | compile ms | run ms | events ms | peak heap | heap at end | peak, stripped bytecode | 1 MB | 4 MB | 16 MB | exceptions | APIs |"
    );
    let _ = writeln!(r, "|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for it in results {
        let b = &it.best;
        let compile: Duration = b.scripts.iter().map(|s| s.compile).sum();
        let run: Duration = b.scripts.iter().map(|s| s.run).sum();
        let verdicts: Vec<String> = it.caps.iter().map(|c| cap_verdict(b, c)).collect();
        let _ = writeln!(
            r,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            it.name,
            b.source_bytes(),
            ms(compile),
            ms(run),
            ms(b.events_time),
            kb(page_heap(b, b.peak)),
            kb(page_heap(b, b.end)),
            kb(page_heap(&it.stripped, it.stripped.peak)),
            verdicts[0],
            verdicts[1],
            verdicts[2],
            b.errors.len(),
            b.apis.iter().filter(|a| !a.starts_with("selector")).count(),
        );
    }
    let _ = writeln!(r, "\n### Syntax probe\n");
    for (name, ok, err) in probes {
        let _ = writeln!(
            r,
            "- {}: {}{}",
            name,
            if *ok { "ok" } else { "FAILED" },
            if err.is_empty() {
                String::new()
            } else {
                format!(" ({err})")
            }
        );
    }
    for it in results {
        let b = &it.best;
        let _ = writeln!(r, "\n### {}\n", it.name);
        let _ = writeln!(
            r,
            "Phases completed: {}. Elements at end: {}. Tasks run: {}. Interactions: {}. \
             Heap without stub share: peak during scripts {}, peak overall {}, at end {}; \
             with stripped bytecode: peak {}, at end {}. Raw (with stub): peak {}, QuickJS \
             malloc_size at end {} ({} objects).\n",
            b.phases.join(" → "),
            b.elements,
            b.tasks_run,
            b.interactions,
            kb(page_heap(b, b.peak_scripts)),
            kb(page_heap(b, b.peak)),
            kb(page_heap(b, b.end)),
            kb(page_heap(&it.stripped, it.stripped.peak)),
            kb(page_heap(&it.stripped, it.stripped.end)),
            kb(b.peak),
            kb(b.qjs_malloc_size as usize),
            b.qjs_obj_count
        );
        let _ = writeln!(
            r,
            "| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |\n|---|---|---|---|---|---|---|"
        );
        for (i, s) in b.scripts.iter().enumerate() {
            let (bc, load) = match it.stripped.scripts.get(i) {
                Some(st) if st.bytecode > 0 => (st.bytecode.to_string(), ms(st.load)),
                _ => ("-".into(), "-".into()),
            };
            let rate = if s.compile.as_secs_f64() > 0.0 && s.bytes > 0 {
                format!(
                    "{:.0}",
                    s.bytes as f64 / 1024.0 / (s.compile.as_secs_f64() * 1e3)
                )
            } else {
                "-".into()
            };
            let short = s.name.rsplit('/').next().unwrap_or(&s.name);
            let _ = writeln!(
                r,
                "| {}{} | {} | {} | {} | {} | {} | {} |",
                short,
                if s.module { " (module)" } else { "" },
                s.bytes,
                ms(s.compile),
                rate,
                ms(s.run),
                bc,
                load
            );
        }
        for (c, o) in CAPS.iter().zip(&it.caps) {
            let v = cap_verdict(b, o);
            if v != "pass" {
                let extra: Vec<&String> = o
                    .errors
                    .iter()
                    .filter(|e| !b.errors.contains(e))
                    .take(2)
                    .collect();
                let _ = writeln!(
                    r,
                    "\nCap {} MB: {v}; peak without stub share {}; first new errors: {:?}",
                    c >> 20,
                    kb(page_heap(o, o.peak)),
                    extra
                        .iter()
                        .map(|e| truncate(&e.replace('\n', " ⏎ "), 200))
                        .collect::<Vec<_>>()
                );
            }
        }
        if !b.errors.is_empty() {
            let _ = writeln!(r, "\nExceptions ({}):\n", b.errors.len());
            for e in b.errors.iter().take(15) {
                let one = e.replace('\n', " ⏎ ");
                let _ = writeln!(r, "- `{}`", truncate(&one, 300));
            }
        }
        if !b.net.is_empty() {
            let _ = writeln!(
                r,
                "\nNetwork ({} requests/messages): {}",
                b.net.len(),
                b.net
                    .iter()
                    .take(12)
                    .map(|n| format!("`{}`", truncate(n, 80)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let _ = writeln!(r, "\nWeb APIs touched:\n");
        let grouped = group_apis(&b.apis);
        for g in GROUPS {
            if let Some(list) = grouped.get(g) {
                let shown: Vec<String> = list.iter().map(|a| format!("`{a}`")).collect();
                let _ = writeln!(r, "- **{g}** ({}): {}", list.len(), shown.join(", "));
            }
        }
    }
    r
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(n).collect::<String>())
    }
}

pub fn group_apis(apis: &BTreeSet<String>) -> BTreeMap<&'static str, Vec<String>> {
    let mut m: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for a in apis {
        m.entry(group(a)).or_default().push(a.clone());
    }
    m
}

/// The union of APIs over the J2 corpus items, without selector strings.
fn j2_set(results: &[ItemResult]) -> String {
    let mut union: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for it in results.iter().filter(|i| i.j2) {
        for a in it.best.apis.iter().filter(|a| !a.starts_with("selector")) {
            union.entry(a.clone()).or_default().insert(it.name.clone());
        }
    }
    let set: BTreeSet<String> = union.keys().cloned().collect();
    let grouped = group_apis(&set);
    let ifaces: BTreeSet<&str> = set
        .iter()
        .filter_map(|a| a.split(['.', ' ', '(']).next())
        .collect();
    let mut r = String::new();
    let _ = writeln!(
        r,
        "J2 set: union over {} — {} distinct API paths on {} interfaces/globals.\n",
        results
            .iter()
            .filter(|i| i.j2)
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        set.len(),
        ifaces.len()
    );
    for g in GROUPS {
        if let Some(list) = grouped.get(g) {
            let shown: Vec<String> = list
                .iter()
                .map(|a| {
                    format!(
                        "`{a}` ({})",
                        union[a]
                            .iter()
                            .map(|s| &s[..2])
                            .collect::<Vec<_>>()
                            .join("/")
                    )
                })
                .collect();
            let _ = writeln!(r, "- **{g}** ({}): {}", list.len(), shown.join(", "));
        }
    }
    r
}

fn quickjs_version() -> String {
    // SAFETY: JS_GetVersion returns a static NUL-terminated string.
    unsafe { std::ffi::CStr::from_ptr(rquickjs::qjs::JS_GetVersion()) }
        .to_string_lossy()
        .into_owned()
}

pub fn run_probes() -> Vec<(String, bool, String)> {
    let rt = rquickjs::Runtime::new().expect("runtime");
    let ctx = rquickjs::Context::full(&rt).expect("context");
    ctx.with(|ctx| {
        SYNTAX_PROBES
            .iter()
            .map(|(name, code)| match ctx.eval::<(), _>(*code) {
                Ok(()) => (name.to_string(), true, String::new()),
                Err(_) => {
                    let v = ctx.catch();
                    let msg = v
                        .as_object()
                        .and_then(|o| o.get::<_, String>("message").ok())
                        .or_else(|| v.as_string().and_then(|s| s.to_string().ok()))
                        .unwrap_or_else(|| format!("{v:?}"));
                    (name.to_string(), false, msg)
                }
            })
            .collect()
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root = repo_root();
    let mut only: Vec<String> = Vec::new();
    let mut repeat = 3;
    let mut out: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => root = PathBuf::from(args.next().expect("--root DIR")),
            "--item" => only.push(args.next().expect("--item NAME")),
            "--repeat" => {
                repeat = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--repeat N")
            }
            "--out" => out = Some(PathBuf::from(args.next().expect("--out FILE"))),
            _ => {
                eprintln!(
                    "usage: js-spike [--root DIR] [--item NAME]... [--repeat N] [--out FILE]"
                );
                std::process::exit(2);
            }
        }
    }
    // Deep recursion in the parser needs more than the default main-thread stack on Windows.
    let handle = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let empty = engine::run_empty();
            let base = (empty.context, empty.baseline, empty.peak);
            let mut results = Vec::new();
            for (name, _, _, j2) in CORPUS {
                if !only.is_empty() && !only.iter().any(|o| o == name) {
                    continue;
                }
                let Some(item) = corpus_item(&root, name) else {
                    eprintln!(
                        "skip {name}: fetched files missing (run tools/js-spike/fetch_corpus.py)"
                    );
                    continue;
                };
                eprintln!("running {name}...");
                let (best, stripped, caps) = run_item(&item, repeat);
                results.push(ItemResult {
                    name: name.to_string(),
                    j2: *j2,
                    best,
                    stripped,
                    caps,
                });
            }
            let mut text = report(&results, base, &run_probes());
            text.push('\n');
            text.push_str(&j2_set(&results));
            text
        })
        .expect("thread");
    let text = handle.join().expect("spike thread panicked");
    match out {
        Some(p) => std::fs::write(&p, text).expect("write report"),
        None => print!("{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_named(name: &str) -> Outcome {
        let item = corpus_item(&repo_root(), name).expect("committed corpus item");
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(move || engine::run(&item, Some(16 << 20), false))
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn dashboard_runs_and_updates_the_dom() {
        let o = run_named("dashboard");
        assert!(
            o.completed(),
            "phases {:?}, errors {:?}",
            o.phases,
            o.errors
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(o.apis.contains("fetch"));
        assert!(
            o.apis.iter().any(|a| a.starts_with("WebSocket")),
            "{:?}",
            o.apis
        );
        assert!(o.body_text.contains("Power412 W"), "{}", o.body_text);
        assert!(o.interactions > 0);
    }

    #[test]
    fn htmx_runs_and_swaps_content() {
        let o = run_named("htmx");
        assert!(
            o.completed(),
            "phases {:?}, errors {:?}",
            o.phases,
            o.errors
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(
            o.apis.iter().any(|a| a.starts_with("XMLHttpRequest")),
            "{:?}",
            o.apis
        );
        assert!(o.body_text.contains("Swapped by htmx"), "{}", o.body_text);
    }

    #[test]
    fn petite_vue_runs_and_renders() {
        let o = run_named("petite-vue");
        assert!(
            o.completed(),
            "phases {:?}, errors {:?}",
            o.phases,
            o.errors
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert!(o.body_text.contains("Count: 1"), "{}", o.body_text);
    }

    #[test]
    fn fetched_items_run_when_present() {
        // Tasmota and ESPHome need fetch_corpus.py; skipped offline.
        for name in ["tasmota", "esphome-v1"] {
            if corpus_item(&repo_root(), name).is_none() {
                continue;
            }
            let o = run_named(name);
            assert!(
                o.completed(),
                "{name}: phases {:?}, errors {:?}",
                o.phases,
                o.errors
            );
        }
    }

    #[test]
    fn modern_syntax_parses() {
        for (name, ok, err) in run_probes() {
            if name.starts_with("Intl") {
                continue; // QuickJS-ng has no Intl; reported, not required.
            }
            assert!(ok, "{name}: {err}");
        }
    }

    #[test]
    fn groups_apis() {
        assert_eq!(group("Node.appendChild"), "DOM core");
        assert_eq!(
            group("Element.attachShadow"),
            "custom elements / shadow DOM"
        );
        assert_eq!(group("HTMLElement.onclick (set)"), "events");
        assert_eq!(group("XMLHttpRequest.open"), "network");
        assert_eq!(group("setTimeout"), "timers");
        assert_eq!(group("Storage.getItem"), "storage");
        assert_eq!(group("canvas getContext(2d)"), "canvas");
        assert_eq!(group("selector: #id"), "selectors");
    }
}
