// The Anzan engine in the browser — the REAL Rust engine compiled to WASM
// (rust/wasm → wasm-pack --target web), vendored into src/wasm/ so the site
// builds with no Rust toolchain (regenerate with `npm run build:wasm` in ts/
// after engine changes, or the demo drifts from the apps).
//
// `ensureWasm()` memoizes init into a single promise (the dorado pattern):
// components await it before constructing calculators.
import init, { WasmCalculator, reference } from "../wasm/anzan_wasm.js";
import wasmUrl from "../wasm/anzan_wasm_bg.wasm?url";

let ready: Promise<unknown> | null = null;

export function ensureWasm(): Promise<unknown> {
  ready ??= init({ module_or_path: wasmUrl });
  return ready;
}

export { WasmCalculator, reference };
