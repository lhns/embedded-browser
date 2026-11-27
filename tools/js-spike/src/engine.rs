//! Runs one corpus page in a fresh QuickJS-ng runtime: stub DOM, scripts in document order
//! (as if deferred), DOMContentLoaded/load, a virtual-clock event loop with canned network
//! replies, then a synthetic click or change on every element with a handler.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::promise::PromiseState;
use rquickjs::{
    Context, Ctx, Function, Module, Object, Persistent, Runtime, Value, allocator::Allocator,
    allocator::RustAllocator, module::Declared, qjs,
};

use crate::html;

const STUB_DOM: &str = include_str!("stub_dom.js");
/// Virtual time the event loop runs after load, and after each interaction.
const LOAD_LOOP_MS: f64 = 10_000.0;
const INTERACT_LOOP_MS: f64 = 3_000.0;
/// Caps so polling pages and big SPAs end.
const MAX_TASKS: usize = 5_000;
const MAX_INTERACTIONS: usize = 200;
const WALL_LIMIT: Duration = Duration::from_secs(20);

/// Heap accounting shared with the allocator: the same per-block accounting QuickJS uses
/// (usable size plus 8 bytes), but over the allocator QuickJS actually calls, so its arena
/// pages count in full.
#[derive(Default)]
pub struct Counter {
    cur: Cell<usize>,
    peak: Cell<usize>,
}

impl Counter {
    fn add(&self, n: usize) {
        let c = self.cur.get() + n;
        self.cur.set(c);
        if c > self.peak.get() {
            self.peak.set(c);
        }
    }
    fn sub(&self, n: usize) {
        self.cur.set(self.cur.get() - n);
    }
    pub fn current(&self) -> usize {
        self.cur.get()
    }
    pub fn peak(&self) -> usize {
        self.peak.get()
    }
    fn reset_peak(&self) {
        self.peak.set(self.cur.get());
    }
}

const OVERHEAD: usize = 8;

struct CountingAllocator(Rc<Counter>);

// SAFETY: every call is forwarded to RustAllocator, which upholds the trait's contract; we only
// read sizes of blocks it returned.
unsafe impl Allocator for CountingAllocator {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        let p = RustAllocator.alloc(size);
        if !p.is_null() {
            self.0
                .add(unsafe { RustAllocator::usable_size(p) } + OVERHEAD);
        }
        p
    }
    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        let p = RustAllocator.calloc(count, size);
        if !p.is_null() {
            self.0
                .add(unsafe { RustAllocator::usable_size(p) } + OVERHEAD);
        }
        p
    }
    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        unsafe {
            self.0.sub(RustAllocator::usable_size(ptr) + OVERHEAD);
            RustAllocator.dealloc(ptr);
        }
    }
    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        unsafe {
            let old = if ptr.is_null() {
                0
            } else {
                RustAllocator::usable_size(ptr) + OVERHEAD
            };
            let p = RustAllocator.realloc(ptr, new_size);
            if !p.is_null() {
                self.0.sub(old);
                self.0.add(RustAllocator::usable_size(p) + OVERHEAD);
            }
            p
        }
    }
    unsafe fn usable_size(ptr: *mut u8) -> usize {
        unsafe { RustAllocator::usable_size(ptr) }
    }
}

pub struct Item {
    pub name: String,
    pub page: PathBuf,
    pub fixtures: String,
}

