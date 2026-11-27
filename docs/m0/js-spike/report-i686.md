Target: x86 (32-bit), release build; QuickJS-ng 0.16.2.

Baseline: runtime + full context 152 KB; with the stub DOM loaded 544 KB (stub share 392 KB). Heaps below are allocator peaks over the run including runtime and context but excluding the stub DOM's share; caps are QuickJS `set_memory_limit` values with the stub's QuickJS-accounted share added on top.

| item | script bytes | compile ms | run ms | events ms | peak heap | heap at end | peak, stripped bytecode | 1 MB | 4 MB | 16 MB | exceptions | APIs |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dashboard | 3718 | 0.64 | 0.33 | 18.97 | 309 KB | 300 KB | 299 KB | pass | pass | pass | 0 | 35 |
| htmx | 52182 | 18.48 | 1.16 | 257.22 | 936 KB | 761 KB | 759 KB | pass | pass | pass | 0 | 94 |
| petite-vue | 16901 | 7.73 | 12.24 | 2.98 | 414 KB | 382 KB | 414 KB | pass | pass | pass | 0 | 34 |
| tasmota | 1997 | 0.63 | 0.09 | 34.56 | 368 KB | 287 KB | 360 KB | pass | pass | pass | 0 | 24 |
| esphome-v1 | 5213 | 0.57 | 4.20 | 13.48 | 319 KB | 296 KB | 319 KB | pass | pass | pass | 0 | 21 |
| home-assistant | 4561636 | 1536.32 | 679.72 | 4155.09 | 33870 KB | 33860 KB | 33870 KB | FAIL after interactions (OOM) | FAIL after interactions (OOM) | FAIL after interactions | 4 | 112 |

### Syntax probe

- optional chaining ?. (ES2020): ok
- nullish ?? (ES2020): ok
- BigInt (ES2020): ok
- Promise.allSettled, globalThis (ES2020): ok
- logical assignment ||= &&= ??= (ES2021): ok
- numeric separators (ES2021): ok
- String.replaceAll, Promise.any (ES2021): ok
- class fields, private methods, static blocks (ES2022): ok
- Array.at, Object.hasOwn, Error cause (ES2022): ok
- RegExp d flag, named groups, lookbehind: ok
- findLast, toSorted, Array.with (ES2023): ok
- Object.groupBy, Promise.withResolvers (ES2024): ok
- RegExp v flag (ES2024): ok
- Set methods, Iterator helpers (ES2025): ok
- async generators, for await (ES2018): ok
- Intl (ECMA-402): FAILED (Intl missing)

### dashboard

Phases completed: context → stub → dom → scripts → load-events → event-loop → interactions. Elements at end: 36. Tasks run: 31. Interactions: 4. Heap without stub share: peak during scripts 219 KB, peak overall 309 KB, at end 300 KB; with stripped bytecode: peak 299 KB, at end 289 KB. Raw (with stub): peak 702 KB, QuickJS malloc_size at end 575 KB (2313 objects).

| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |
|---|---|---|---|---|---|---|
| dashboard.js | 3718 | 0.64 | 6 | 0.33 | 4171 | 0.05 |

Network (7 requests/messages): `WS open ws://panel.local:8123/api/websocket`, `GET http://panel.local:8123/api/states`, `WS send {"type":"auth","access_token":"synthetic-token"}`, `WS send {"id":1,"type":"subscribe_events","event_type":"state_changed"}`, `POST http://panel.local:8123/api/services/light/toggle body={"entity_id":"light....`, `POST http://panel.local:8123/api/services/switch/toggle body={"entity_id":"switc...`, `GET http://panel.local:8123/api/states`

Web APIs touched:

