// Run the wasm32-wasip1 build of `measure` under Node's WASI, with the repo mounted at /repo.
// Usage: node tools/measure/wasi-run.mjs <measure.wasm> <args...>   (run from the repo root)
import { readFile } from "node:fs/promises";
import { WASI } from "node:wasi";

const [wasmPath, ...args] = process.argv.slice(2);
const wasi = new WASI({
  version: "preview1",
  args: ["measure", ...args],
  env: {},
  preopens: { "/repo": process.cwd() },
});
const module = await WebAssembly.compile(await readFile(wasmPath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
process.exitCode = wasi.start(instance);
