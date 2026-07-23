// Vendors the wasm-pack outputs into ts/wasm/ AND the site's REPL island
// (site/src/wasm — the same web-target build): all three locations are
// committed, like dorado's web/src/wasm, so `npm install` and CI never need
// a Rust toolchain. Run via `npm run build:wasm`, which builds ../rust/wasm
// for both targets first (`pkg/` = nodejs, `pkg-web/` = web) and then
// invokes this copy step.
import { copyFileSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const wasmCrate = join(here, "..", "..", "rust", "wasm");
const files = [
  "anzan_wasm.js",
  "anzan_wasm.d.ts",
  "anzan_wasm_bg.wasm",
  "anzan_wasm_bg.wasm.d.ts",
];

for (const [source, target] of [
  [join(wasmCrate, "pkg"), join(here, "..", "wasm", "node")],
  [join(wasmCrate, "pkg-web"), join(here, "..", "wasm", "web")],
  [join(wasmCrate, "pkg-web"), join(here, "..", "..", "site", "src", "wasm")],
]) {
  mkdirSync(target, { recursive: true });
  for (const file of files) copyFileSync(join(source, file), join(target, file));
}

// The nodejs-target output is CommonJS; the package root is `type: module`,
// so wasm/node needs its own package.json to keep require() reading it as CJS.
writeFileSync(
  join(here, "..", "wasm", "node", "package.json"),
  JSON.stringify({ type: "commonjs" }, null, 2) + "\n",
);

console.log("vendored rust/wasm pkg → wasm/node and pkg-web → wasm/web + site/src/wasm");
