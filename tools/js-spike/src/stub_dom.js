// Logging stub DOM for the JS host spike, evaluated once per runtime before the page.
//
// Rust passes three hooks, which this script removes from the global object:
//   __eb_log(path)   records a web API path the first time it is touched
//   __eb_err(msg)    records an exception thrown by a callback
//   __eb_parse(html) parses an HTML fragment into a JSON tree
// and gets back a control object (see `ctl` at the end) that drives the page phases.
//
// Logged paths look like "Node.appendChild", "Element.attachShadow", "fetch",
// "Element.animate (stub)" or "window.foo (missing)". Members of DOM objects that we do not
// model return undefined, because pages probe expandos and feature-detect; members of
// wholly stubbed APIs (IntersectionObserver, canvas contexts, ...) return further stubs.
(() => {
  'use strict';
  const G = globalThis;
  const rlog = G.__eb_log, rerr = G.__eb_err, rparse = G.__eb_parse;
  delete G.__eb_log; delete G.__eb_err; delete G.__eb_parse;

  const seen = Object.create(null);
  const log = (p) => { if (seen[p] !== 1) { seen[p] = 1; rlog(p); } };
  const report = (where, e) => {
    let msg;
    try { msg = (e && e.stack) ? `${e}\n${e.stack}` : String(e); } catch (_) { msg = '<unprintable>'; }
    rerr(`${where}: ${msg}`);
  };

  const ST = Symbol('state');
  const CLS = Symbol('iface');
  const hasOwn = (o, k) => Object.prototype.hasOwnProperty.call(o, k);

  // ---------------------------------------------------------------------------------------
  // Generic stubs: callable, constructible, every member is a further stub.
  const stubCache = new Map();
  function stub(path) {
    if (path.length > 160) path = path.slice(0, 160) + '...';
    let s = stubCache.get(path);
    if (s) return s;
    s = new Proxy(function () {}, {
      get(_, k) {
        if (k === Symbol.toPrimitive) return (hint) => (hint === 'number' ? 0 : '');
        if (k === Symbol.iterator) return function* () {};
        if (typeof k === 'symbol' || k === 'then' || k === 'toJSON') return undefined;
        if (k === 'length') return 0;
        if (k === 'toString' || k === 'valueOf') return () => '';
        const p = `${path}.${k}`;
        log(p);
        return stub(p);
      },
      set(_, k) { if (typeof k === 'string') log(`${path}.${k} (set)`); return true; },
      apply() { const p = `${path}()`; log(p); return stub(p); },
      construct() { const p = `new ${path}`; log(p); return stub(p); },
    });
    stubCache.set(path, s);
    return s;
  }
  const stubFn = (path) => function () { log(path); return stub(`${path}()`); };

  // ---------------------------------------------------------------------------------------
  // Logged interfaces: objects are Proxies over raw objects whose prototype chain carries
  // the interface name, so a member read logs "<owning interface>.<member>".
  function ifaceOf(t) {
    for (let p = Object.getPrototypeOf(t); p; p = Object.getPrototypeOf(p)) {
      if (hasOwn(p, CLS)) return p[CLS];
    }
    return 'Object';
  }
  function owner(t, k) {
    for (let p = t; p; p = Object.getPrototypeOf(p)) {
      if (hasOwn(p, k)) {
        if (p === t) return null; // expando
        return hasOwn(p, CLS) ? p[CLS] : null; // user subclass members are not web APIs
      }
    }
    return null;
  }
  // Web APIs we do not model but that pages call: logged, return stubs.
  const STUB_MEMBERS = new Set([
    'animate', 'getAnimations', 'requestFullscreen', 'requestPointerLock', 'scroll', 'scrollTo',
    'scrollBy', 'showModal', 'show', 'showPopover', 'hidePopover', 'togglePopover',
    'setPointerCapture', 'releasePointerCapture', 'hasPointerCapture', 'getClientRects',
    'checkValidity', 'reportValidity', 'setCustomValidity', 'setSelectionRange', 'select',
    'play', 'pause', 'load', 'toDataURL', 'toBlob', 'computedStyleMap', 'checkVisibility',
    'getAttributeNode', 'setAttributeNode', 'getAttributeNS', 'setAttributeNS', 'removeAttributeNS',
    'hasAttributeNS', 'getElementsByTagNameNS', 'webkitMatchesSelector', 'msMatchesSelector',
    'mozMatchesSelector', 'oMatchesSelector', 'attachInternals', 'moveBefore', 'normalize',
  ]);
  const handler = {
    get(t, k) {
      if (typeof k === 'symbol') return Reflect.get(t, k, t);
      if (k in t) {
        const o = owner(t, k);
        if (o) log(`${o}.${k}`);
        return Reflect.get(t, k, t);
      }
      if (STUB_MEMBERS.has(k)) {
        const p = `${ifaceOf(t)}.${k} (stub)`;
        log(p);
        return stubFn(p);
      }
      if (/^(on|webkit|moz|ms)/.test(k) || k === 'then' || k === 'toJSON') return undefined; // probes
      if (/^[a-z][A-Za-z0-9]*$/.test(k)) log(`${ifaceOf(t)}.${k} (missing)`);
      return undefined;
    },
    set(t, k, v) {
      if (typeof k === 'string' && k in t) {
        const o = owner(t, k);
        if (o) log(`${o}.${k} (set)`);
      }
      return Reflect.set(t, k, v, t);
    },
  };
  function wrap(proto, state) {
    const raw = Object.create(proto);
    raw[ST] = state;
    const pub = new Proxy(raw, handler);
    state.pub = pub;
    state.raw = raw;
    return pub;
  }
  const S = (o) => (o == null ? undefined : o[ST]);

  // Object spread would call a mixin's getters, so mixins are spread as `...M(mixin)`: a
  // symbol key that define() expands into the mixin's property descriptors.
  const MIX = new Map();
  const M = (obj) => { const k = Symbol('mixin'); MIX.set(k, obj); return { [k]: obj }; };
  function define(o, props) {
    for (const k of Reflect.ownKeys(props)) {
      if (MIX.has(k)) { define(o, MIX.get(k)); continue; }
      const d = Object.getOwnPropertyDescriptor(props, k);
      d.enumerable = false;
      Object.defineProperty(o, k, d);
    }
  }
  function setGlobal(name, value) {
    Object.defineProperty(G, name, { value, writable: true, configurable: true, enumerable: false });
  }
  const illegal = function () { throw new TypeError('Illegal constructor'); };
  let internal = 0; // >0 while the stub itself constructs objects, so `new X` is not logged
  const make = (C, ...a) => { internal++; try { return new C(...a); } finally { internal--; } };
  function iface(name, parent, props, ctor, statics) {
    const C = ctor
      ? function (...a) {
        if (!new.target) throw new TypeError(`Constructor ${name} requires 'new'`);
        if (!internal) log(`new ${name}`);
        return Reflect.construct(ctor, a, new.target);
      }
      : function () { return illegal(); };
    Object.defineProperty(C, 'name', { value: name });
    C.prototype = Object.create(parent ? parent.prototype : Object.prototype);
    Object.defineProperty(C.prototype, 'constructor', { value: C, writable: true, configurable: true });
    Object.defineProperty(C.prototype, CLS, { value: name });
    if (parent) Object.setPrototypeOf(C, parent);
    if (props) define(C.prototype, props);
    if (statics) define(C, statics);
    setGlobal(name, C);
    return C;
  }
  const list = (arr) => {
    Object.defineProperty(arr, 'item', { value: (i) => (arr[i] === undefined ? null : arr[i]) });
    return arr;
  };

  // ---------------------------------------------------------------------------------------
  // Event loop on a virtual clock. Rust calls ctl.step() and drains microtasks in between.
  let now = 0, seq = 0, timerSeq = 0, tasksRun = 0;
  const tasks = [];
  function schedule(delay, fn, interval) {
    const d = Math.max(0, Number(delay) || 0);
    const task = { t: now + d, s: seq++, fn, id: ++timerSeq, iv: interval ? Math.max(4, d) : 0 };
    let i = tasks.length;
    while (i > 0 && (tasks[i - 1].t > task.t)) i--;
    tasks.splice(i, 0, task);
    return task.id;
  }
  function cancel(id) {
    const i = tasks.findIndex((t) => t.id === id);
    if (i >= 0) tasks.splice(i, 1);
  }
  function callbackOf(fn, what) {
    if (typeof fn === 'function') return fn;
    log(`${what}(string)`);
    return new Function(String(fn));
  }

  // ---------------------------------------------------------------------------------------
  // Nodes. State: { t: nodeType, name, attrs: [[k, v]], kids: [pub], parent, doc, ... }.
  const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta',
    'source', 'track', 'wbr']);
  const kids = (n) => S(n).kids;
  const isEl = (n) => n != null && S(n) !== undefined && S(n).t === 1;
  function elementKids(n) { return kids(n).filter(isEl); }
  function attrGet(s, k) {
    k = String(k).toLowerCase();
    for (const a of s.attrs) if (a[0] === k) return a[1];
    return null;
  }
  function attrSet(s, k, v) {
    k = String(k).toLowerCase();
    v = String(v);
    let old = null;
    const a = s.attrs.find((x) => x[0] === k);
    if (a) { old = a[1]; a[1] = v; } else s.attrs.push([k, v]);
    if (k === 'style' && s.style) s.style = null;
    attributeChanged(s, k, old, v);
  }
  function attrRemove(s, k) {
    k = String(k).toLowerCase();
    const i = s.attrs.findIndex((x) => x[0] === k);
    if (i >= 0) { const old = s.attrs[i][1]; s.attrs.splice(i, 1); attributeChanged(s, k, old, null); }
  }
  function attributeChanged(s, k, old, v) {
    const def = s.ce;
    if (def && def.observed.has(k) && typeof s.raw.attributeChangedCallback === 'function') {
      try { s.raw.attributeChangedCallback.call(s.pub, k, old, v); } catch (e) { report('attributeChangedCallback', e); }
    }
  }
  function connected(n) {
    for (let p = n; p; ) {
      const s = S(p);
      if (s.t === 9) return true;
      p = s.parent || (s.host || null);
    }
    return false;
  }
  function root(n) { let p = n; while (S(p).parent) p = S(p).parent; return p; }
  function descendants(n, out) {
    for (const k of kids(n)) { out.push(k); descendants(k, out); }
    return out;
  }
  function ceReaction(n, cb) {
    const all = [n, ...descendants(n, [])];
    for (const x of all) {
      const s = S(x);
      if (s.t === 1 && s.ce && typeof s.raw[cb] === 'function') {
        try { s.raw[cb].call(x); } catch (e) { report(cb, e); }
      }
    }
  }
  function detach(n) {
    const s = S(n);
    if (!s.parent) return;
    const was = connected(n);
    const k = kids(s.parent);
    k.splice(k.indexOf(n), 1);
    s.parent = null;
    if (was) ceReaction(n, 'disconnectedCallback');
  }
  function insert(parent, n, before) {
    if (n == null || S(n) === undefined) throw new TypeError('parameter is not of type Node');
    const ps = S(parent);
    if (S(n).t === 11) { // fragment: move its children
      for (const c of kids(n).slice()) insert(parent, c, before);
      return n;
    }
    for (let p = parent; p; p = S(p).parent) if (p === n) throw new Error('HierarchyRequestError');
    detach(n);
    const k = ps.kids;
    const i = before == null ? k.length : k.indexOf(before);
    k.splice(i < 0 ? k.length : i, 0, n);
    S(n).parent = parent;
    adopt(n, ps.doc || parent);
    if (connected(n)) ceReaction(n, 'connectedCallback');
    return n;
  }
  function adopt(n, doc) {
    const s = S(n);
    s.doc = doc;
    for (const k of s.kids) adopt(k, doc);
  }
  function text(n) {
    const s = S(n);
    if (s.t === 3 || s.t === 8) return s.data;
    let out = '';
    for (const k of s.kids) if (S(k).t !== 8) out += text(k);
    return out;
  }
  function setChildrenText(n, v) {
    for (const k of kids(n).slice()) detach(k);
    if (v !== '') insert(n, newText(S(n).doc, v));
  }
  const esc = (v, attr) => String(v).replace(attr ? /[&"]/g : /[&<>]/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  function serialize(n) {
    const s = S(n);
    if (s.t === 3) {
      const p = s.parent && S(s.parent);
      return p && (p.name === 'SCRIPT' || p.name === 'STYLE') ? s.data : esc(s.data, false);
    }
    if (s.t === 8) return `<!--${s.data}-->`;
    if (s.t !== 1) return s.kids.map(serialize).join('');
    const tag = s.name.toLowerCase();
    const at = s.attrs.map((a) => ` ${a[0]}="${esc(a[1], true)}"`).join('');
    if (VOID.has(tag)) return `<${tag}${at}>`;
    const inner = (s.content ? kids(s.content) : s.kids).map(serialize).join('');
    return `<${tag}${at}>${inner}</${tag}>`;
  }
  function parseInto(doc, html, parent, before) {
    const tree = JSON.parse(rparse(String(html)));
    for (const t of tree) insert(parent, build(doc, t), before);
  }
  function build(doc, t) {
    if (typeof t === 'string') return newText(doc, t);
    if (t[0] === '#comment') return newComment(doc, t[1]);
    const el = newElement(doc, t[0]);
    const s = S(el);
    for (const k of Object.keys(t[1])) s.attrs.push([k, t[1][k]]);
    const target = s.content || el;
    for (const c of t[2]) insert(target, build(doc, c));
    return el;
  }

  function newState(t, name, doc) {
    return { t, name, attrs: [], kids: [], parent: null, doc, ls: null, h: null };
  }
  function newText(doc, data) { const s = newState(3, '#text', doc); s.data = String(data); return wrap(Text.prototype, s); }
  function newComment(doc, data) { const s = newState(8, '#comment', doc); s.data = String(data); return wrap(Comment.prototype, s); }
  function newFragment(doc) { return wrap(DocumentFragment.prototype, newState(11, '#document-fragment', doc)); }

  // ---------------------------------------------------------------------------------------
  // Events.
  const HANDLER_EVENTS = ['click', 'dblclick', 'input', 'change', 'submit', 'reset', 'load', 'error',
    'keydown', 'keyup', 'keypress', 'focus', 'blur', 'mousedown', 'mouseup', 'mousemove', 'mouseover',
    'mouseout', 'mouseenter', 'mouseleave', 'touchstart', 'touchend', 'touchmove', 'pointerdown',
    'pointerup', 'pointermove', 'scroll', 'resize', 'contextmenu', 'wheel', 'abort', 'readystatechange',
    'loadend', 'loadstart', 'progress', 'timeout', 'open', 'message', 'close', 'popstate', 'hashchange',
    'beforeunload', 'unload', 'visibilitychange', 'animationend', 'transitionend'];
  function listenersOf(s, type) {
    if (!s.ls) s.ls = new Map();
    let a = s.ls.get(type);
    if (!a) { a = []; s.ls.set(type, a); }
    return a;
  }
  function handlerProps(names) {
    const props = {};
    for (const ev of names) {
      Object.defineProperty(props, `on${ev}`, {
        get() { const s = this[ST]; return (s.h && s.h[ev]) || null; },
        set(v) { const s = this[ST]; (s.h || (s.h = {}))[ev] = typeof v === 'function' ? v : null; },
        enumerable: true, configurable: true,
      });
    }
    return props;
  }
  function dispatch(target, ev) {
    const es = S(ev);
    es.target = target;
    const path = [];
    for (let p = target; p; ) {
      path.push(p);
      if (p === win) break;
      const s = S(p);
      p = s.parent || s.host || (s.t === 9 && s === S(document) ? win : null);
    }
    const fire = (node, phase, capture) => {
      const s = node === win ? WIN : S(node);
      es.currentTarget = node;
      es.phase = phase;
      if (s.ls && s.ls.has(es.type)) {
        for (const l of s.ls.get(es.type).slice()) {
          if (es.stopImm) break;
          if (phase !== 2 && l.capture !== capture) continue;
          if (l.once) { const a = s.ls.get(es.type); a.splice(a.indexOf(l), 1); }
          try {
            if (typeof l.fn === 'function') l.fn.call(node, ev); else l.fn.handleEvent(ev);
          } catch (e) { report(`listener ${es.type}`, e); }
        }
      }
      if (capture && phase !== 2) return;
      if (s.h && s.h[es.type]) {
        try { if (s.h[es.type].call(node, ev) === false) es.prevented = true; } catch (e) { report(`on${es.type}`, e); }
      } else if (s.t === 1 && !es.stopImm) {
        const src = attrGet(s, `on${es.type}`);
        if (src != null) {
          log(`inline on${es.type} attribute`);
          try {
            // Inline handlers see the element's members as variables, as in browsers.
            const f = (s.inl || (s.inl = {}))[es.type] || (s.inl[es.type] = new Function('event', `with (this) { ${src}
}`));
            if (f.call(node, ev) === false) es.prevented = true;
          } catch (e) { report(`inline on${es.type}`, e); }
        }
      }
    };
    for (let i = path.length - 1; i > 0 && !es.stop; i--) fire(path[i], 1, true);
    if (!es.stop) fire(path[0], 2, true);
    if (es.bubbles) for (let i = 1; i < path.length && !es.stop; i++) fire(path[i], 3, false);
    es.currentTarget = null;
    es.phase = 0;
    if (!es.prevented) defaultAction(target, es);
    return !es.prevented;
  }
  function defaultAction(target, es) {
    const s = S(target);
    if (target === win || !s || s.t !== 1 || es.type !== 'click') return;
    for (let p = target; p; p = S(p).parent) {
      const ps = S(p);
      if (ps.t !== 1) break;
      if (ps.name === 'A' && attrGet(ps, 'href') != null) { log('navigation (link default action)'); return; }
      if ((ps.name === 'BUTTON' && (attrGet(ps, 'type') || 'submit') === 'submit') ||
          (ps.name === 'INPUT' && attrGet(ps, 'type') === 'submit')) {
        const form = closestState(p, 'FORM');
        if (form) {
          const ev = makeEvent(SubmitEvent, 'submit', { bubbles: true, cancelable: true, submitter: p });
          if (dispatch(form, ev)) log('form submission (default action)');
        }
        return;
      }
    }
  }
  function closestState(n, tag) {
    for (let p = n; p; p = S(p).parent) if (S(p).name === tag) return p;
    return null;
  }

  const EventTarget = iface('EventTarget', null, {
    addEventListener(type, fn, opts) {
      if (fn == null) return;
      const s = this === win ? WIN : this[ST];
      const capture = typeof opts === 'boolean' ? opts : !!(opts && opts.capture);
      const a = listenersOf(s, String(type));
      if (a.some((l) => l.fn === fn && l.capture === capture)) return;
      const l = { fn, capture, once: !!(opts && opts.once) };
      a.push(l);
      if (opts && opts.signal) {
        log('addEventListener signal option');
        opts.signal.addEventListener('abort', () => { const i = a.indexOf(l); if (i >= 0) a.splice(i, 1); });
      }
    },
    removeEventListener(type, fn, opts) {
      const s = this === win ? WIN : this[ST];
      if (!s.ls || !s.ls.has(type)) return;
      const capture = typeof opts === 'boolean' ? opts : !!(opts && opts.capture);
      const a = s.ls.get(type);
      const i = a.findIndex((l) => l.fn === fn && l.capture === capture);
      if (i >= 0) a.splice(i, 1);
    },
    dispatchEvent(ev) { return dispatch(this, ev); },
  }, function EventTarget() { return wrap(new.target.prototype, newState(0, '', null)); });

  const EVENT_FIELDS = ['detail', 'data', 'origin', 'lastEventId', 'source', 'ports', 'code', 'reason',
    'wasClean', 'key', 'keyCode', 'charCode', 'which', 'ctrlKey', 'shiftKey', 'altKey', 'metaKey',
    'repeat', 'button', 'buttons', 'clientX', 'clientY', 'pageX', 'pageY', 'screenX', 'screenY',
    'offsetX', 'offsetY', 'movementX', 'movementY', 'relatedTarget', 'pointerId', 'pointerType',
    'isPrimary', 'pressure', 'touches', 'changedTouches', 'targetTouches', 'deltaX', 'deltaY',
    'deltaMode', 'inputType', 'isComposing', 'submitter', 'state', 'oldURL', 'newURL', 'loaded',
    'total', 'lengthComputable', 'message', 'filename', 'lineno', 'colno', 'error', 'view'];
  const eventProps = {
    get type() { return this[ST].type; },
    get target() { return this[ST].target || null; },
    get srcElement() { return this[ST].target || null; },
    get currentTarget() { return this[ST].currentTarget || null; },
    get bubbles() { return this[ST].bubbles; },
    get cancelable() { return this[ST].cancelable; },
    get composed() { return this[ST].composed; },
    get defaultPrevented() { return this[ST].prevented; },
    get returnValue() { return !this[ST].prevented; },
    get eventPhase() { return this[ST].phase; },
    get timeStamp() { return this[ST].ts; },
    get isTrusted() { return this[ST].trusted; },
    get cancelBubble() { return this[ST].stop; },
    set cancelBubble(v) { if (v) this[ST].stop = true; },
    preventDefault() { if (this[ST].cancelable) this[ST].prevented = true; },
    stopPropagation() { this[ST].stop = true; },
    stopImmediatePropagation() { this[ST].stop = true; this[ST].stopImm = true; },
    composedPath() { const out = []; for (let p = this[ST].target; p; p = S(p) && S(p).parent) out.push(p); return out; },
    initEvent(type, bubbles, cancelable) { Object.assign(this[ST], { type: String(type), bubbles: !!bubbles, cancelable: !!cancelable }); },
    initCustomEvent(type, bubbles, cancelable, detail) {
      Object.assign(this[ST], { type: String(type), bubbles: !!bubbles, cancelable: !!cancelable });
      this[ST].init.detail = detail;
    },
  };
  for (const f of EVENT_FIELDS) {
    Object.defineProperty(eventProps, f, {
      get() { const v = this[ST].init[f]; return v === undefined ? (f === 'detail' || f === 'data' || f === 'relatedTarget' || f === 'submitter' || f === 'state' || f === 'error' || f === 'view' ? null : (/Key$|^repeat$|^wasClean$|^isPrimary$|^isComposing$|^lengthComputable$/.test(f) ? false : (/^(key|code|origin|lastEventId|reason|pointerType|inputType|oldURL|newURL|message|filename)$/.test(f) ? '' : (/^(touches|changedTouches|targetTouches|ports)$/.test(f) ? [] : 0)))) : v; },
      enumerable: true, configurable: true,
    });
  }
  function eventCtor(type, init) {
    if (!new.target) throw new TypeError("Constructor requires 'new'");
    if (arguments.length === 0) throw new TypeError('Event type required');
    init = init || {};
    return wrap(new.target.prototype, {
      type: String(type), bubbles: !!init.bubbles, cancelable: !!init.cancelable, composed: !!init.composed,
      prevented: false, stop: false, stopImm: false, phase: 0, ts: now, trusted: false, init,
    });
  }
  const Event = iface('Event', null, eventProps, eventCtor, { NONE: 0, CAPTURING_PHASE: 1, AT_TARGET: 2, BUBBLING_PHASE: 3 });
  const evSub = (name, parent) => iface(name, parent || Event, {}, function (type, init) {
    return Reflect.construct(eventCtor, [type, init], new.target);
  });
  const CustomEvent = evSub('CustomEvent');
  const UIEvent = evSub('UIEvent');
  const MouseEvent = evSub('MouseEvent', UIEvent);
  evSub('PointerEvent', MouseEvent);
  evSub('WheelEvent', MouseEvent);
  evSub('KeyboardEvent', UIEvent);
  evSub('FocusEvent', UIEvent);
  evSub('InputEvent', UIEvent);
  evSub('TouchEvent', UIEvent);
  const MessageEvent = evSub('MessageEvent');
  const CloseEvent = evSub('CloseEvent');
  const ProgressEvent = evSub('ProgressEvent');
  evSub('ErrorEvent');
  evSub('PopStateEvent');
  evSub('HashChangeEvent');
  evSub('AnimationEvent');
  evSub('TransitionEvent');
  const SubmitEvent = evSub('SubmitEvent');
  function makeEvent(C, type, init) { const e = make(C, type, init); S(e).trusted = true; return e; }

  // ---------------------------------------------------------------------------------------
  // Node interfaces.
  const Node = iface('Node', EventTarget, {
    get nodeType() { return this[ST].t; },
    get nodeName() { return this[ST].name; },
    get ownerDocument() { const s = this[ST]; return s.t === 9 ? null : s.doc; },
    get parentNode() { return this[ST].parent; },
    get parentElement() { const p = this[ST].parent; return isEl(p) ? p : null; },
    get childNodes() { return list(this[ST].kids.slice()); },
    get firstChild() { return this[ST].kids[0] || null; },
    get lastChild() { const k = this[ST].kids; return k[k.length - 1] || null; },
    get nextSibling() { const p = this[ST].parent; if (!p) return null; const k = kids(p); return k[k.indexOf(this[ST].pub) + 1] || null; },
    get previousSibling() { const p = this[ST].parent; if (!p) return null; const k = kids(p); return k[k.indexOf(this[ST].pub) - 1] || null; },
    get isConnected() { return connected(this[ST].pub); },
    get textContent() { const s = this[ST]; return s.t === 9 ? null : text(s.pub); },
    set textContent(v) { const s = this[ST]; if (s.t === 3 || s.t === 8) s.data = String(v); else setChildrenText(s.pub, v == null ? '' : String(v)); },
    get nodeValue() { const s = this[ST]; return s.t === 3 || s.t === 8 ? s.data : null; },
    set nodeValue(v) { const s = this[ST]; if (s.t === 3 || s.t === 8) s.data = String(v); },
    hasChildNodes() { return this[ST].kids.length > 0; },
    appendChild(n) { return insert(this[ST].pub, n, null); },
    insertBefore(n, ref) { return insert(this[ST].pub, n, ref); },
    removeChild(n) { if (S(n).parent !== this[ST].pub) throw new Error('NotFoundError'); detach(n); return n; },
    replaceChild(n, old) { insert(this[ST].pub, n, old); detach(old); return old; },
    contains(n) { for (let p = n; p; p = S(p).parent) if (p === this[ST].pub) return true; return false; },
    getRootNode() { return root(this[ST].pub); },
    isSameNode(n) { return n === this[ST].pub; },
    isEqualNode(n) { return n != null && serialize(n) === serialize(this[ST].pub); },
    compareDocumentPosition(n) {
      const me = this[ST].pub;
      if (n === me) return 0;
      if (S(me).raw.contains(n)) return 20;
      if (S(n).raw.contains(me)) return 10;
      const all = descendants(root(me), []);
      return all.indexOf(n) > all.indexOf(me) ? 4 : 2;
    },
    cloneNode(deep) { return clone(this[ST].pub, !!deep); },
  }, null, { ELEMENT_NODE: 1, ATTRIBUTE_NODE: 2, TEXT_NODE: 3, COMMENT_NODE: 8, DOCUMENT_NODE: 9, DOCUMENT_FRAGMENT_NODE: 11,
    DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4, DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16 });
    for (const [k, v] of Object.entries({ ELEMENT_NODE: 1, TEXT_NODE: 3, COMMENT_NODE: 8, DOCUMENT_NODE: 9, DOCUMENT_FRAGMENT_NODE: 11 })) {
    Object.defineProperty(Node.prototype, k, { value: v });
  }
  function clone(n, deep) {
    const s = S(n);
    let c;
    if (s.t === 3) c = newText(s.doc, s.data);
    else if (s.t === 8) c = newComment(s.doc, s.data);
    else if (s.t === 11) c = newFragment(s.doc);
    else if (s.t === 1) { c = newElement(s.doc, s.name.toLowerCase()); S(c).attrs = s.attrs.map((a) => a.slice()); }
    else throw new Error('NotSupportedError: cloning a document');
    if (deep) {
      for (const k of s.kids) insert(c, clone(k, true));
      if (s.content) for (const k of kids(s.content)) insert(S(c).content, clone(k, true));
    }
    return c;
  }

  // ParentNode / ChildNode mixins shared by Element, Document and DocumentFragment.
  const toNodes = (doc, args) => args.map((a) => (typeof a === 'string' ? newText(doc, a) : a));
  const parentNodeMixin = {
    get children() { return list(elementKids(this[ST].pub)); },
    get childElementCount() { return elementKids(this[ST].pub).length; },
    get firstElementChild() { return elementKids(this[ST].pub)[0] || null; },
    get lastElementChild() { const e = elementKids(this[ST].pub); return e[e.length - 1] || null; },
    querySelector(sel) { return querySelectorAll(this[ST].pub, sel, true)[0] || null; },
    querySelectorAll(sel) { return list(querySelectorAll(this[ST].pub, sel, false)); },
    getElementsByTagName(tag) { tag = String(tag).toUpperCase(); return list(descendants(this[ST].pub, []).filter((n) => isEl(n) && (tag === '*' || S(n).name === tag))); },
    getElementsByClassName(cls) { const want = String(cls).split(/\s+/).filter(Boolean); return list(descendants(this[ST].pub, []).filter((n) => isEl(n) && want.every((c) => classes(S(n)).includes(c)))); },
    append(...a) { for (const n of toNodes(this[ST].doc || this[ST].pub, a)) insert(this[ST].pub, n, null); },
    prepend(...a) { const first = this[ST].kids[0] || null; for (const n of toNodes(this[ST].doc || this[ST].pub, a)) insert(this[ST].pub, n, first); },
    replaceChildren(...a) { for (const k of this[ST].kids.slice()) detach(k); this.append(...a); },
  };
  const childNodeMixin = {
    remove() { detach(this[ST].pub); },
    before(...a) { const s = this[ST]; if (s.parent) for (const n of toNodes(s.doc, a)) insert(s.parent, n, s.pub); },
    after(...a) { const s = this[ST]; if (!s.parent) return; const k = kids(s.parent); const next = k[k.indexOf(s.pub) + 1] || null; for (const n of toNodes(s.doc, a)) insert(s.parent, n, next); },
    replaceWith(...a) { const s = this[ST]; if (!s.parent) return; const p = s.parent; const k = kids(p); const next = k[k.indexOf(s.pub) + 1] || null; detach(s.pub); for (const n of toNodes(s.doc, a)) insert(p, n, next); },
    get nextElementSibling() { const s = this[ST]; if (!s.parent) return null; const k = kids(s.parent); for (let i = k.indexOf(s.pub) + 1; i < k.length; i++) if (isEl(k[i])) return k[i]; return null; },
    get previousElementSibling() { const s = this[ST]; if (!s.parent) return null; const k = kids(s.parent); for (let i = k.indexOf(s.pub) - 1; i >= 0; i--) if (isEl(k[i])) return k[i]; return null; },
  };

  const CharacterData = iface('CharacterData', Node, {
    get data() { return this[ST].data; },
    set data(v) { this[ST].data = String(v); },
    get length() { return this[ST].data.length; },
    appendData(d) { this[ST].data += String(d); },
    ...M(childNodeMixin),
  });
  const Text = iface('Text', CharacterData, {
    get wholeText() { return this[ST].data; },
    splitText(off) { const s = this[ST]; const rest = s.data.slice(off); s.data = s.data.slice(0, off); const t = newText(s.doc, rest); if (s.parent) this.after(t); return t; },
  }, function Text(data) { return newText(document, data === undefined ? '' : data); });
  const Comment = iface('Comment', CharacterData, {}, function Comment(data) { return newComment(document, data === undefined ? '' : data); });

  const DocumentFragment = iface('DocumentFragment', Node, {
    ...M(parentNodeMixin),
    getElementById(id) { return descendants(this[ST].pub, []).find((n) => isEl(n) && attrGet(S(n), 'id') === String(id)) || null; },
  }, function DocumentFragment() { return newFragment(document); });
  const ShadowRoot = iface('ShadowRoot', DocumentFragment, {
    get host() { return this[ST].host; },
    get mode() { return this[ST].mode; },
    get innerHTML() { return this[ST].kids.map(serialize).join(''); },
    set innerHTML(v) { const s = this[ST]; for (const k of s.kids.slice()) detach(k); parseInto(s.doc, v, s.pub, null); },
    get adoptedStyleSheets() { return this[ST].sheets || (this[ST].sheets = []); },
    set adoptedStyleSheets(v) { this[ST].sheets = v; },
    get activeElement() { return null; },
  });

  // Classes and the DOMTokenList over the class attribute.
  const classes = (s) => (attrGet(s, 'class') || '').split(/\s+/).filter(Boolean);
  const DOMTokenList = iface('DOMTokenList', null, {
    get length() { return classes(this[ST].el).length; },
    get value() { return attrGet(this[ST].el, 'class') || ''; },
    item(i) { return classes(this[ST].el)[i] || null; },
    contains(c) { return classes(this[ST].el).includes(String(c)); },
    add(...cs) { const s = this[ST].el; const cur = classes(s); for (const c of cs) if (!cur.includes(c)) cur.push(String(c)); attrSet(s, 'class', cur.join(' ')); },
    remove(...cs) { const s = this[ST].el; attrSet(s, 'class', classes(s).filter((c) => !cs.includes(c)).join(' ')); },
    toggle(c, force) { const has = classes(this[ST].el).includes(String(c)); const want = force === undefined ? !has : !!force; const s = this[ST].el; if (want && !has) attrSet(s, 'class', [...classes(s), String(c)].join(' ')); if (!want && has) attrSet(s, 'class', classes(s).filter((x) => x !== String(c)).join(' ')); return want; },
    replace(a, b) { const s = this[ST].el; const cur = classes(s); const i = cur.indexOf(a); if (i < 0) return false; cur[i] = b; attrSet(s, 'class', cur.join(' ')); return true; },
    forEach(fn, t) { classes(this[ST].el).forEach(fn, t); },
    [Symbol.iterator]() { return classes(this[ST].el)[Symbol.iterator](); },
    toString() { return attrGet(this[ST].el, 'class') || ''; },
  });

  // Inline style: a CSSStyleDeclaration that records which properties pages set.
  function styleOf(s) {
    if (s.style) return s.style;
    const values = Object.create(null);
    const parse = (css) => { for (const d of String(css).split(';')) { const i = d.indexOf(':'); if (i > 0) values[d.slice(0, i).trim()] = d.slice(i + 1).trim(); } };
    parse(attrGet(s, 'style') || '');
    const kebab = (k) => k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
    const sync = () => { const i = s.attrs.findIndex((a) => a[0] === 'style'); const css = Object.keys(values).map((k) => `${k}: ${values[k]}`).join('; '); if (i >= 0) s.attrs[i][1] = css; else s.attrs.push(['style', css]); };
    const api = {
      setProperty(k, v) { log('CSSStyleDeclaration.setProperty'); values[k] = String(v); sync(); },
      getPropertyValue(k) { log('CSSStyleDeclaration.getPropertyValue'); return values[k] || ''; },
      removeProperty(k) { log('CSSStyleDeclaration.removeProperty'); const v = values[k] || ''; delete values[k]; sync(); return v; },
    };
    s.style = new Proxy(values, {
      get(_, k) {
        if (typeof k === 'symbol') return undefined;
        if (k in api) return api[k];
        if (k === 'cssText') { log('CSSStyleDeclaration.cssText'); return Object.keys(values).map((x) => `${x}: ${values[x]}`).join('; '); }
        if (k === 'length') return Object.keys(values).length;
        log(`CSSStyleDeclaration.${k}`);
        return values[kebab(k)] || '';
      },
      set(_, k, v) {
        if (typeof k === 'symbol') return true;
        if (k === 'cssText') { log('CSSStyleDeclaration.cssText (set)'); for (const x of Object.keys(values)) delete values[x]; parse(v); sync(); return true; }
        log(`CSSStyleDeclaration.${k} (set)`);
        values[kebab(k)] = String(v);
        sync();
        return true;
      },
    });
    return s.style;
  }
  function datasetOf(s) {
    const toAttr = (k) => `data-${String(k).replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
    return new Proxy({}, {
      get(_, k) { if (typeof k === 'symbol') return undefined; const v = attrGet(s, toAttr(k)); return v === null ? undefined : v; },
      set(_, k, v) { attrSet(s, toAttr(k), v); return true; },
      has(_, k) { return attrGet(s, toAttr(k)) !== null; },
      deleteProperty(_, k) { attrRemove(s, toAttr(k)); return true; },
      ownKeys() { return s.attrs.filter((a) => a[0].startsWith('data-')).map((a) => a[0].slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())); },
      getOwnPropertyDescriptor(_, k) { const v = attrGet(s, toAttr(k)); return v === null ? undefined : { value: v, enumerable: true, configurable: true, writable: true }; },
    });
  }

  const Element = iface('Element', Node, {
    get tagName() { return this[ST].name; },
    get localName() { return this[ST].name.toLowerCase(); },
    get namespaceURI() { return this[ST].ns || 'http://www.w3.org/1999/xhtml'; },
    get id() { return attrGet(this[ST], 'id') || ''; },
    set id(v) { attrSet(this[ST], 'id', v); },
    get className() { return attrGet(this[ST], 'class') || ''; },
    set className(v) { attrSet(this[ST], 'class', v); },
    get classList() { const s = this[ST]; return s.cl || (s.cl = wrap(DOMTokenList.prototype, { el: s })); },
    get attributes() { return list(this[ST].attrs.map((a) => ({ name: a[0], localName: a[0], value: a[1], nodeName: a[0], nodeValue: a[1], specified: true }))); },
    getAttribute(k) { return attrGet(this[ST], k); },
    setAttribute(k, v) { attrSet(this[ST], k, v); },
    removeAttribute(k) { attrRemove(this[ST], k); },
    hasAttribute(k) { return attrGet(this[ST], k) !== null; },
    hasAttributes() { return this[ST].attrs.length > 0; },
    getAttributeNames() { return this[ST].attrs.map((a) => a[0]); },
    toggleAttribute(k, force) { const has = attrGet(this[ST], k) !== null; const want = force === undefined ? !has : !!force; if (want && !has) attrSet(this[ST], k, ''); if (!want && has) attrRemove(this[ST], k); return want; },
    matches(sel) { return matches(this[ST].pub, sel); },
    closest(sel) { for (let p = this[ST].pub; p && isEl(p); p = S(p).parent) if (matches(p, sel)) return p; return null; },
    get innerHTML() { const s = this[ST]; return (s.content ? kids(s.content) : s.kids).map(serialize).join(''); },
    set innerHTML(v) { const s = this[ST]; const target = s.content || s.pub; for (const k of kids(target).slice()) detach(k); parseInto(s.doc, v, target, null); },
    get outerHTML() { return serialize(this[ST].pub); },
    set outerHTML(v) { const s = this[ST]; if (!s.parent) return; parseInto(s.doc, v, s.parent, s.pub); detach(s.pub); },
    insertAdjacentHTML(pos, html) {
      const s = this[ST];
      const p = String(pos).toLowerCase();
      if (p === 'beforebegin') parseInto(s.doc, html, s.parent, s.pub);
      else if (p === 'afterbegin') parseInto(s.doc, html, s.pub, s.kids[0] || null);
      else if (p === 'beforeend') parseInto(s.doc, html, s.pub, null);
      else if (p === 'afterend') { const k = kids(s.parent); parseInto(s.doc, html, s.parent, k[k.indexOf(s.pub) + 1] || null); }
    },
    insertAdjacentElement(pos, el) {
      const s = this[ST];
      const p = String(pos).toLowerCase();
      if (p === 'beforebegin') insert(s.parent, el, s.pub);
      else if (p === 'afterbegin') insert(s.pub, el, s.kids[0] || null);
      else if (p === 'beforeend') insert(s.pub, el, null);
      else if (p === 'afterend') { const k = kids(s.parent); insert(s.parent, el, k[k.indexOf(s.pub) + 1] || null); }
      return el;
    },
    insertAdjacentText(pos, t) { this.insertAdjacentElement(pos, newText(this[ST].doc, t)); },
    attachShadow(init) {
      const s = this[ST];
      if (s.shadow) throw new Error('NotSupportedError: shadow root already attached');
      const sr = wrap(ShadowRoot.prototype, newState(11, '#document-fragment', s.doc));
      S(sr).host = s.pub;
      S(sr).mode = (init && init.mode) || 'open';
      s.shadow = sr;
      return sr;
    },
    get shadowRoot() { const s = this[ST]; return s.shadow && S(s.shadow).mode === 'open' ? s.shadow : null; },
    get slot() { return attrGet(this[ST], 'slot') || ''; },
    get assignedSlot() { return null; },
    getBoundingClientRect() { return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; },
    scrollIntoView() {},
    get clientWidth() { return 0; }, get clientHeight() { return 0; },
    get clientTop() { return 0; }, get clientLeft() { return 0; },
    get scrollWidth() { return 0; }, get scrollHeight() { return 0; },
    get scrollTop() { return 0; }, set scrollTop(v) {},
    get scrollLeft() { return 0; }, set scrollLeft(v) {},
    ...M(parentNodeMixin),
    ...M(childNodeMixin),
  });

  const htmlElementProps = {
    get style() { return styleOf(this[ST]); },
    set style(v) { attrSet(this[ST], 'style', v); },
    get dataset() { const s = this[ST]; return s.ds || (s.ds = datasetOf(s)); },
    get hidden() { return attrGet(this[ST], 'hidden') !== null; },
    set hidden(v) { if (v) attrSet(this[ST], 'hidden', ''); else attrRemove(this[ST], 'hidden'); },
    get title() { return attrGet(this[ST], 'title') || ''; },
    set title(v) { attrSet(this[ST], 'title', v); },
    get lang() { return attrGet(this[ST], 'lang') || ''; },
    get dir() { return attrGet(this[ST], 'dir') || ''; },
    get tabIndex() { return Number(attrGet(this[ST], 'tabindex') || -1); },
    set tabIndex(v) { attrSet(this[ST], 'tabindex', v); },
    get innerText() { return text(this[ST].pub); },
    set innerText(v) { setChildrenText(this[ST].pub, v == null ? '' : String(v)); },
    get offsetWidth() { return 0; }, get offsetHeight() { return 0; },
    get offsetTop() { return 0; }, get offsetLeft() { return 0; },
    get offsetParent() { return null; },
    get isContentEditable() { return false; },
    get contentEditable() { return attrGet(this[ST], 'contenteditable') || 'inherit'; },
    get draggable() { return attrGet(this[ST], 'draggable') === 'true'; },
    focus() { activeElement = this[ST].pub; },
    blur() { if (activeElement === this[ST].pub) activeElement = null; },
    click() { dispatch(this[ST].pub, makeEvent(MouseEvent, 'click', { bubbles: true, cancelable: true })); },
    ...M(handlerProps(HANDLER_EVENTS)),
  };
  const HTMLElement = iface('HTMLElement', Element, htmlElementProps, function HTMLElement() {
    // Custom element constructor: upgrade a pending element or create a fresh one.
    if (!new.target || new.target === HTMLElement) throw new TypeError('Illegal constructor');
    const def = ceByClass.get(new.target);
    if (!def) throw new TypeError('custom element constructor not registered');
    if (upgrading) {
      const s = upgrading;
      upgrading = null;
      Object.setPrototypeOf(s.raw, new.target.prototype);
      return s.pub;
    }
    const s = newState(1, def.name.toUpperCase(), document);
    s.ce = def;
    return wrap(new.target.prototype, s);
  });

  const reflect = (attr) => ({
    get() { return attrGet(this[ST], attr) || ''; },
    set(v) { attrSet(this[ST], attr, v); },
    enumerable: true, configurable: true,
  });
  const reflectBool = (attr) => ({
    get() { return attrGet(this[ST], attr) !== null; },
    set(v) { if (v) attrSet(this[ST], attr, ''); else attrRemove(this[ST], attr); },
    enumerable: true, configurable: true,
  });
  function props(spec) {
    const out = {};
    for (const k of Reflect.ownKeys(spec)) {
      if (MIX.has(k)) { define(out, MIX.get(k)); continue; }
      const d = Object.getOwnPropertyDescriptor(spec, k);
      if (!('value' in d)) { Object.defineProperty(out, k, d); continue; }
      const v = d.value;
      if (typeof v === 'string') Object.defineProperty(out, k, v.startsWith('?') ? reflectBool(v.slice(1)) : reflect(v));
      else Object.defineProperty(out, k, Object.getOwnPropertyDescriptor(spec, k));
    }
    return out;
  }
  const formOf = (s) => closestState(s.pub, 'FORM');
  const valueProps = {
    get value() { const s = this[ST]; return s.value !== undefined ? s.value : (s.name === 'TEXTAREA' ? text(s.pub) : (attrGet(s, 'value') || (attrGet(s, 'type') === 'checkbox' ? 'on' : ''))); },
    set value(v) { this[ST].value = String(v); },
    get defaultValue() { return attrGet(this[ST], 'value') || ''; },
    get form() { return formOf(this[ST]); },
    get validity() { return { valid: true }; },
    get labels() { return list([]); },
  };
  const tagClasses = {};
  const htmlSub = (name, tags, spec) => {
    const C = iface(name, HTMLElement, props(spec || {}));
    for (const t of tags) tagClasses[t] = C;
    return C;
  };
  htmlSub('HTMLInputElement', ['INPUT'], {
    ...M(valueProps),
    type: 'type', name: 'name', placeholder: 'placeholder', min: 'min', max: 'max', step: 'step',
    disabled: '?disabled', required: '?required', readOnly: '?readonly', multiple: '?multiple',
    get checked() { const s = this[ST]; return s.checked !== undefined ? s.checked : attrGet(s, 'checked') !== null; },
    set checked(v) { this[ST].checked = !!v; },
    get valueAsNumber() { return Number(this.value); },
    get files() { log('HTMLInputElement.files'); return list([]); },
  });
  htmlSub('HTMLButtonElement', ['BUTTON'], { ...M(valueProps), type: 'type', name: 'name', disabled: '?disabled' });
  htmlSub('HTMLTextAreaElement', ['TEXTAREA'], { ...M(valueProps), name: 'name', disabled: '?disabled' });
  htmlSub('HTMLSelectElement', ['SELECT'], {
    name: 'name', disabled: '?disabled', multiple: '?multiple',
    get form() { return formOf(this[ST]); },
    get options() { return list(descendants(this[ST].pub, []).filter((n) => S(n).name === 'OPTION')); },
    get selectedIndex() { const o = this.options; const i = o.findIndex((x) => S(x).selected || attrGet(S(x), 'selected') !== null); return i < 0 ? (o.length ? 0 : -1) : i; },
    set selectedIndex(i) { this.options.forEach((x, j) => { S(x).selected = j === i; }); },
    get value() { const o = this.options[this.selectedIndex]; return o ? S(o).raw.value : ''; },
    set value(v) { this.options.forEach((x) => { S(x).selected = S(x).raw.value === String(v); }); },
  });
  htmlSub('HTMLOptionElement', ['OPTION'], {
    get value() { const s = this[ST]; const v = attrGet(s, 'value'); return v === null ? text(s.pub) : v; },
    set value(v) { attrSet(this[ST], 'value', v); },
    get text() { return text(this[ST].pub); },
    get selected() { const s = this[ST]; return s.selected !== undefined ? s.selected : attrGet(s, 'selected') !== null; },
    set selected(v) { this[ST].selected = !!v; },
  });
  htmlSub('HTMLFormElement', ['FORM'], {
    action: 'action', method: 'method', name: 'name', enctype: 'enctype', target: 'target',
    get elements() { return list(descendants(this[ST].pub, []).filter((n) => ['INPUT', 'SELECT', 'TEXTAREA', 'BUTTON'].includes(S(n).name))); },
    get length() { return this.elements.length; },
    submit() { log('form submission (submit())'); },
    requestSubmit() { dispatch(this[ST].pub, makeEvent(SubmitEvent, 'submit', { bubbles: true, cancelable: true })); },
    reset() {},
  });
  htmlSub('HTMLAnchorElement', ['A'], { href: 'href', target: 'target', rel: 'rel', download: 'download' });
  htmlSub('HTMLImageElement', ['IMG'], { src: 'src', alt: 'alt', get complete() { return true; }, get naturalWidth() { return 0; }, get width() { return Number(attrGet(this[ST], 'width') || 0); }, get height() { return Number(attrGet(this[ST], 'height') || 0); } });
  htmlSub('HTMLScriptElement', ['SCRIPT'], {
    src: 'src', type: 'type', async: '?async', defer: '?defer', noModule: '?nomodule', crossOrigin: 'crossorigin', integrity: 'integrity',
    get text() { return text(this[ST].pub); }, set text(v) { setChildrenText(this[ST].pub, String(v)); },
  });
  htmlSub('HTMLStyleElement', ['STYLE'], { media: 'media', get sheet() { log('HTMLStyleElement.sheet'); return stub('CSSStyleSheet'); } });
  htmlSub('HTMLLinkElement', ['LINK'], { href: 'href', rel: 'rel', as: 'as' });
  htmlSub('HTMLMetaElement', ['META'], { name: 'name', content: 'content' });
  htmlSub('HTMLTemplateElement', ['TEMPLATE'], { get content() { return this[ST].content; } });
  htmlSub('HTMLTableElement', ['TABLE'], {
    get rows() { const out = []; for (const k of elementKids(this[ST].pub)) { const n = S(k).name; if (n === 'TR') out.push(k); else if (n === 'THEAD' || n === 'TBODY' || n === 'TFOOT') out.push(...elementKids(k).filter((r) => S(r).name === 'TR')); } return list(out); },
    get tBodies() { return list(elementKids(this[ST].pub).filter((k) => S(k).name === 'TBODY')); },
    insertRow() { const r = newElement(this[ST].doc, 'tr'); insert(this[ST].pub, r); return r; },
  });
  htmlSub('HTMLTableSectionElement', ['THEAD', 'TBODY', 'TFOOT'], { get rows() { return list(elementKids(this[ST].pub).filter((r) => S(r).name === 'TR')); } });
  htmlSub('HTMLTableRowElement', ['TR'], {
    get cells() { return list(elementKids(this[ST].pub).filter((c) => S(c).name === 'TD' || S(c).name === 'TH')); },
    insertCell() { const c = newElement(this[ST].doc, 'td'); insert(this[ST].pub, c); return c; },
  });
  htmlSub('HTMLTableCellElement', ['TD', 'TH'], { colSpan: 'colspan', rowSpan: 'rowspan' });
  htmlSub('HTMLLabelElement', ['LABEL'], { htmlFor: 'for' });
  htmlSub('HTMLCanvasElement', ['CANVAS'], {
    get width() { return Number(attrGet(this[ST], 'width') || 300); }, set width(v) { attrSet(this[ST], 'width', v); },
    get height() { return Number(attrGet(this[ST], 'height') || 150); }, set height(v) { attrSet(this[ST], 'height', v); },
    getContext(kind) { log(`canvas getContext(${kind})`); return stub(`CanvasRenderingContext(${kind})`); },
  });
  htmlSub('HTMLIFrameElement', ['IFRAME'], { src: 'src', get contentWindow() { return stub('HTMLIFrameElement.contentWindow'); } });
  htmlSub('HTMLDialogElement', ['DIALOG'], { open: '?open', close() {} });
  htmlSub('HTMLMediaElement', ['VIDEO', 'AUDIO'], { src: 'src' });
  htmlSub('HTMLSlotElement', ['SLOT'], { name: 'name', assignedNodes() { return list([]); }, assignedElements() { return list([]); } });
  for (const [n, tags] of [['HTMLDivElement', ['DIV']], ['HTMLSpanElement', ['SPAN']], ['HTMLParagraphElement', ['P']],
    ['HTMLHeadingElement', ['H1', 'H2', 'H3', 'H4', 'H5', 'H6']], ['HTMLUListElement', ['UL']], ['HTMLOListElement', ['OL']],
    ['HTMLLIElement', ['LI']], ['HTMLPreElement', ['PRE']], ['HTMLBRElement', ['BR']], ['HTMLHRElement', ['HR']],
    ['HTMLHtmlElement', ['HTML']], ['HTMLHeadElement', ['HEAD']], ['HTMLBodyElement', ['BODY']], ['HTMLTitleElement', ['TITLE']],
    ['HTMLUnknownElement', []]]) htmlSub(n, tags);

  function newElement(doc, tag) {
    tag = String(tag);
    const lower = tag.toLowerCase();
    const def = ceRegistry.get(lower);
    if (def) {
      const s = newState(1, lower.toUpperCase(), doc);
      s.ce = def;
      const pub = wrap(HTMLElement.prototype, s);
      upgrading = s;
      try { new def.cls(); } catch (e) { report(`custom element constructor <${lower}>`, e); }
      upgrading = null;
      return pub;
    }
    const s = newState(1, lower.toUpperCase(), doc);
    const C = tagClasses[s.name] || (lower.includes('-') ? HTMLElement : G.HTMLUnknownElement);
    const pub = wrap(C.prototype, s);
    if (s.name === 'TEMPLATE') s.content = newFragment(doc);
    return pub;
  }

  // Custom elements.
  const ceRegistry = new Map();
  const ceByClass = new Map();
  const ceWaiters = new Map();
  let upgrading = null;
  const CustomElementRegistry = iface('CustomElementRegistry', null, {
    define(name, cls, opts) {
      name = String(name);
      if (opts && opts.extends) log('customElements.define extends (customized built-in)');
      if (ceRegistry.has(name)) throw new Error(`NotSupportedError: '${name}' already defined`);
      const observed = new Set((cls.observedAttributes || []).map((a) => String(a).toLowerCase()));
      const def = { name, cls, observed };
      ceRegistry.set(name, def);
      ceByClass.set(cls, def);
      for (const n of descendants(document, [])) {
        const s = S(n);
        if (s.t === 1 && !s.ce && s.name === name.toUpperCase()) {
          s.ce = def;
          upgrading = s;
          try { new cls(); } catch (e) { report(`custom element upgrade <${name}>`, e); }
          upgrading = null;
          for (const a of s.attrs) if (observed.has(a[0])) attributeChanged(s, a[0], null, a[1]);
          if (connected(n)) ceReaction(n, 'connectedCallback');
        }
      }
      const w = ceWaiters.get(name);
      if (w) { w(cls); ceWaiters.delete(name); }
    },
    get(name) { const d = ceRegistry.get(String(name)); return d ? d.cls : undefined; },
    getName(cls) { const d = ceByClass.get(cls); return d ? d.name : null; },
    whenDefined(name) {
      const d = ceRegistry.get(String(name));
      if (d) return Promise.resolve(d.cls);
      return new Promise((res) => { const prev = ceWaiters.get(name); ceWaiters.set(name, prev ? (c) => { prev(c); res(c); } : res); });
    },
    upgrade() {},
  });

  // ---------------------------------------------------------------------------------------
  // Selectors: compound selectors with descendant, child and sibling combinators.
  const selCache = new Map();
  function parseSelector(src) {
    let sel = selCache.get(src);
    if (sel) return sel;
    log(`selector: ${src.length > 80 ? `${src.slice(0, 80)}...` : src}`);
    const groups = [];
    let i = 0;
    const s = src;
    const ws = () => { while (i < s.length && /\s/.test(s[i])) i++; };
    const ident = () => { const m = /^(?:\\.|[\w\- -￿])+/.exec(s.slice(i)); if (!m) throw new SyntaxError(`bad selector '${src}'`); i += m[0].length; return m[0].replace(/\\(.)/g, '$1'); };
    const compound = () => {
      const c = { tag: null, ids: [], cls: [], attrs: [], pseudo: [] };
      let any = false;
      for (;;) {
        const ch = s[i];
        if (ch === '*') { i++; any = true; } else if (ch === '#') { i++; c.ids.push(ident()); any = true; } else if (ch === '.') { i++; c.cls.push(ident()); any = true; } else if (ch === '[') {
          i++; ws(); const name = ident().toLowerCase(); ws();
          let op = null, val = null;
          const m = /^([~|^$*]?=)/.exec(s.slice(i));
          if (m) {
            op = m[1]; i += op.length; ws();
            if (s[i] === '"' || s[i] === "'") { const q = s[i++]; const j = s.indexOf(q, i); val = s.slice(i, j); i = j + 1; } else val = ident();
            ws();
            if (/[iIsS]/.test(s[i]) && s[i + 1] === ']') i++;
          }
          if (s[i] !== ']') throw new SyntaxError(`bad selector '${src}'`);
          i++;
          c.attrs.push([name, op, val]);
          any = true;
        } else if (ch === ':') {
          i++; if (s[i] === ':') i++;
          const name = ident().toLowerCase();
          let arg = null;
          if (s[i] === '(') { let depth = 1; const st = ++i; while (i < s.length && depth) { if (s[i] === '(') depth++; else if (s[i] === ')') depth--; i++; } arg = s.slice(st, i - 1); }
          c.pseudo.push([name, arg]);
          any = true;
        } else if (/[\w\-]/.test(ch || '')) { c.tag = ident().toUpperCase(); any = true; } else break;
      }
      if (!any) throw new SyntaxError(`bad selector '${src}'`);
      return c;
    };
    for (;;) {
      ws();
      const chain = [{ comb: null, c: compound() }];
      for (;;) {
        const before = i; ws();
        if (i >= s.length || s[i] === ',') break;
        let comb = ' ';
        if ('>+~'.includes(s[i])) { comb = s[i++]; ws(); } else if (i === before) break;
        chain.push({ comb, c: compound() });
      }
      groups.push(chain);
      ws();
      if (s[i] === ',') { i++; continue; }
      if (i < s.length) throw new SyntaxError(`bad selector '${src}'`);
      break;
    }
    if (selCache.size > 300) selCache.clear();
    selCache.set(src, groups);
    return groups;
  }
  function matchCompound(n, c, scope) {
    const s = S(n);
    if (!s || s.t !== 1) return false;
    if (c.tag && c.tag !== s.name) return false;
    for (const id of c.ids) if (attrGet(s, 'id') !== id) return false;
    if (c.cls.length) { const cl = classes(s); for (const x of c.cls) if (!cl.includes(x)) return false; }
    for (const [name, op, val] of c.attrs) {
      const v = attrGet(s, name);
      if (v === null) return false;
      if (op === '=' && v !== val) return false;
      if (op === '~=' && !v.split(/\s+/).includes(val)) return false;
      if (op === '|=' && v !== val && !v.startsWith(`${val}-`)) return false;
      if (op === '^=' && !v.startsWith(val)) return false;
      if (op === '$=' && !v.endsWith(val)) return false;
      if (op === '*=' && !v.includes(val)) return false;
    }
    for (const [name, arg] of c.pseudo) {
      const sib = s.parent ? elementKids(s.parent) : [n];
      switch (name) {
        case 'not': if (matchesList(n, arg, scope)) return false; break;
        case 'is': case 'where': if (!matchesList(n, arg, scope)) return false; break;
        case 'first-child': if (sib[0] !== n) return false; break;
        case 'last-child': if (sib[sib.length - 1] !== n) return false; break;
        case 'only-child': if (sib.length !== 1) return false; break;
        case 'nth-child': if (String(sib.indexOf(n) + 1) !== String(arg).trim()) return false; break;
        case 'checked': if (!(s.checked || s.selected || attrGet(s, 'checked') !== null || attrGet(s, 'selected') !== null)) return false; break;
        case 'disabled': if (attrGet(s, 'disabled') === null) return false; break;
        case 'enabled': if (attrGet(s, 'disabled') !== null) return false; break;
        case 'empty': if (s.kids.length) return false; break;
        case 'root': if (!(s.parent && S(s.parent).t === 9)) return false; break;
        case 'scope': if (n !== scope) return false; break;
        default: log(`selector :${name} (unsupported)`); return false;
      }
    }
    return true;
  }
  function matchChain(n, chain, k, scope) {
    if (!matchCompound(n, chain[k].c, scope)) return false;
    if (k === 0) return true;
    const comb = chain[k].comb;
    const up = (x) => { const p = S(x).parent; return isEl(p) ? p : null; };
    if (comb === '>') { const p = up(n); return !!p && matchChain(p, chain, k - 1, scope); }
    if (comb === ' ') { for (let p = up(n); p; p = up(p)) if (matchChain(p, chain, k - 1, scope)) return true; return false; }
    const sib = S(n).parent ? elementKids(S(n).parent) : [];
    const idx = sib.indexOf(n);
    if (comb === '+') return idx > 0 && matchChain(sib[idx - 1], chain, k - 1, scope);
    for (let j = idx - 1; j >= 0; j--) if (matchChain(sib[j], chain, k - 1, scope)) return true;
    return false;
  }
  function matchesList(n, src, scope) {
    return parseSelector(String(src).trim()).some((chain) => matchChain(n, chain, chain.length - 1, scope));
  }
  function matches(n, sel) { return matchesList(n, sel, n); }
  function querySelectorAll(rootNode, sel, first) {
    const groups = parseSelector(String(sel).trim());
    const out = [];
    const walk = (p) => {
      for (const k of kids(p)) {
        if (isEl(k) && groups.some((chain) => matchChain(k, chain, chain.length - 1, rootNode))) {
          out.push(k);
          if (first) return true;
        }
        if (walk(k)) return true;
      }
      return false;
    };
    walk(S(rootNode).content || rootNode);
    return out;
  }

  // ---------------------------------------------------------------------------------------
  // Documents.
  let activeElement = null;
  let cookies = '';
  let currentScript = null;
  let readyState = 'loading';
  const Document = iface('Document', Node, {
    get documentElement() { return elementKids(this[ST].pub)[0] || null; },
    get head() { const h = this.documentElement; return h ? elementKids(h).find((k) => S(k).name === 'HEAD') || null : null; },
    get body() { const h = this.documentElement; return h ? elementKids(h).find((k) => S(k).name === 'BODY') || null : null; },
    get title() { const t = querySelectorAll(this[ST].pub, 'title', true)[0]; return t ? text(t) : ''; },
    set title(v) { const t = querySelectorAll(this[ST].pub, 'title', true)[0]; if (t) setChildrenText(t, String(v)); },
    getElementById(id) { id = String(id); return descendants(this[ST].pub, []).find((n) => isEl(n) && attrGet(S(n), 'id') === id) || null; },
    getElementsByName(name) { return list(descendants(this[ST].pub, []).filter((n) => isEl(n) && attrGet(S(n), 'name') === String(name))); },
    createElement(tag) { return newElement(this[ST].pub, tag); },
    createElementNS(ns, tag) { if (ns !== 'http://www.w3.org/1999/xhtml') log(`createElementNS(${ns})`); const e = newElement(this[ST].pub, tag); S(e).ns = ns; return e; },
    createTextNode(d) { return newText(this[ST].pub, d); },
    createComment(d) { return newComment(this[ST].pub, d); },
    createDocumentFragment() { return newFragment(this[ST].pub); },
    createEvent(kind) { log(`createEvent(${kind})`); return new (G[kind] && G[kind].prototype instanceof Event ? G[kind] : Event)(''); },
    createRange() { return stub('Range'); },
    createTreeWalker(rootNode, what, filter) { return treeWalker(rootNode, what === undefined ? 0xffffffff : what, filter); },
    importNode(n, deep) { return clone(n, !!deep); },
    adoptNode(n) { detach(n); adopt(n, this[ST].pub); return n; },
    get readyState() { return this[ST] === S(document) ? readyState : 'complete'; },
    get cookie() { return cookies; },
    set cookie(v) { const kv = String(v).split(';')[0]; const k = kv.split('=')[0]; cookies = cookies.split('; ').filter((c) => c && c.split('=')[0] !== k).concat(kv).join('; '); },
    get URL() { return location.href; },
    get documentURI() { return location.href; },
    get baseURI() { return location.href; },
    get location() { return location; },
    get defaultView() { return this[ST] === S(document) ? win : null; },
    get currentScript() { return currentScript; },
    get activeElement() { return activeElement || this.body; },
    get hidden() { return false; },
    get visibilityState() { return 'visible'; },
    hasFocus() { return true; },
    get referrer() { return ''; },
    get characterSet() { return 'UTF-8'; },
    get contentType() { return 'text/html'; },
    get compatMode() { return 'CSS1Compat'; },
    get forms() { return this.getElementsByTagName('form'); },
    get images() { return this.getElementsByTagName('img'); },
    get links() { return list(this.querySelectorAll('a[href]')); },
    get scripts() { return this.getElementsByTagName('script'); },
    get styleSheets() { return list([]); },
    get adoptedStyleSheets() { return this[ST].sheets || (this[ST].sheets = []); },
    set adoptedStyleSheets(v) { this[ST].sheets = v; },
    get fonts() { return stub('document.fonts'); },
    get implementation() { return { createHTMLDocument: (t) => { log('DOMImplementation.createHTMLDocument'); return newDocument('', t); }, hasFeature: () => true }; },
    elementFromPoint() { return null; },
    getSelection() { return stub('Selection'); },
    execCommand() { return false; },
    write(...html) { log('document.write'); parseInto(this[ST].pub, html.join(''), this.body || this.documentElement, null); },
    writeln(...html) { this.write(...html, '\n'); },
    open() { return this; },
    close() {},
    ...M(parentNodeMixin),
    ...M(handlerProps(['readystatechange', 'visibilitychange', 'DOMContentLoaded', 'click', 'keydown'])),
  }, function Document() { return newDocument('', ''); });
  function newDocument(html, title) {
    const s = newState(9, '#document', null);
    const doc = wrap(Document.prototype, s);
    s.doc = doc;
    const tree = JSON.parse(rparse(String(html)));
    let htmlNode = tree.find((t) => Array.isArray(t) && t[0] === 'html');
    let htmlEl;
    if (htmlNode) {
      htmlEl = build(doc, htmlNode);
    } else {
      htmlEl = newElement(doc, 'html');
      const head = newElement(doc, 'head');
      const body = newElement(doc, 'body');
      insert(htmlEl, head);
      insert(htmlEl, body);
      for (const t of tree) {
        const el = build(doc, t);
        const headish = Array.isArray(t) && ['title', 'meta', 'link', 'base', 'style'].includes(t[0]);
        insert(headish ? head : body, el);
      }
      if (title) { const t = newElement(doc, 'title'); insert(t, newText(doc, title)); insert(head, t); }
    }
    // Ensure head and body exist, as an HTML parser would.
    if (!elementKids(htmlEl).some((k) => S(k).name === 'HEAD')) insert(htmlEl, newElement(doc, 'head'), elementKids(htmlEl)[0] || null);
    if (!elementKids(htmlEl).some((k) => S(k).name === 'BODY')) insert(htmlEl, newElement(doc, 'body'));
    s.kids.push(htmlEl);
    S(htmlEl).parent = doc;
    return doc;
  }
  function treeWalker(rootNode, what, filter) {
    const show = (n) => {
      const t = S(n).t;
      if (!(what & (1 << (t - 1)))) return false;
      if (filter) { const f = typeof filter === 'function' ? filter : filter.acceptNode; return f.call(filter, n) === 1; }
      return true;
    };
    let cur = rootNode;
    const next = (n) => {
      const k = kids(n);
      if (k.length) return k[0];
      for (let p = n; p && p !== rootNode; p = S(p).parent) {
        const sib = S(p).parent ? kids(S(p).parent) : [];
        const j = sib.indexOf(p);
        if (j >= 0 && j + 1 < sib.length) return sib[j + 1];
      }
      return null;
    };
    const tw = {
      get root() { return rootNode; },
      get currentNode() { return cur; },
      set currentNode(n) { cur = n; },
      nextNode() { for (let n = next(cur); n; n = next(n)) if (show(n)) { cur = n; return n; } return null; },
      parentNode() { for (let p = S(cur).parent; p && p !== S(rootNode).parent; p = S(p).parent) if (show(p)) { cur = p; return p; } return null; },
      firstChild() { const k = kids(cur).find(show); if (k) cur = k; return k || null; },
      nextSibling() { const p = S(cur).parent; if (!p) return null; const k = kids(p); for (let j = k.indexOf(cur) + 1; j < k.length; j++) if (show(k[j])) { cur = k[j]; return cur; } return null; },
    };
    return wrap(Object.create(TreeWalker.prototype, Object.getOwnPropertyDescriptors(tw)), { tw });
  }
  const TreeWalker = iface('TreeWalker', null, {});
  iface('NodeFilter', null, {}, null, { SHOW_ALL: 0xffffffff, SHOW_ELEMENT: 1, SHOW_TEXT: 4, SHOW_COMMENT: 128, FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3 });
  iface('DOMParser', null, {
    parseFromString(str, type) { if (type !== 'text/html') log(`DOMParser.parseFromString(${type})`); return newDocument(str, ''); },
  }, function DOMParser() { return wrap(new.target.prototype, {}); });

  // ---------------------------------------------------------------------------------------
  // Window-level objects: location, history, navigator, screen, storage.
  let fixtures = { url: 'http://localhost/', http: [], ws: [], sse: [] };
  const Location = iface('Location', null, {
    get href() { return this[ST].url.href; },
    set href(v) { log('navigation (location.href set)'); },
    get protocol() { return this[ST].url.protocol; },
    get host() { return this[ST].url.host; },
    get hostname() { return this[ST].url.hostname; },
    get port() { return this[ST].url.port; },
    get pathname() { return this[ST].url.pathname; },
    get search() { return this[ST].url.search; },
    get hash() { return this[ST].url.hash; },
    set hash(v) { log('navigation (location.hash set)'); },
    get origin() { return this[ST].url.origin; },
    assign() { log('navigation (location.assign)'); },
    replace() { log('navigation (location.replace)'); },
    reload() { log('navigation (location.reload)'); },
    toString() { return this[ST].url.href; },
  });
  let location;
  const History = iface('History', null, {
    get length() { return 1; },
    get state() { return this[ST].state; },
    pushState(st) { this[ST].state = st; },
    replaceState(st) { this[ST].state = st; },
    back() {}, forward() {}, go() {},
    get scrollRestoration() { return 'auto'; }, set scrollRestoration(v) {},
  });
  const Navigator = iface('Navigator', null, {
    get userAgent() { return 'Mozilla/5.0 (embedded-browser js-spike) QuickJS-ng'; },
    get appVersion() { return '5.0'; },
    get platform() { return 'embedded'; },
    get language() { return 'en-US'; },
    get languages() { return ['en-US']; },
    get onLine() { return true; },
    get cookieEnabled() { return true; },
    get hardwareConcurrency() { return 2; },
    get maxTouchPoints() { return 5; },
    get vendor() { return ''; },
    get clipboard() { return stub('navigator.clipboard'); },
    get serviceWorker() { return stub('navigator.serviceWorker'); },
    get permissions() { return stub('navigator.permissions'); },
    get mediaDevices() { return stub('navigator.mediaDevices'); },
    get wakeLock() { return stub('navigator.wakeLock'); },
    sendBeacon() { return true; },
    vibrate() { return false; },
  });
  const Screen = iface('Screen', null, {
    get width() { return 1024; }, get height() { return 600; },
    get availWidth() { return 1024; }, get availHeight() { return 600; },
    get colorDepth() { return 16; }, get pixelDepth() { return 16; },
    get orientation() { return stub('screen.orientation'); },
  });
  const Storage = iface('Storage', null, {
    get length() { return this[ST].m.size; },
    key(i) { return [...this[ST].m.keys()][i] ?? null; },
    getItem(k) { const v = this[ST].m.get(String(k)); return v === undefined ? null : v; },
    setItem(k, v) { this[ST].m.set(String(k), String(v)); },
    removeItem(k) { this[ST].m.delete(String(k)); },
    clear() { this[ST].m.clear(); },
  });

  // ---------------------------------------------------------------------------------------
  // URL and URLSearchParams (QuickJS-ng has neither).
  const URLSearchParams = iface('URLSearchParams', null, {
    append(k, v) { this[ST].p.push([String(k), String(v)]); this[ST].sync(); },
    delete(k) { this[ST].p = this[ST].p.filter((e) => e[0] !== k); this[ST].sync(); },
    get(k) { const e = this[ST].p.find((x) => x[0] === k); return e ? e[1] : null; },
    getAll(k) { return this[ST].p.filter((x) => x[0] === k).map((x) => x[1]); },
    has(k) { return this[ST].p.some((x) => x[0] === k); },
    set(k, v) { const p = this[ST].p; const i = p.findIndex((x) => x[0] === k); if (i < 0) p.push([k, String(v)]); else { p[i][1] = String(v); this[ST].p = p.filter((x, j) => j <= i || x[0] !== k); } this[ST].sync(); },
    sort() { this[ST].p.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0)); },
    forEach(fn, t) { for (const [k, v] of this[ST].p) fn.call(t, v, k, this); },
    entries() { return this[ST].p.map((e) => e.slice())[Symbol.iterator](); },
    keys() { return this[ST].p.map((e) => e[0])[Symbol.iterator](); },
    values() { return this[ST].p.map((e) => e[1])[Symbol.iterator](); },
    get size() { return this[ST].p.length; },
    toString() { return this[ST].p.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&'); },
    [Symbol.iterator]() { return this.entries(); },
  }, function URLSearchParams(init) {
    const s = { p: [], sync: () => {} };
    if (typeof init === 'string') {
      for (const part of init.replace(/^\?/, '').split('&')) {
        if (!part) continue;
        const i = part.indexOf('=');
        const dec = (x) => decodeURIComponent(x.replace(/\+/g, ' '));
        s.p.push(i < 0 ? [dec(part), ''] : [dec(part.slice(0, i)), dec(part.slice(i + 1))]);
      }
    } else if (init && typeof init === 'object') {
      const entries = typeof init[Symbol.iterator] === 'function' ? init : Object.entries(init);
      for (const [k, v] of entries) s.p.push([String(k), String(v)]);
    }
    return wrap(new.target.prototype, s);
  });
  function parseUrl(input, base) {
    input = String(input).trim();
    const abs = /^([a-zA-Z][a-zA-Z0-9+.-]*:)(?:\/\/([^/?#]*))?([^?#]*)(\?[^#]*)?(#.*)?$/.exec(input);
    if (abs) {
      const [, protocol, host = '', path, search = '', hash = ''] = abs;
      return { protocol: protocol.toLowerCase(), host: host.toLowerCase(), pathname: path || (host ? '/' : ''), search: search === '?' ? '' : search, hash: hash === '#' ? '' : hash };
    }
    if (base === undefined) throw new TypeError(`Invalid URL: ${input}`);
    const b = typeof base === 'string' ? parseUrl(base) : S(base).u;
    const m = /^([^?#]*)(\?[^#]*)?(#.*)?$/.exec(input);
    const [, path, search, hash = ''] = m;
    if (input.startsWith('//')) return parseUrl(b.protocol + input);
    let pathname = b.pathname;
    if (path.startsWith('/')) pathname = path;
    else if (path) pathname = b.pathname.replace(/[^/]*$/, '') + path;
    const segs = [];
    for (const seg of pathname.split('/')) { if (seg === '..') segs.pop(); else if (seg !== '.') segs.push(seg); }
    pathname = segs.join('/') || '/';
    if (!pathname.startsWith('/')) pathname = `/${pathname}`;
    return { protocol: b.protocol, host: b.host, pathname, search: search !== undefined ? (search === '?' ? '' : search) : (path ? '' : b.search), hash: hash === '#' ? '' : hash };
  }
  const URL = iface('URL', null, {
    get href() { const u = this[ST].u; return `${u.protocol}${u.host || u.protocol === 'file:' ? '//' : ''}${u.host}${u.pathname}${u.search}${u.hash}`; },
    set href(v) { this[ST].u = parseUrl(v); },
    get protocol() { return this[ST].u.protocol; },
    get host() { return this[ST].u.host; },
    get hostname() { return this[ST].u.host.replace(/:\d+$/, ''); },
    get port() { const m = /:(\d+)$/.exec(this[ST].u.host); return m ? m[1] : ''; },
    get pathname() { return this[ST].u.pathname; },
    set pathname(v) { this[ST].u.pathname = String(v); },
    get search() { return this[ST].u.search; },
    set search(v) { v = String(v); this[ST].u.search = v && !v.startsWith('?') ? `?${v}` : v; },
    get hash() { return this[ST].u.hash; },
    set hash(v) { v = String(v); this[ST].u.hash = v && !v.startsWith('#') ? `#${v}` : v; },
    get origin() { const u = this[ST].u; return u.host ? `${u.protocol}//${u.host}` : 'null'; },
    get searchParams() {
      const s = this[ST];
      if (!s.sp) { s.sp = make(URLSearchParams, s.u.search); S(s.sp).sync = () => { const q = s.sp.toString(); s.u.search = q ? `?${q}` : ''; }; }
      return s.sp;
    },
    toString() { return this.href; },
    toJSON() { return this.href; },
  }, function URL(input, base) { return wrap(new.target.prototype, { u: parseUrl(input, base) }); },
  { createObjectURL() { log('URL.createObjectURL'); return 'blob:stub'; }, revokeObjectURL() {}, canParse(i, b) { try { parseUrl(i, b); return true; } catch (_) { return false; } } });

  // ---------------------------------------------------------------------------------------
  // Network: fetch, XMLHttpRequest, WebSocket, EventSource. Responses come from fixtures.
  const net = [];
  const NET_DELAY = 5;
  function route(method, url, body) {
    const u = String(url);
    net.push(`${method} ${u}${body ? ` body=${String(body).slice(0, 60)}` : ''}`);
    for (const r of fixtures.http) if (u.includes(r[0]) && (!r[3] || r[3] === method)) return { status: 200, type: r[2] || 'text/plain', body: r[1] };
    return { status: 404, type: 'text/plain', body: '' };
  }
  const Headers = iface('Headers', null, {
    get(k) { const v = this[ST].m.get(String(k).toLowerCase()); return v === undefined ? null : v; },
    set(k, v) { this[ST].m.set(String(k).toLowerCase(), String(v)); },
    append(k, v) { const m = this[ST].m; k = String(k).toLowerCase(); m.set(k, m.has(k) ? `${m.get(k)}, ${v}` : String(v)); },
    has(k) { return this[ST].m.has(String(k).toLowerCase()); },
    delete(k) { this[ST].m.delete(String(k).toLowerCase()); },
    forEach(fn, t) { for (const [k, v] of this[ST].m) fn.call(t, v, k, this); },
    entries() { return this[ST].m.entries(); },
    keys() { return this[ST].m.keys(); },
    values() { return this[ST].m.values(); },
    [Symbol.iterator]() { return this[ST].m.entries(); },
  }, function Headers(init) {
    const m = new Map();
    if (init) {
      const entries = init instanceof Headers ? [...init] : (typeof init[Symbol.iterator] === 'function' ? init : Object.entries(init));
      for (const [k, v] of entries) m.set(String(k).toLowerCase(), String(v));
    }
    return wrap(new.target.prototype, { m });
  });
  const utf8 = (str) => { const out = []; for (const ch of unescape(encodeURIComponent(str))) out.push(ch.charCodeAt(0)); return new Uint8Array(out); };
  const Response = iface('Response', null, {
    get ok() { const s = this[ST].status; return s >= 200 && s < 300; },
    get status() { return this[ST].status; },
    get statusText() { return this[ST].status === 200 ? 'OK' : 'Not Found'; },
    get headers() { return this[ST].headers; },
    get url() { return this[ST].url; },
    get redirected() { return false; },
    get type() { return 'basic'; },
    get bodyUsed() { return this[ST].used; },
    get body() { return stub('Response.body (ReadableStream)'); },
    text() { this[ST].used = true; return Promise.resolve(this[ST].text); },
    json() { this[ST].used = true; try { return Promise.resolve(JSON.parse(this[ST].text)); } catch (e) { return Promise.reject(e); } },
    arrayBuffer() { this[ST].used = true; return Promise.resolve(utf8(this[ST].text).buffer); },
    blob() { return Promise.resolve(stub('Blob')); },
    formData() { return Promise.resolve(make(G.FormData)); },
    clone() { const s = this[ST]; return wrap(Response.prototype, { ...s, used: false }); },
  }, function Response(body, init) {
    init = init || {};
    return wrap(new.target.prototype, { status: init.status || 200, headers: make(Headers, init.headers), url: '', text: body == null ? '' : String(body), used: false });
  });
  iface('Request', null, {
    get url() { return this[ST].url; }, get method() { return this[ST].method; }, get headers() { return this[ST].headers; },
    clone() { return this; },
  }, function Request(input, init) {
    init = init || {};
    return wrap(new.target.prototype, { url: String(input instanceof Object && S(input) && S(input).url ? S(input).url : input), method: (init.method || 'GET').toUpperCase(), headers: make(Headers, init.headers), body: init.body });
  });
  const AbortSignal = iface('AbortSignal', EventTarget, {
    get aborted() { return !!this[ST].aborted; },
    get reason() { return this[ST].reason; },
    throwIfAborted() { if (this[ST].aborted) throw this[ST].reason; },
    ...M(handlerProps(['abort'])),
  }, null, { timeout() { log('AbortSignal.timeout'); return wrap(AbortSignal.prototype, newState(0, '', null)); }, abort() { const s = wrap(AbortSignal.prototype, newState(0, '', null)); S(s).aborted = true; return s; } });
  iface('AbortController', null, {
    get signal() { return this[ST].signal; },
    abort(reason) { const sig = this[ST].signal; const s = S(sig); if (s.aborted) return; s.aborted = true; s.reason = reason; dispatch(sig, makeEvent(Event, 'abort', {})); },
  }, function AbortController() { return wrap(new.target.prototype, { signal: wrap(AbortSignal.prototype, newState(0, '', null)) }); });
  function fetch(input, init) {
    init = init || {};
    const isReq = input != null && typeof input === 'object' && S(input) && S(input).method;
    const url = S(make(URL, isReq ? S(input).url : String(input), location.href)).raw.href;
    const method = (init.method || (isReq ? S(input).method : 'GET')).toUpperCase();
    log('fetch');
    if (init.signal) log('fetch signal option');
    return new Promise((resolve) => {
      schedule(NET_DELAY, () => {
        const r = route(method, url, init.body);
        const h = make(Headers, { 'content-type': r.type });
        resolve(wrap(Response.prototype, { status: r.status, headers: h, url, text: r.body, used: false }));
      });
    });
  }
  setGlobal('fetch', fetch);

  const XMLHttpRequest = iface('XMLHttpRequest', EventTarget, {
    get readyState() { return this[ST].rs; },
    get status() { return this[ST].status; },
    get statusText() { return this[ST].status === 200 ? 'OK' : ''; },
    get responseText() { return this[ST].body; },
    get response() { const s = this[ST]; if (s.rtype === 'json') { try { return JSON.parse(s.body); } catch (_) { return null; } } return s.body; },
    get responseType() { return this[ST].rtype; },
    set responseType(v) { this[ST].rtype = String(v); },
    get responseURL() { const s = this[ST]; return s.url ? S(make(URL, s.url, location.href)).raw.href : ''; },
    get responseXML() { return null; },
    get timeout() { return this[ST].timeoutMs || 0; }, set timeout(v) { this[ST].timeoutMs = v; },
    get withCredentials() { return false; }, set withCredentials(v) {},
    get upload() { const s = this[ST]; return s.up || (s.up = wrap(EventTarget.prototype, newState(0, '', null))); },
    open(method, url) { const s = this[ST]; s.method = String(method).toUpperCase(); s.url = String(url); s.rs = 1; s.reqHeaders = {}; this.dispatchEvent(makeEvent(Event, 'readystatechange', {})); },
    setRequestHeader(k, v) { this[ST].reqHeaders[k] = v; },
    overrideMimeType() {},
    getResponseHeader(k) { const s = this[ST]; return s.rs >= 2 && String(k).toLowerCase() === 'content-type' ? s.type : null; },
    getAllResponseHeaders() { const s = this[ST]; return s.rs >= 2 ? `content-type: ${s.type}\r\n` : ''; },
    abort() { const s = this[ST]; s.aborted = true; s.rs = 0; },
    send(body) {
      const s = this[ST];
      const self = s.pub;
      s.aborted = false;
      self.dispatchEvent(makeEvent(ProgressEvent, 'loadstart', {}));
      schedule(NET_DELAY, () => {
        if (s.aborted) return;
        const r = route(s.method, s.url, body);
        s.status = r.status; s.type = r.type; s.body = r.body;
        s.rs = 2; self.dispatchEvent(makeEvent(Event, 'readystatechange', {}));
        s.rs = 4; self.dispatchEvent(makeEvent(Event, 'readystatechange', {}));
        self.dispatchEvent(makeEvent(ProgressEvent, 'load', { loaded: r.body.length, total: r.body.length, lengthComputable: true }));
        self.dispatchEvent(makeEvent(ProgressEvent, 'loadend', {}));
      });
    },
    ...M(handlerProps(['readystatechange', 'load', 'loadend', 'loadstart', 'error', 'abort', 'timeout', 'progress'])),
  }, function XMLHttpRequest() {
    const s = newState(0, '', null);
    Object.assign(s, { rs: 0, status: 0, body: '', rtype: '', type: '' });
    return wrap(new.target.prototype, s);
  }, { UNSENT: 0, OPENED: 1, HEADERS_RECEIVED: 2, LOADING: 3, DONE: 4 });

  const WebSocket = iface('WebSocket', EventTarget, {
    get url() { return this[ST].url; },
    get readyState() { return this[ST].rs; },
    get protocol() { return ''; },
    get extensions() { return ''; },
    get bufferedAmount() { return 0; },
    get binaryType() { return this[ST].bt; }, set binaryType(v) { this[ST].bt = v; },
    send(data) {
      const s = this[ST];
      if (s.rs !== 1) throw new Error('InvalidStateError: WebSocket is not open');
      const msg = String(data);
      net.push(`WS send ${msg.slice(0, 80)}`);
      let delay = NET_DELAY;
      for (const [pattern, reply] of fixtures.ws) {
        if (pattern != null && msg.includes(pattern)) {
          const out = reply.replace(/"id":\s*"?\$ID"?/g, () => { const m = /"id":\s*(\d+)/.exec(msg); return `"id":${m ? m[1] : 0}`; });
          schedule(delay, () => wsMessage(s, out));
          delay += NET_DELAY;
        }
      }
    },
    close(code, reason) {
      const s = this[ST];
      if (s.rs >= 2) return;
      s.rs = 3;
      schedule(0, () => s.pub.dispatchEvent(makeEvent(CloseEvent, 'close', { code: code || 1000, reason: reason || '', wasClean: true })));
    },
    ...M(handlerProps(['open', 'message', 'close', 'error'])),
  }, function WebSocket(url) {
    const s = newState(0, '', null);
    Object.assign(s, { url: String(url), rs: 0, bt: 'blob' });
    const pub = wrap(new.target.prototype, s);
    net.push(`WS open ${url}`);
    schedule(20, () => {
      s.rs = 1;
      pub.dispatchEvent(makeEvent(Event, 'open', {}));
      let delay = 10;
      for (const [pattern, reply] of fixtures.ws) if (pattern == null) { schedule(delay, () => wsMessage(s, reply)); delay += 50; }
    });
    return pub;
  }, { CONNECTING: 0, OPEN: 1, CLOSING: 2, CLOSED: 3 });
  function wsMessage(s, data) { if (s.rs === 1) s.pub.dispatchEvent(makeEvent(MessageEvent, 'message', { data, origin: '' })); }

  iface('EventSource', EventTarget, {
    get url() { return this[ST].url; },
    get readyState() { return this[ST].rs; },
    get withCredentials() { return false; },
    close() { this[ST].rs = 2; },
    ...M(handlerProps(['open', 'message', 'error'])),
  }, function EventSource(url) {
    const s = newState(0, '', null);
    Object.assign(s, { url: String(url), rs: 0 });
    const pub = wrap(new.target.prototype, s);
    net.push(`SSE open ${url}`);
    schedule(20, () => {
      s.rs = 1;
      pub.dispatchEvent(makeEvent(Event, 'open', {}));
      let delay = 10;
      for (const [type, data] of fixtures.sse) {
        schedule(delay, () => { if (s.rs === 1) pub.dispatchEvent(makeEvent(MessageEvent, type, { data, lastEventId: '', origin: '' })); });
        delay += 50;
      }
    });
    return pub;
  }, { CONNECTING: 0, OPEN: 1, CLOSED: 2 });

  iface('FormData', null, {
    append(k, v) { this[ST].p.push([String(k), v]); },
    delete(k) { this[ST].p = this[ST].p.filter((e) => e[0] !== k); },
    get(k) { const e = this[ST].p.find((x) => x[0] === k); return e ? e[1] : null; },
    getAll(k) { return this[ST].p.filter((x) => x[0] === k).map((x) => x[1]); },
    has(k) { return this[ST].p.some((x) => x[0] === k); },
    set(k, v) { this.delete(k); this.append(k, v); },
    forEach(fn, t) { for (const [k, v] of this[ST].p) fn.call(t, v, k, this); },
    entries() { return this[ST].p.map((e) => e.slice())[Symbol.iterator](); },
    keys() { return this[ST].p.map((e) => e[0])[Symbol.iterator](); },
    values() { return this[ST].p.map((e) => e[1])[Symbol.iterator](); },
    [Symbol.iterator]() { return this.entries(); },
  }, function FormData(form) {
    const p = [];
    if (form) {
      for (const el of S(form).raw.elements) {
        const s = S(el);
        const name = attrGet(s, 'name');
        if (!name || attrGet(s, 'disabled') !== null) continue;
        const type = attrGet(s, 'type');
        if ((type === 'checkbox' || type === 'radio') && !s.raw.checked) continue;
        if (s.name === 'BUTTON' || type === 'submit') continue;
        p.push([name, s.raw.value]);
      }
    }
    return wrap(new.target.prototype, { p });
  });

  // ---------------------------------------------------------------------------------------
  // Observers and other APIs that are logged and otherwise inert.
  iface('MutationObserver', null, {
    observe() {}, disconnect() {}, takeRecords() { return []; },
  }, function MutationObserver(cb) { return wrap(new.target.prototype, { cb }); });
  for (const name of ['IntersectionObserver', 'ResizeObserver', 'PerformanceObserver', 'Notification', 'Worker',
    'SharedWorker', 'BroadcastChannel', 'MessageChannel', 'Image', 'Audio', 'FileReader', 'Blob', 'File',
    'CSSStyleSheet', 'Range', 'Selection', 'Animation', 'OffscreenCanvas', 'ImageData', 'Path2D',
    'CanvasRenderingContext2D', 'WebGLRenderingContext', 'DOMRect', 'DOMMatrix', 'FontFace', 'ReadableStream',
    'WritableStream', 'TransformStream', 'Option']) {
    if (!(name in G)) setGlobal(name, stub(name));
  }
  // XPath finds nothing (a generic stub's iterateNext() would never end a loop).
  const noXPathResult = { iterateNext: () => null, snapshotLength: 0, snapshotItem: () => null, singleNodeValue: null };
  iface('XPathEvaluator', null, {
    createExpression() { return { evaluate: () => noXPathResult }; },
    evaluate() { return noXPathResult; },
  }, function XPathEvaluator() { return wrap(new.target.prototype, {}); });
  if (!('Intl' in G)) setGlobal('Intl', stub('Intl'));
  setGlobal('indexedDB', stub('indexedDB'));
  setGlobal('caches', stub('caches'));
  setGlobal('visualViewport', stub('visualViewport'));
  setGlobal('CSS', { escape: (s) => { log('CSS.escape'); return String(s).replace(/([^\w-])/g, '\\$1'); }, supports: () => { log('CSS.supports'); return false; } });
  setGlobal('crypto', {
    getRandomValues(a) { log('crypto.getRandomValues'); for (let i = 0; i < a.length; i++) a[i] = Math.floor(Math.random() * 256); return a; },
    randomUUID() { log('crypto.randomUUID'); return '00000000-0000-4000-8000-000000000000'.replace(/0/g, () => Math.floor(Math.random() * 16).toString(16)); },
    get subtle() { return stub('crypto.subtle'); },
  });

  // ---------------------------------------------------------------------------------------
  // Globals. `window` is a Proxy over the global object so `window.x` reads are logged.
  const WIN = newState(0, 'window', null);
  const winProps = {
    setTimeout(fn, ms, ...a) { log('setTimeout'); const f = callbackOf(fn, 'setTimeout'); return schedule(ms, () => f(...a)); },
    clearTimeout(id) { log('clearTimeout'); cancel(id); },
    setInterval(fn, ms, ...a) { log('setInterval'); const f = callbackOf(fn, 'setInterval'); return schedule(ms, () => f(...a), true); },
    clearInterval(id) { log('clearInterval'); cancel(id); },
    requestAnimationFrame(fn) { log('requestAnimationFrame'); return schedule(16, () => fn(now)); },
    cancelAnimationFrame(id) { log('cancelAnimationFrame'); cancel(id); },
    requestIdleCallback(fn) { log('requestIdleCallback'); return schedule(1, () => fn({ didTimeout: false, timeRemaining: () => 10 })); },
    cancelIdleCallback(id) { cancel(id); },
    queueMicrotask(fn) { log('queueMicrotask'); Promise.resolve().then(fn).catch((e) => report('microtask', e)); },
    structuredClone(v) { log('structuredClone'); return JSON.parse(JSON.stringify(v)); },
    getComputedStyle(el) { log('getComputedStyle'); return new Proxy({}, { get: (_, k) => (k === 'getPropertyValue' ? () => '' : (typeof k === 'string' ? '' : undefined)) }); },
    matchMedia(q) { log(`matchMedia`); return { matches: false, media: String(q), onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} }; },
    alert() { log('alert'); }, confirm() { log('confirm'); return true; }, prompt() { log('prompt'); return ''; },
    open() { log('window.open'); return null; }, close() {}, print() { log('print'); }, focus() {}, blur() {},
    postMessage() { log('postMessage'); },
    scrollTo() { log('window.scrollTo'); }, scrollBy() { log('window.scrollBy'); }, scroll() { log('window.scroll'); },
    getSelection() { return stub('Selection'); },
    addEventListener(type, fn, opts) { log('window.addEventListener'); EventTarget.prototype.addEventListener.call(win, type, fn, opts); },
    removeEventListener(type, fn, opts) { log('window.removeEventListener'); EventTarget.prototype.removeEventListener.call(win, type, fn, opts); },
    dispatchEvent(ev) { log('window.dispatchEvent'); return dispatch(win, ev); },
  };
  for (const k of Object.keys(winProps)) setGlobal(k, winProps[k]);
  if (typeof G.atob !== 'function') {
    const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    setGlobal('btoa', (s) => { log('btoa'); s = String(s); let o = ''; for (let i = 0; i < s.length; i += 3) { const n = (s.charCodeAt(i) << 16) | ((s.charCodeAt(i + 1) || 0) << 8) | (s.charCodeAt(i + 2) || 0); o += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (i + 1 < s.length ? B64[(n >> 6) & 63] : '=') + (i + 2 < s.length ? B64[n & 63] : '='); } return o; });
    setGlobal('atob', (s) => { log('atob'); s = String(s).replace(/=+$/, ''); let o = ''; for (let i = 0, b = 0, n = 0; i < s.length; i++) { n = (n << 6) | B64.indexOf(s[i]); b += 6; if (b >= 8) { b -= 8; o += String.fromCharCode((n >> b) & 255); } } return o; });
  }
  if (typeof G.TextEncoder !== 'function') {
    setGlobal('TextEncoder', function TextEncoder() { log('TextEncoder'); return { encoding: 'utf-8', encode: (s) => utf8(String(s === undefined ? '' : s)) }; });
    setGlobal('TextDecoder', function TextDecoder() { log('TextDecoder'); return { encoding: 'utf-8', decode: (b) => { let s = ''; for (const c of new Uint8Array(b && b.buffer ? b.buffer : (b || []))) s += String.fromCharCode(c); try { return decodeURIComponent(escape(s)); } catch (_) { return s; } } }; });
  }
  const perfStart = Date.now();
  setGlobal('performance', { now: () => now, timeOrigin: perfStart, mark() { log('performance.mark'); }, measure() { log('performance.measure'); }, getEntriesByName: () => [], getEntriesByType: () => [], clearMarks() {}, clearMeasures() {} });
  // console.error/warn are recorded with the exceptions: libraries such as htmx catch handler
  // errors and report them only there.
  const fmt = (a) => a.map((x) => { try { return x instanceof Error ? `${x} ${x.stack || ''}` : (typeof x === 'object' ? JSON.stringify(x) : String(x)); } catch (_) { return String(x); } }).join(' ').slice(0, 500);
  setGlobal('console', new Proxy({}, {
    get: (_, k) => (typeof k === 'string' ? (...a) => { log(`console.${k}`); if (k === 'error' || k === 'warn') rerr(`console.${k}: ${fmt(a)}`); } : undefined),
  }));
  const document = newDocument('', '');
  const win = new Proxy(G, {
    get(t, k) {
      if (typeof k === 'string' && !(k in t) && !/^(on|webkit|moz|ms|__)/.test(k)) log(`window.${k} (missing)`);
      return Reflect.get(t, k);
    },
    set(t, k, v) { return Reflect.set(t, k, v); },
  });
  WIN.pub = win;
  for (const ev of ['load', 'error', 'resize', 'popstate', 'hashchange', 'message', 'beforeunload', 'unload', 'online', 'offline', 'scroll', 'keydown', 'focus', 'blur', 'click', 'storage']) {
    Object.defineProperty(G, `on${ev}`, {
      get() { return (WIN.h && WIN.h[ev]) || null; },
      set(v) { log(`window.on${ev} (set)`); (WIN.h || (WIN.h = {}))[ev] = typeof v === 'function' ? v : null; },
      configurable: true,
    });
  }
  const navigator = wrap(Navigator.prototype, {});
  const values = {
    window: win, self: win, top: win, parent: win, frames: win, globalThis: G, document, navigator,
    history: wrap(History.prototype, { state: null }), screen: wrap(Screen.prototype, {}),
    localStorage: wrap(Storage.prototype, { m: new Map() }), sessionStorage: wrap(Storage.prototype, { m: new Map() }),
    customElements: wrap(CustomElementRegistry.prototype, {}),
    innerWidth: 1024, innerHeight: 600, outerWidth: 1024, outerHeight: 600, devicePixelRatio: 1,
    scrollX: 0, scrollY: 0, pageXOffset: 0, pageYOffset: 0, isSecureContext: false, frameElement: null,
    name: '', closed: false, length: 0,
  };
  for (const k of Object.keys(values)) if (k !== 'globalThis') setGlobal(k, values[k]);
  Object.defineProperty(G, 'location', { get() { return location; }, set() { log('navigation (window.location set)'); }, configurable: true });
  Object.defineProperty(G, 'origin', { get() { return location.origin; }, configurable: true });

  // ---------------------------------------------------------------------------------------
  // Control object for Rust.
  let interactTargets = [];
  return {
    load(html, fixtureJson) {
      fixtures = Object.assign(fixtures, JSON.parse(fixtureJson));
      location = wrap(Location.prototype, { url: S(make(URL, fixtures.url)).raw });
      const doc = newDocument(html, '');
      const ds = S(document);
      ds.kids = S(doc).kids;
      for (const k of ds.kids) { S(k).parent = document; adopt(k, document); }
      return descendants(document, []).filter((n) => S(n).name === 'SCRIPT').map((n) => {
        const s = S(n);
        return { src: attrGet(s, 'src'), text: text(n), type: attrGet(s, 'type') || '' };
      });
    },
    setCurrentScript(i) {
      const scripts = descendants(document, []).filter((n) => S(n).name === 'SCRIPT');
      currentScript = i < 0 ? null : scripts[i];
      readyState = 'interactive';
    },
    domReady() {
      readyState = 'interactive';
      dispatch(document, makeEvent(Event, 'readystatechange', {}));
      dispatch(document, makeEvent(Event, 'DOMContentLoaded', { bubbles: true }));
      readyState = 'complete';
      dispatch(document, makeEvent(Event, 'readystatechange', {}));
      // load fires at the window only.
      const ev = makeEvent(Event, 'load', {});
      S(ev).target = document;
      const fire = () => {
        if (WIN.ls && WIN.ls.has('load')) for (const l of WIN.ls.get('load').slice()) { try { if (typeof l.fn === 'function') l.fn.call(win, ev); else l.fn.handleEvent(ev); } catch (e) { report('listener load', e); } }
        if (WIN.h && WIN.h.load) { try { WIN.h.load.call(win, ev); } catch (e) { report('onload', e); } }
      };
      fire();
    },
    // Runs the next task if it is due before `until`; returns false when none is.
    step(until) {
      if (!tasks.length || tasks[0].t > until) return false;
      const task = tasks.shift();
      now = task.t;
      if (task.iv) { task.t = now + task.iv; task.s = seq++; let i = tasks.length; while (i > 0 && tasks[i - 1].t > task.t) i--; tasks.splice(i, 0, task); }
      tasksRun++;
      try { task.fn(); } catch (e) { report(`task at t=${now}ms`, e); }
      return true;
    },
    now() { return now; },
    // Elements a user could interact with: listeners or inline handlers for click/change.
    collectTargets() {
      interactTargets = descendants(document, []).filter((n) => {
        const s = S(n);
        if (s.t !== 1) return false;
        const has = (t) => (s.ls && s.ls.has(t) && s.ls.get(t).length) || (s.h && s.h[t]) || attrGet(s, `on${t}`) != null;
        return has('click') || has('change') || attrGet(s, 'hx-get') != null || attrGet(s, 'hx-post') != null || attrGet(s, '@click') != null || attrGet(s, 'v-on:click') != null;
      });
      return interactTargets.length;
    },
    interact(i) {
      const n = interactTargets[i];
      if (!n || !connected(n)) return;
      const s = S(n);
      if (s.name === 'FORM') {
        // Submit the way a user would: click the form's submit button (default action fires
        // `submit`), or dispatch `submit` when it has none.
        const btn = descendants(n, []).find((d) => { const ds = S(d); return (ds.name === 'BUTTON' && (attrGet(ds, 'type') || 'submit') === 'submit') || (ds.name === 'INPUT' && attrGet(ds, 'type') === 'submit'); });
        if (btn) dispatch(btn, makeEvent(MouseEvent, 'click', { bubbles: true, cancelable: true, button: 0 }));
        else dispatch(n, makeEvent(SubmitEvent, 'submit', { bubbles: true, cancelable: true }));
        return;
      }
      const changes = s.name === 'SELECT' || (s.ls && s.ls.has('change')) || (s.h && s.h.change) || attrGet(s, 'onchange') != null;
      if (changes) {
        if (s.name === 'SELECT') { if (s.raw.options.length > 1) s.raw.selectedIndex = 1; } else if (s.name === 'INPUT') s.value = '42';
        dispatch(n, makeEvent(Event, 'input', { bubbles: true }));
        dispatch(n, makeEvent(Event, 'change', { bubbles: true }));
      } else {
        for (const t of ['pointerdown', 'mousedown', 'pointerup', 'mouseup']) dispatch(n, makeEvent(MouseEvent, t, { bubbles: true, cancelable: true }));
        dispatch(n, makeEvent(MouseEvent, 'click', { bubbles: true, cancelable: true, button: 0 }));
      }
    },
    summary() {
      const all = descendants(document, []);
      return {
        elements: all.filter((n) => S(n).t === 1).length,
        nodes: all.length,
        tasksRun,
        pendingTasks: tasks.length,
        net: net.slice(),
        bodyText: text(document.body || document).replace(/\s+/g, ' ').trim().slice(0, 400),
      };
    },
  };
})()