- **DOM core** (19): `CSSStyleDeclaration.opacity (set)`, `DOMTokenList.add`, `DOMTokenList.remove`, `DOMTokenList.toggle`, `Document.body`, `Document.getElementById`, `Document.querySelector`, `DocumentFragment.firstElementChild`, `Element.classList`, `Element.className (set)`, `Element.id (set)`, `Element.querySelector`, `HTMLElement.dataset`, `HTMLElement.hidden (set)`, `HTMLElement.style`, `HTMLTemplateElement.content`, `Node.appendChild`, `Node.cloneNode`, `Node.textContent (set)`
- **events** (5): `Event.data`, `EventTarget.addEventListener`, `EventTarget.dispatchEvent`, `inline onclick attribute`, `window.addEventListener`
- **timers** (1): `setInterval`
- **network** (6): `Response.json`, `Response.ok`, `WebSocket.onclose (set)`, `WebSocket.send`, `fetch`, `new WebSocket`
- **storage** (2): `Storage.getItem`, `Storage.setItem`
- **other** (2): `Location.href`, `Location.origin`
- **selectors** (6): `selector: #cards`, `selector: #clock`, `selector: #conn`, `selector: .name`, `selector: .state`, `selector: .toggle`

### htmx

Phases completed: context → stub → dom → scripts → load-events → event-loop → interactions. Elements at end: 21. Tasks run: 34. Interactions: 4. Heap without stub share: peak during scripts 753 KB, peak overall 936 KB, at end 761 KB; with stripped bytecode: peak 759 KB, at end 643 KB. Raw (with stub): peak 1328 KB, QuickJS malloc_size at end 829 KB (2668 objects).

| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |
|---|---|---|---|---|---|---|
| htmx.min.js | 52182 | 18.48 | 3 | 1.16 | 72355 | 0.89 |

Network (12 requests/messages): `GET /status`, `GET /status`, `GET /status`, `GET /status`, `GET /status`, `GET /status`, `GET /status`, `POST /door/toggle`, `GET /status`, `GET /details`, `GET /status`, `GET /status`

Web APIs touched:

- **DOM core** (50): `DOMParser.parseFromString`, `DOMTokenList.add`, `DOMTokenList.length`, `DOMTokenList.remove`, `Document.activeElement`, `Document.body`, `Document.createDocumentFragment`, `Document.createElement`, `Document.head`, `Document.querySelector`, `Document.querySelectorAll`, `Document.readyState`, `Document.title`, `DocumentFragment.querySelector`, `DocumentFragment.querySelectorAll`, `Element.attributes`, `Element.children`, `Element.classList`, `Element.closest`, `Element.getAttribute`, `Element.hasAttribute`, `Element.insertAdjacentHTML`, `Element.matches`, `Element.querySelector`, `Element.querySelectorAll`, `Element.remove`, `Element.removeAttribute`, `Element.tagName`, `HTMLBodyElement.selectionEnd (missing)`, `HTMLBodyElement.selectionStart (missing)`, `HTMLButtonElement.form`, `HTMLButtonElement.name`, `HTMLButtonElement.value`, `HTMLDivElement.name (missing)`, `HTMLTemplateElement.content`, `Node.appendChild`, `Node.childNodes`, `Node.cloneNode`, `Node.firstChild`, `Node.getRootNode`, `Node.insertBefore`, `Node.isConnected`, `Node.nextSibling`, `Node.nodeType`, `Node.parentElement`, `Node.parentNode`, `Node.previousSibling`, `Node.removeChild`, `Text.children (missing)`, `new DOMParser`
- **events** (10): `Event.detail`, `Event.lengthComputable`, `Event.loaded`, `Event.target`, `Event.total`, `Event.type`, `EventTarget.addEventListener`, `EventTarget.dispatchEvent`, `new CustomEvent`, `window.onpopstate (set)`
- **timers** (1): `setTimeout`
- **network** (23): `FormData.forEach`, `FormData.keys`, `URL.origin`, `URL.pathname`, `URL.search`, `XMLHttpRequest.getResponseHeader`, `XMLHttpRequest.onabort (set)`, `XMLHttpRequest.onerror (set)`, `XMLHttpRequest.onload (set)`, `XMLHttpRequest.ontimeout (set)`, `XMLHttpRequest.open`, `XMLHttpRequest.overrideMimeType`, `XMLHttpRequest.response`, `XMLHttpRequest.responseURL`, `XMLHttpRequest.send`, `XMLHttpRequest.setRequestHeader`, `XMLHttpRequest.status`, `XMLHttpRequest.timeout (set)`, `XMLHttpRequest.upload`, `XMLHttpRequest.withCredentials (set)`, `new FormData`, `new URL`, `new XMLHttpRequest`
- **storage** (2): `Storage.removeItem`, `Storage.setItem`
- **other** (8): `CSS.escape`, `Location.href`, `Location.origin`, `Location.pathname`, `Location.protocol`, `Location.search`, `XPathEvaluator.createExpression`, `new XPathEvaluator`
- **selectors** (24): `selector: #--htmx-preserve-pantry--`, `selector: #alerts`, `selector: #details`, `selector: #result`, `selector: DIV#alerts`, `selector: UL#details`, `selector: [autofocus]`, `selector: [hx-disable], [data-hx-disable]`, `selector: [hx-get], [data-hx-get], [hx-post], [data-hx-post], [hx-put], [data-hx-put], [hx...`, `selector: [hx-preserve], [data-hx-preserve]`, `selector: [hx-swap-oob], [data-hx-swap-oob]`, `selector: [hx-trigger='restored'],[data-hx-trigger='restored']`, `selector: [id]`, `selector: a`, `selector: button, input[type='submit']`, `selector: form`, `selector: input, textarea, select`, `selector: input[type="button"], input[type="submit"]`, `selector: input[type="submit"], button`, `selector: meta[name="htmx-config"]`, `selector: script`, `selector: template`, `selector: template[hx]`, `selector: title`