#[derive(Debug, Default, Clone)]
pub struct ScriptStat {
    pub name: String,
    pub bytes: usize,
    pub module: bool,
    pub compile: Duration,
    pub run: Duration,
    /// Stripped bytecode size and the time to load it (`JS_ReadObject`), stripped runs only.
    pub bytecode: usize,
    pub load: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct Outcome {
    /// QuickJS-accounted heap of the stub DOM, excluded from caps.
    pub stub_qjs: usize,
    /// Allocator peak while the stub DOM was compiled and run.
    pub stub_peak: usize,
    /// Heap after runtime and a full context (all intrinsics), before the stub DOM.
    pub context: usize,
    /// Heap after runtime, context and stub DOM, before the page (allocator view).
    pub baseline: usize,
    /// Peak heap over the whole run (allocator view).
    pub peak: usize,
    /// Peak heap while scripts were compiled and run, before events (allocator view).
    pub peak_scripts: usize,
    /// Live heap at the end, after a GC.
    pub end: usize,
    /// QuickJS's own accounting at the end (what `set_memory_limit` is checked against).
    pub qjs_malloc_size: i64,
    pub qjs_obj_count: i64,
    pub scripts: Vec<ScriptStat>,
    pub modules_loaded: usize,
    pub module_bytes: usize,
    pub events_time: Duration,
    pub errors: Vec<String>,
    pub apis: BTreeSet<String>,
    pub phases: Vec<&'static str>,
    pub elements: i64,
    pub tasks_run: i64,
    pub interactions: usize,
    pub net: Vec<String>,
    pub body_text: String,
}

impl Outcome {
    pub fn completed(&self) -> bool {
        self.phases.last() == Some(&"interactions")
    }
    pub fn source_bytes(&self) -> usize {
        self.scripts.iter().map(|s| s.bytes).sum::<usize>() + self.module_bytes
    }
}

/// Resolves module specifiers against the importing file, and absolute paths against the
/// page directory.
struct FsResolver {
    root: PathBuf,
}

impl Resolver for FsResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        let path = if let Some(abs) = name.strip_prefix('/') {
            self.root.join(abs)
        } else if name.starts_with('.') {
            Path::new(base).parent().unwrap_or(Path::new("")).join(name)
        } else {
            return Err(rquickjs::Error::new_resolving(base, name));
        };
        Ok(normalize(&path).to_string_lossy().replace('\\', "/"))
    }
}

pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

#[derive(Default)]
struct ModuleStats {
    count: usize,
    bytes: usize,
    compile: Duration,
}

struct FsLoader {
    stats: Rc<RefCell<ModuleStats>>,
    root: PathBuf,
    origin: String,
}

/// Gives a module an http `import.meta.url` under the page origin, since bundles resolve
/// assets with `new URL(x, import.meta.url)`.
fn set_meta_url(m: &Module<Declared>, path: &str, root: &Path, origin: &str) {
    let root = normalize(root).to_string_lossy().replace('\\', "/");
    let rel = path
        .strip_prefix(root.as_str())
        .unwrap_or(path)
        .trim_start_matches('/');
    if let Ok(meta) = m.meta() {
        let _ = meta.set("url", format!("{}/{rel}", origin.trim_end_matches('/')));
    }
}

/// The origin of the page URL in the fixtures, e.g. `http://host:8123`.
fn fixture_origin(fixtures: &str) -> String {
    let url = fixtures
        .split("\"url\"")
        .nth(1)
        .and_then(|r| r.split('"').nth(1))
        .unwrap_or("http://localhost/");
    let after = url.find("//").map_or(0, |i| i + 2);
    let end = url[after..].find('/').map_or(url.len(), |i| after + i);
    url[..end].to_string()
}

impl Loader for FsLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        let src = std::fs::read(name).map_err(|_| rquickjs::Error::new_loading(name))?;
        let t = Instant::now();
        let m = Module::declare(ctx.clone(), name, src.clone())?;
        set_meta_url(&m, name, &self.root, &self.origin);
        let mut s = self.stats.borrow_mut();
        s.count += 1;
        s.bytes += src.len();
        s.compile += t.elapsed();
        Ok(m)
    }
}

fn exception_text(ctx: &Ctx) -> String {
    let v = ctx.catch();
    value_text(&v)
}

fn value_text(v: &Value) -> String {
    if let Some(o) = v.as_object() {
        let msg: Option<String> = o.get("message").ok();
        let stack: Option<String> = o.get("stack").ok();
        let name: Option<String> = o.get("name").ok();
        if let Some(m) = msg {
            let first = stack
                .as_deref()
                .and_then(|s| s.lines().find(|l| !l.trim().is_empty()))
                .unwrap_or("")
                .trim()
                .to_string();
            return format!("{}: {m} {first}", name.unwrap_or_else(|| "Error".into()));
        }
    }
    if let Some(s) = v.as_string() {
        return s.to_string().unwrap_or_default();
    }
    format!("{v:?}")
}

fn drain_jobs(ctx: &Ctx) {
    while ctx.execute_pending_job() {}
}

/// A compiled classic script, owned until `run_compiled` consumes it.
struct Compiled(qjs::JSValue);