### petite-vue

Phases completed: context → stub → dom → scripts → load-events → event-loop → interactions. Elements at end: 18. Tasks run: 0. Interactions: 3. Heap without stub share: peak during scripts 414 KB, peak overall 414 KB, at end 382 KB; with stripped bytecode: peak 414 KB, at end 340 KB. Raw (with stub): peak 807 KB, QuickJS malloc_size at end 670 KB (2393 objects).

| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |
|---|---|---|---|---|---|---|
| petite-vue.iife.js | 16901 | 7.73 | 2 | 12.24 | 31106 | 0.41 |

Web APIs touched:

- **DOM core** (33): `CSSStyleDeclaration.display`, `CSSStyleDeclaration.display (set)`, `CharacterData.data`, `Document.activeElement`, `Document.body`, `Document.currentScript`, `Document.documentElement`, `Element.attributes`, `Element.className`, `Element.getAttribute`, `Element.hasAttribute`, `Element.matches`, `Element.nextElementSibling`, `Element.querySelectorAll`, `Element.removeAttribute`, `Element.setAttribute`, `Element.tagName`, `HTMLElement.style`, `HTMLInputElement.composing (missing)`, `HTMLInputElement.type`, `HTMLInputElement.value`, `HTMLInputElement.value (set)`, `Node.cloneNode`, `Node.firstChild`, `Node.insertBefore`, `Node.nextSibling`, `Node.nodeType`, `Node.parentElement`, `Node.parentNode`, `Node.removeChild`, `Node.textContent (set)`, `new Comment`, `new Text`
- **events** (1): `EventTarget.addEventListener`
- **selectors** (2): `selector: [v-scope]`, `selector: [v-scope] [v-scope]`

### tasmota

Phases completed: context → stub → dom → scripts → load-events → event-loop → interactions. Elements at end: 45. Tasks run: 18. Interactions: 3. Heap without stub share: peak during scripts 227 KB, peak overall 368 KB, at end 287 KB; with stripped bytecode: peak 360 KB, at end 287 KB. Raw (with stub): peak 760 KB, QuickJS malloc_size at end 551 KB (2131 objects).

| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |
|---|---|---|---|---|---|---|
| root.js | 1997 | 0.63 | 3 | 0.09 | 3145 | 0.05 |

Network (11 requests/messages): `GET .?m=1[object Object]`, `GET .?m=1`, `GET .?m=1`, `GET .?m=1`, `GET .?m=1`, `GET .?m=1&d0=42`, `GET .?m=1`, `GET .?m=1&o=1`, `GET .?m=1`, `GET .?m=1&o=2`, `GET .?m=1`

Web APIs touched:

- **DOM core** (10): `Document.body`, `Document.getElementById`, `Document.querySelectorAll`, `Element.hasAttribute`, `Element.id`, `Element.innerHTML (set)`, `HTMLButtonElement.name`, `HTMLButtonElement.name (set)`, `HTMLInputElement.name (set)`, `HTMLInputElement.value`
- **events** (4): `EventTarget.dispatchEvent`, `inline onchange attribute`, `inline onclick attribute`, `window.addEventListener`
- **timers** (2): `clearTimeout`, `setTimeout`
- **network** (8): `XMLHttpRequest.abort`, `XMLHttpRequest.onreadystatechange (set)`, `XMLHttpRequest.open`, `XMLHttpRequest.readyState`, `XMLHttpRequest.responseText`, `XMLHttpRequest.send`, `XMLHttpRequest.status`, `new XMLHttpRequest`
- **selectors** (1): `selector: input,button,textarea,select`

### esphome-v1

Phases completed: context → stub → dom → scripts → load-events → event-loop → interactions. Elements at end: 54. Tasks run: 15. Interactions: 5. Heap without stub share: peak during scripts 247 KB, peak overall 319 KB, at end 296 KB; with stripped bytecode: peak 319 KB, at end 293 KB. Raw (with stub): peak 711 KB, QuickJS malloc_size at end 572 KB (2338 objects).

| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |
|---|---|---|---|---|---|---|
| www.js | 5213 | 0.57 | 9 | 4.20 | 3390 | 0.04 |

Network (6 requests/messages): `SSE open /events`, `POST /switch/relay/toggle`, `POST /light/Desk%20lamp/toggle`, `POST /cover/blind/open`, `POST /cover/blind/close`, `POST /select/mode/set?option=Manual`

Web APIs touched:

- **DOM core** (13): `DOMTokenList.contains`, `Document.body`, `Document.getElementById`, `Document.querySelector`, `Element.children`, `Element.classList`, `Element.id`, `Element.innerHTML`, `Element.innerHTML (set)`, `HTMLElement.dataset`, `HTMLElement.innerText (set)`, `HTMLSelectElement.value`, `HTMLTableElement.rows`
- **events** (3): `Event.data`, `EventTarget.addEventListener`, `EventTarget.dispatchEvent`
- **network** (4): `XMLHttpRequest.open`, `XMLHttpRequest.send`, `new EventSource`, `new XMLHttpRequest`
- **other** (1): `CSS.escape`
- **selectors** (3): `selector: [data-device]`, `selector: tr[data-domain="light"][data-name="Desk\ lamp"]:not([data-device])`, `selector: tr[data-domain="sensor"][data-name="Humidity"]:not([data-device])`

### home-assistant

Phases completed: context → stub → dom → scripts → load-events → event-loop → interactions. Elements at end: 9. Tasks run: 12. Interactions: 0. Heap without stub share: peak during scripts 8863 KB, peak overall 33870 KB, at end 33860 KB; with stripped bytecode: peak 33870 KB, at end 33860 KB. Raw (with stub): peak 34263 KB, QuickJS malloc_size at end 33188 KB (61604 objects).

| script | bytes | compile ms | KB/ms | run ms | stripped bytecode bytes | bytecode load ms |
|---|---|---|---|---|---|---|
| core.d1e78ca365209979.js (module) | 56662 | 26.98 | 2 | 2.90 | - | - |
| app.b64f3cdeb78eeb1b.js (module) | 603590 | 161.79 | 4 | 676.82 | - | - |
| 95 imported modules (module) | 0 | 1347.55 | - | 0.00 | - | - |

Cap 1 MB: FAIL after interactions (OOM); peak without stub share 1102 KB; first new errors: ["module testpages/fetched/home-assistant/frontend_latest/app.b64f3cdeb78eeb1b.js: InternalError: out of memory "]