/// Compiles a classic script without running it, so parse/compile and run time separate.
/// Raw values are used because a `Value` holds a context reference that would leak if the
/// value were forgotten when ownership passes to `JS_EvalFunction`.
fn compile_classic(ctx: &Ctx, src: &str, name: &str) -> Result<Compiled, String> {
    let csrc = CString::new(src).map_err(|_| "script contains NUL".to_string())?;
    let cname = CString::new(name).unwrap_or_default();
    let flags = (qjs::JS_EVAL_TYPE_GLOBAL | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as i32;
    // SAFETY: csrc is NUL-terminated as JS_Eval requires.
    let raw = unsafe {
        qjs::JS_Eval(
            ctx.as_raw().as_ptr(),
            csrc.as_ptr(),
            src.len() as _,
            cname.as_ptr(),
            flags,
        )
    };
    if unsafe { qjs::JS_IsException(raw) } {
        return Err(exception_text(ctx));
    }
    Ok(Compiled(raw))
}

/// Round-trips compiled bytecode through `JS_WriteObject` without the per-function source
/// copies QuickJS-ng otherwise keeps for `Function.prototype.toString` (and, with `debug`,
/// without line tables). This is how precompiled bytecode would be loaded on a device.
fn strip(ctx: &Ctx, func: Compiled, debug: bool) -> Result<(Compiled, usize, Duration), String> {
    let c = ctx.as_raw().as_ptr();
    let mut flags = qjs::JS_WRITE_OBJ_BYTECODE | qjs::JS_WRITE_OBJ_STRIP_SOURCE;
    if debug {
        flags |= qjs::JS_WRITE_OBJ_STRIP_DEBUG;
    }
    // SAFETY: func.0 is an owned compiled function; the buffer is freed with js_free.
    unsafe {
        let mut len = 0;
        let buf = qjs::JS_WriteObject(c, &mut len, func.0, flags as i32);
        qjs::JS_FreeValue(c, func.0);
        if buf.is_null() {
            return Err(format!("JS_WriteObject: {}", exception_text(ctx)));
        }
        let t = Instant::now();
        let v = qjs::JS_ReadObject(c, buf, len, qjs::JS_READ_OBJ_BYTECODE as i32);
        let load = t.elapsed();
        qjs::js_free(c, buf.cast());
        if qjs::JS_IsException(v) {
            return Err(format!("JS_ReadObject: {}", exception_text(ctx)));
        }
        Ok((Compiled(v), len as usize, load))
    }
}

fn run_compiled<'js>(ctx: &Ctx<'js>, func: Compiled) -> Result<Value<'js>, String> {
    // SAFETY: JS_EvalFunction takes ownership of the compiled function; its result is owned
    // and released by Value's drop.
    let r = unsafe {
        Value::from_raw(
            ctx.clone(),
            qjs::JS_EvalFunction(ctx.as_raw().as_ptr(), func.0),
        )
    };
    if r.is_exception() {
        return Err(exception_text(ctx));
    }
    Ok(r)
}