Cap 4 MB: FAIL after interactions (OOM); peak without stub share 4207 KB; first new errors: ["module testpages/fetched/home-assistant/frontend_latest/app.b64f3cdeb78eeb1b.js: InternalError: out of memory "]

Cap 16 MB: FAIL after interactions; peak without stub share 16989 KB; first new errors: ["unhandled rejection: Null\"null\""]

Exceptions (4):

- `unhandled rejection: Error: Fail to fetch translation en-ebf6226d270f8d6ce15858aa1d5c7c36.json: HTTP response status is 404 at <anonymous> (testpages/fetched/home-assistant/frontend_latest/app.b64f3cdeb78eeb1b.js:135:61651)`
- `unhandled rejection: Error: Fail to fetch translation en-ebf6226d270f8d6ce15858aa1d5c7c36.json: HTTP response status is 404 at <anonymous> (testpages/fetched/home-assistant/frontend_latest/app.b64f3cdeb78eeb1b.js:135:61651)`
- `unhandled rejection: Error: Fail to fetch translation en-ebf6226d270f8d6ce15858aa1d5c7c36.json: HTTP response status is 404 at <anonymous> (testpages/fetched/home-assistant/frontend_latest/app.b64f3cdeb78eeb1b.js:135:61651)`
- `unhandled rejection: Error: Fail to fetch translation en-ebf6226d270f8d6ce15858aa1d5c7c36.json: HTTP response status is 404 at <anonymous> (testpages/fetched/home-assistant/frontend_latest/app.b64f3cdeb78eeb1b.js:135:61651)`

Network (10 requests/messages): `GET http://homeassistant.local:8123/manifest.json`, `GET http://homeassistant.local:8123/static/translations/en-ebf6226d270f8d6ce1585...`, `GET http://homeassistant.local:8123/static/locale-data/intl-datetimeformat/add-a...`, `GET http://homeassistant.local:8123/.well-known/oauth-authorization-server`, `GET http://homeassistant.local:8123/static/locale-data/intl-datetimeformat/en.js...`, `GET http://homeassistant.local:8123/static/locale-data/intl-displaynames/en.json`, `GET http://homeassistant.local:8123/static/locale-data/intl-listformat/en.json`, `GET http://homeassistant.local:8123/static/locale-data/intl-numberformat/en.json`, `GET http://homeassistant.local:8123/static/locale-data/intl-relativetimeformat/e...`, `GET http://homeassistant.local:8123/static/translations/en-ebf6226d270f8d6ce1585...`

Web APIs touched:

- **DOM core** (28): `CSSStyleDeclaration.tabSize`, `Document.all (missing)`, `Document.body`, `Document.createComment`, `Document.createElement`, `Document.createTreeWalker`, `Document.documentElement`, `Document.getElementById`, `Document.head`, `Document.location`, `Document.readyState`, `Document.registry (missing)`, `Element.append`, `Element.matches`, `HTMLElement.dir`, `HTMLElement.lang`, `HTMLElement.renderRoot (missing)`, `HTMLElement.style`, `HTMLInputElement.required (set)`, `HTMLInputElement.type (set)`, `HTMLInputElement.validationMessage (missing)`, `MutationObserver.observe`, `Node.firstChild`, `Node.getRootNode`, `Node.insertBefore`, `Node.isConnected`, `Node.textContent (set)`, `new MutationObserver`
- **events** (5): `EventTarget.addEventListener`, `EventTarget.dispatchEvent`, `new Event`, `window.MSPointerEvent (missing)`, `window.addEventListener`
- **timers** (2): `clearTimeout`, `setTimeout`
- **network** (8): `AbortController.signal`, `Response.ok`, `Response.status`, `fetch`, `fetch signal option`, `navigation (location.href set)`, `new AbortController`, `new URL`
- **storage** (2): `Storage.getItem`, `Storage.selectedLanguage (missing)`
- **custom elements / shadow DOM** (8): `CSSStyleSheet.prototype`, `CSSStyleSheet.prototype.replaceSync`, `CustomElementRegistry.define`, `CustomElementRegistry.get`, `Element.attachShadow`, `Element.shadowRoot`, `new HTMLElement`, `window.CustomElementRegistryPolyfill (missing)`
- **other** (59): `History.length`, `History.replaceState`, `History.state`, `Intl.DateTimeFormat`, `Intl.DateTimeFormat()`, `Intl.DateTimeFormat().resolvedOptions`, `Intl.DateTimeFormat().resolvedOptions()`, `Intl.DateTimeFormat().resolvedOptions().timeZone`, `Intl.DateTimeFormat.__addLocaleData`, `Intl.DateTimeFormat.__addTZData`, `Intl.DateTimeFormat.__setDefaultTimeZone`, `Intl.DateTimeFormat.__setDefaultTimeZone()`, `Intl.DisplayNames`, `Intl.DisplayNames.__addLocaleData`, `Intl.DisplayNames.polyfilled`, `Intl.DisplayNames.supportedLocalesOf`, `Intl.DisplayNames.supportedLocalesOf()`, `Intl.ListFormat`, `Intl.ListFormat.__addLocaleData`, `Intl.Locale`, `Intl.Locale.prototype`, `Intl.NumberFormat`, `Intl.NumberFormat.__addLocaleData`, `Intl.PluralRules`, `Intl.PluralRules.__addLocaleData`, `Intl.PluralRules.__addLocaleData()`, `Intl.RelativeTimeFormat`, `Intl.RelativeTimeFormat.__addLocaleData`, `Intl.Segmenter`, `Intl.getCanonicalLocales`, `Intl.getCanonicalLocales()`, `Intl.getCanonicalLocales().0`, `Intl.getCanonicalLocales().map`, `Intl.getCanonicalLocales().map()`, `Location.host`, `Location.hostname`, `Location.href`, `Location.href (set)`, `Location.pathname`, `Location.protocol`, `Location.search`, `Navigator.language`, `Navigator.languages`, `Navigator.maxTouchPoints`, `Navigator.platform`, `Navigator.scheduling (missing)`, `Navigator.userAgent`, `Navigator.vendor`, `matchMedia`, `new MessageChannel`, `new MessageChannel.port1`, `new MessageChannel.port1.onmessage (set)`, `new MessageChannel.port2`, `new MessageChannel.port2.postMessage`, `new MessageChannel.port2.postMessage.name`, `window.ElementInternals (missing)`, `window.externalApp (missing)`, `window.externalAppV2 (missing)`, `window.hassConnectionReady (missing)`
- **selectors** (2): `selector :dir (unsupported)`, `selector: :dir(rtl)`

J2 set: union over dashboard, htmx, tasmota, esphome-v1 — 132 distinct API paths on 34 interfaces/globals.