/// Runs `item` once. `cap` is passed to `set_memory_limit`, on top of the stub DOM's own
/// heap (a device would implement the DOM natively, so the stub's JS heap is excluded).
/// `strip_pages` loads page scripts as stripped bytecode (no retained source).
pub fn run(item: &Item, cap: Option<usize>, strip_pages: bool) -> Outcome {
    let mut out = Outcome::default();
    let counter = Rc::new(Counter::default());
    let errors = Rc::new(RefCell::new(Vec::<String>::new()));
    let apis = Rc::new(RefCell::new(BTreeSet::<String>::new()));
    let mstats = Rc::new(RefCell::new(ModuleStats::default()));

    let rt = match Runtime::new_with_alloc(CountingAllocator(counter.clone())) {
        Ok(rt) => rt,
        Err(e) => {
            out.errors.push(format!("runtime: {e}"));
            return out;
        }
    };
    rt.set_max_stack_size(512 * 1024);
    let deadline = Instant::now() + WALL_LIMIT;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    {
        let errors = errors.clone();
        rt.set_host_promise_rejection_tracker(Some(Box::new(move |_ctx, _p, reason, handled| {
            if !handled {
                errors
                    .borrow_mut()
                    .push(format!("unhandled rejection: {}", value_text(&reason)));
            }
        })));
    }
    let root = item.page.parent().unwrap_or(Path::new(".")).to_path_buf();
    rt.set_loader(
        FsResolver { root: root.clone() },
        FsLoader {
            stats: mstats.clone(),
            root: root.clone(),
            origin: fixture_origin(&item.fixtures),
        },
    );

    let ctx = match Context::full(&rt) {
        Ok(c) => c,
        Err(e) => {
            out.errors.push(format!("context: {e}"));
            out.peak = counter.peak();
            return out;
        }
    };
    out.context = counter.current();
    out.phases.push("context");

    let html_src = match std::fs::read_to_string(&item.page) {
        Ok(s) => s,
        Err(e) => {
            out.errors
                .push(format!("page {}: {e}", item.page.display()));
            return out;
        }
    };

    let m_context = rt.memory_usage().malloc_size;
    let saved = ctx.with(|ctx| -> Result<Persistent<Object<'static>>, String> {
        let g = ctx.globals();
        let a = apis.clone();
        let e = errors.clone();
        let hooks = (|| -> rquickjs::Result<()> {
            g.set(
                "__eb_log",
                Function::new(ctx.clone(), move |s: String| {
                    a.borrow_mut().insert(s);
                })?,
            )?;
            g.set(
                "__eb_err",
                Function::new(ctx.clone(), move |s: String| e.borrow_mut().push(s))?,
            )?;
            g.set(
                "__eb_parse",
                Function::new(ctx.clone(), |s: String| html::to_json(&html::parse(&s)))?,
            )
        })();
        hooks.map_err(|e| format!("hooks: {e}"))?;
        let compiled = compile_classic(&ctx, STUB_DOM, "stub_dom.js")?;
        let ctl = run_compiled(&ctx, strip(&ctx, compiled, true)?.0)
            .map_err(|e| format!("stub DOM: {e}"))?
            .into_object()
            .ok_or("stub DOM returned no control object")?;
        ctx.run_gc();
        Ok(Persistent::save(&ctx, ctl))
    });
    let saved = match saved {
        Ok(p) => p,
        Err(e) => {
            out.errors.push(format!("aborted: {e}"));
            out.peak = counter.peak();
            return out;
        }
    };
    out.baseline = counter.current();
    out.stub_peak = counter.peak();
    // Page peaks start here, so the stub's own compile peak does not count.
    counter.reset_peak();
    out.stub_qjs = (rt.memory_usage().malloc_size - m_context) as usize;
    if let Some(c) = cap {
        rt.set_memory_limit(c + out.stub_qjs);
    }
    out.phases.push("stub");

    ctx.with(|ctx| {
        let res: Result<(), String> = (|| {
            let ctl: Object = saved.restore(&ctx).map_err(|e| e.to_string())?;
            let call = |name: &str| -> Result<Function, String> {
                ctl.get::<_, Function>(name).map_err(|e| e.to_string())
            };

            let list: Vec<Object> = call("load")?
                .call((html_src.clone(), item.fixtures.clone()))
                .map_err(|_| format!("load: {}", exception_text(&ctx)))?;
            let scripts: Vec<(Option<String>, String, String)> = list
                .iter()
                .map(|o| {
                    (
                        o.get("src").unwrap_or(None),
                        o.get("text").unwrap_or_default(),
                        o.get("type").unwrap_or_default(),
                    )
                })
                .collect();
            out.phases.push("dom");

            for (i, (src, inline, ty)) in scripts.iter().enumerate() {
                let module = ty == "module";
                if !(ty.is_empty()
                    || module
                    || ty.contains("javascript")
                    || ty.contains("ecmascript"))
                {
                    continue;
                }
                let (name, source) = match src {
                    Some(s) => {
                        let path = root.join(s.trim_start_matches('/'));
                        match std::fs::read_to_string(&path) {
                            Ok(t) => (normalize(&path).to_string_lossy().replace('\\', "/"), t),
                            Err(e) => {
                                errors.borrow_mut().push(format!(
                                    "script {s}: cannot read {}: {e}",
                                    path.display()
                                ));
                                continue;
                            }
                        }
                    }
                    None => (format!("inline-{i}"), inline.clone()),
                };
                let _: () = call("setCurrentScript")?
                    .call((i as i32,))
                    .map_err(|e| e.to_string())?;
                let mut stat = ScriptStat {
                    name: name.clone(),
                    bytes: source.len(),
                    module,
                    ..Default::default()
                };
                if module {
                    let t = Instant::now();
                    let declared = Module::declare(ctx.clone(), name.clone(), source.clone());
                    stat.compile = t.elapsed();
                    if let Ok(m) = &declared {
                        set_meta_url(m, &name, &root, &fixture_origin(&item.fixtures));
                    }
                    let t = Instant::now();
                    match declared.and_then(|m| m.eval()) {
                        Ok((_, promise)) => {
                            drain_jobs(&ctx);
                            if promise.state() == PromiseState::Rejected {
                                let reason: Value = promise
                                    .result::<Value>()
                                    .and_then(|r| r.err())
                                    .map(|_| ctx.catch())
                                    .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
                                errors
                                    .borrow_mut()
                                    .push(format!("module {name}: {}", value_text(&reason)));
                            }
                        }
                        Err(_) => errors
                            .borrow_mut()
                            .push(format!("module {name}: {}", exception_text(&ctx))),
                    }
                    stat.run = t.elapsed();
                } else {
                    let t = Instant::now();
                    let compiled = compile_classic(&ctx, &source, &name).and_then(|c| {
                        stat.compile = t.elapsed();
                        if strip_pages {
                            let (c, bytes, load) = strip(&ctx, c, false)?;
                            stat.bytecode = bytes;
                            stat.load = load;
                            Ok(c)
                        } else {
                            Ok(c)
                        }
                    });
                    match compiled {
                        Ok(f) => {
                            let t = Instant::now();
                            if let Err(e) = run_compiled(&ctx, f).map(drop) {
                                errors.borrow_mut().push(format!("script {name}: {e}"));
                            }
                            drain_jobs(&ctx);
                            stat.run = t.elapsed();
                        }
                        Err(e) => {
                            stat.compile = t.elapsed();
                            errors.borrow_mut().push(format!("compile {name}: {e}"));
                        }
                    }
                }
                out.scripts.push(stat);
            }
            let _: () = call("setCurrentScript")?
                .call((-1,))
                .map_err(|e| e.to_string())?;
            out.peak_scripts = counter.peak();
            out.phases.push("scripts");

            let t = Instant::now();
            let _: () = call("domReady")?
                .call(())
                .map_err(|_| format!("domReady: {}", exception_text(&ctx)))?;
            drain_jobs(&ctx);
            out.phases.push("load-events");

            let step = call("step")?;
            let now = call("now")?;
            let mut tasks = 0usize;
            let mut run_loop = |for_ms: f64| -> Result<(), String> {
                let start: f64 = now.call(()).map_err(|e| e.to_string())?;
                while tasks < MAX_TASKS {
                    let ran: bool = step
                        .call((start + for_ms,))
                        .map_err(|_| format!("event loop: {}", exception_text(&ctx)))?;
                    if !ran {
                        break;
                    }
                    tasks += 1;
                    drain_jobs(&ctx);
                }
                Ok(())
            };
            run_loop(LOAD_LOOP_MS)?;
            out.phases.push("event-loop");

            let targets: usize = call("collectTargets")?
                .call(())
                .map_err(|e| e.to_string())?;
            let interact = call("interact")?;
            for i in 0..targets.min(MAX_INTERACTIONS) {
                let _: () = interact
                    .call((i,))
                    .map_err(|_| format!("interact: {}", exception_text(&ctx)))?;
                drain_jobs(&ctx);
                run_loop(INTERACT_LOOP_MS)?;
                out.interactions += 1;
            }
            out.events_time = t.elapsed();
            out.phases.push("interactions");

            let summary: Object = call("summary")?.call(()).map_err(|e| e.to_string())?;
            out.elements = summary.get("elements").unwrap_or(0);
            out.tasks_run = summary.get("tasksRun").unwrap_or(0);
            out.net = summary.get("net").unwrap_or_default();
            out.body_text = summary.get("bodyText").unwrap_or_default();
            Ok(())
        })();
        if let Err(e) = res {
            errors.borrow_mut().push(format!("aborted: {e}"));
        }
        ctx.run_gc();
    });
    out.peak = counter.peak();
    out.end = counter.current();
    let mu = rt.memory_usage();
    out.qjs_malloc_size = mu.malloc_size;
    out.qjs_obj_count = mu.obj_count;
    let ms = mstats.borrow();
    out.modules_loaded = ms.count;
    out.module_bytes = ms.bytes;
    if ms.count > 0 {
        out.scripts.push(ScriptStat {
            name: format!("{} imported modules", ms.count),
            bytes: 0,
            module: true,
            compile: ms.compile,
            run: Duration::ZERO,
            ..Default::default()
        });
    }
    out.errors.extend(errors.borrow().iter().cloned());
    out.apis = apis.borrow().clone();
    out
}

/// Runs an empty page, for the runtime + context + stub DOM baseline.
pub fn run_empty() -> Outcome {
    let dir = std::env::temp_dir().join("js-spike-empty");
    let _ = std::fs::create_dir_all(&dir);
    let page = dir.join("index.html");
    let _ = std::fs::write(
        &page,
        "<!doctype html><html><head></head><body></body></html>",
    );
    run(
        &Item {
            name: "empty".into(),
            page,
            fixtures: "{}".into(),
        },
        None,
        false,
    )
}