- **DOM core** (70): `CSSStyleDeclaration.opacity (set)` (da), `DOMParser.parseFromString` (ht), `DOMTokenList.add` (da/ht), `DOMTokenList.contains` (es), `DOMTokenList.length` (ht), `DOMTokenList.remove` (da/ht), `DOMTokenList.toggle` (da), `Document.activeElement` (ht), `Document.body` (da/es/ht/ta), `Document.createDocumentFragment` (ht), `Document.createElement` (ht), `Document.getElementById` (da/es/ta), `Document.head` (ht), `Document.querySelector` (da/es/ht), `Document.querySelectorAll` (ht/ta), `Document.readyState` (ht), `Document.title` (ht), `DocumentFragment.firstElementChild` (da), `DocumentFragment.querySelector` (ht), `DocumentFragment.querySelectorAll` (ht), `Element.attributes` (ht), `Element.children` (es/ht), `Element.classList` (da/es/ht), `Element.className (set)` (da), `Element.closest` (ht), `Element.getAttribute` (ht), `Element.hasAttribute` (ht/ta), `Element.id` (es/ta), `Element.id (set)` (da), `Element.innerHTML` (es), `Element.innerHTML (set)` (es/ta), `Element.insertAdjacentHTML` (ht), `Element.matches` (ht), `Element.querySelector` (da/ht), `Element.querySelectorAll` (ht), `Element.remove` (ht), `Element.removeAttribute` (ht), `Element.tagName` (ht), `HTMLBodyElement.selectionEnd (missing)` (ht), `HTMLBodyElement.selectionStart (missing)` (ht), `HTMLButtonElement.form` (ht), `HTMLButtonElement.name` (ht/ta), `HTMLButtonElement.name (set)` (ta), `HTMLButtonElement.value` (ht), `HTMLDivElement.name (missing)` (ht), `HTMLElement.dataset` (da/es), `HTMLElement.hidden (set)` (da), `HTMLElement.innerText (set)` (es), `HTMLElement.style` (da), `HTMLInputElement.name (set)` (ta), `HTMLInputElement.value` (ta), `HTMLSelectElement.value` (es), `HTMLTableElement.rows` (es), `HTMLTemplateElement.content` (da/ht), `Node.appendChild` (da/ht), `Node.childNodes` (ht), `Node.cloneNode` (da/ht), `Node.firstChild` (ht), `Node.getRootNode` (ht), `Node.insertBefore` (ht), `Node.isConnected` (ht), `Node.nextSibling` (ht), `Node.nodeType` (ht), `Node.parentElement` (ht), `Node.parentNode` (ht), `Node.previousSibling` (ht), `Node.removeChild` (ht), `Node.textContent (set)` (da), `Text.children (missing)` (ht), `new DOMParser` (ht)
- **events** (14): `Event.data` (da/es), `Event.detail` (ht), `Event.lengthComputable` (ht), `Event.loaded` (ht), `Event.target` (ht), `Event.total` (ht), `Event.type` (ht), `EventTarget.addEventListener` (da/es/ht), `EventTarget.dispatchEvent` (da/es/ht/ta), `inline onchange attribute` (ta), `inline onclick attribute` (da/ta), `new CustomEvent` (ht), `window.addEventListener` (da/ta), `window.onpopstate (set)` (ht)
- **timers** (3): `clearTimeout` (ta), `setInterval` (da), `setTimeout` (ht/ta)
- **network** (34): `FormData.forEach` (ht), `FormData.keys` (ht), `Response.json` (da), `Response.ok` (da), `URL.origin` (ht), `URL.pathname` (ht), `URL.search` (ht), `WebSocket.onclose (set)` (da), `WebSocket.send` (da), `XMLHttpRequest.abort` (ta), `XMLHttpRequest.getResponseHeader` (ht), `XMLHttpRequest.onabort (set)` (ht), `XMLHttpRequest.onerror (set)` (ht), `XMLHttpRequest.onload (set)` (ht), `XMLHttpRequest.onreadystatechange (set)` (ta), `XMLHttpRequest.ontimeout (set)` (ht), `XMLHttpRequest.open` (es/ht/ta), `XMLHttpRequest.overrideMimeType` (ht), `XMLHttpRequest.readyState` (ta), `XMLHttpRequest.response` (ht), `XMLHttpRequest.responseText` (ta), `XMLHttpRequest.responseURL` (ht), `XMLHttpRequest.send` (es/ht/ta), `XMLHttpRequest.setRequestHeader` (ht), `XMLHttpRequest.status` (ht/ta), `XMLHttpRequest.timeout (set)` (ht), `XMLHttpRequest.upload` (ht), `XMLHttpRequest.withCredentials (set)` (ht), `fetch` (da), `new EventSource` (es), `new FormData` (ht), `new URL` (ht), `new WebSocket` (da), `new XMLHttpRequest` (es/ht/ta)
- **storage** (3): `Storage.getItem` (da), `Storage.removeItem` (ht), `Storage.setItem` (da/ht)
- **other** (8): `CSS.escape` (es/ht), `Location.href` (da/ht), `Location.origin` (da/ht), `Location.pathname` (ht), `Location.protocol` (ht), `Location.search` (ht), `XPathEvaluator.createExpression` (ht), `new XPathEvaluator` (ht)
