// The engine backend seam. Today the only backend is the Rust engine compiled
// to WebAssembly (vendored under ../wasm); a future pure-TS engine fills the
// same slot. The backend speaks the wasm boundary dialect — JSON strings, one
// crossing per statement — and `index.ts` parses them once into typed values,
// so consumers never see strings.
//
// Loading is lazy and cached (dorado's createRequire trick in
// ts/src/engine/wasm-backend.ts): merely importing this module never fails,
// and a missing vendored build throws an actionable error naming
// `npm run build:wasm` instead of a raw module-not-found.

import { createRequire } from "node:module";

/** One stateful calculation session at the JSON boundary: `ans`, variables,
 * user functions, and the mode persist across `evaluate` calls. */
export interface BackendSession {
  /** JSON: `{ok:true, kind, description, displayDescription, rawBlock?}`
   * or `{ok:false, error, position?}`. */
  evaluate(line: string): string;
  /** JSON: `{results:[{line, statement, ...outcome}], halted}` — halts at
   * the first error, like a `.anzan` script. */
  runScript(source: string): string;
  getMode(): string;
  /** Throws on an unknown mode name (the engine's own message — `finance`
   * gets the currency-promotion hint). */
  setMode(mode: string): void;
  getSciStyle(): string;
  /** Throws on an unknown style name (`sci` | `eng`). */
  setSciStyle(style: string): void;
  /** Applies a `:mode` command argument ("scientific eng") through the
   * engine's shared parse seam; throws the engine's error text. */
  setModeParsing(argument: string): void;
  /** JSON: `[{name}]`. */
  completions(prefix: string): string;
  /** JSON: `{ans, variables, functions, dataTypes}` — the session's
   * environment, what the apps' inspector shows. */
  environment(): string;
  /** JSON: `{signature, summary, examples}` or `"null"`. */
  documentation(name: string): string;
}

/** The streaming statement splitter (pipes/REPLs): push physical lines, get
 * completed logical statements. */
export interface BackendAccumulator {
  /** JSON: `{text, line}` or `"null"`. */
  push(line: string): string;
  isPending(): boolean;
  pendingText(): string;
  /** `"null"`, or an error object for an unterminated block. */
  finish(): string;
}

export interface EngineBackend {
  createSession(): BackendSession;
  createAccumulator(): BackendAccumulator;
  /** The trailing `# comment` of a line, for the CLI's pretty echo. */
  trailingComment(line: string): string | undefined;
  /** Whether a line speaks programmer notation (0x/0b, bit functions). */
  usesProgrammerNotation(line: string): boolean;
  /** JSON: every builtin — `[{name, category, signature, summary,
   * examples}]` — the apps' help/reference browser. */
  reference(): string;
}

// The wasm-pack `--target nodejs` surface (see ../wasm/node/anzan_wasm.d.ts).
interface WasmCalculatorInstance {
  evaluate(line: string): string;
  runScript(source: string): string;
  mode: string;
  sciStyle: string;
  setModeParsing(argument: string): void;
  completions(prefix: string): string;
  documentation(name: string): string;
  environment(): string;
}

interface WasmAccumulatorInstance {
  push(line: string): string;
  isPending(): boolean;
  pendingText(): string;
  finish(): string;
}

interface WasmExports {
  WasmCalculator: new () => WasmCalculatorInstance;
  WasmStatementAccumulator: new () => WasmAccumulatorInstance;
  trailingComment(line: string): string | undefined;
  usesProgrammerNotation(line: string): boolean;
  reference(): string;
}

const require = createRequire(import.meta.url);
let cached: WasmExports | undefined;

function loadWasm(): WasmExports {
  if (cached) return cached;
  try {
    cached = require("../wasm/node/anzan_wasm.js") as WasmExports;
  } catch (e) {
    throw new Error(
      "the Anzan WASM build is missing. Build and vendor it once with:\n" +
        "  npm run build:wasm\n" +
        "(needs wasm-pack; the result is committed, so a fresh clone should not hit this)\n" +
        `(underlying error: ${e instanceof Error ? e.message : String(e)})`,
    );
  }
  return cached;
}

/** The Rust engine over WebAssembly — the default (and today the only)
 * backend. */
export const wasmBackend: EngineBackend = {
  createSession() {
    const session = new (loadWasm().WasmCalculator)();
    return {
      evaluate: (line) => session.evaluate(line),
      runScript: (source) => session.runScript(source),
      getMode: () => session.mode,
      setMode: (mode) => {
        session.mode = mode;
      },
      getSciStyle: () => session.sciStyle,
      setSciStyle: (style) => {
        session.sciStyle = style;
      },
      setModeParsing: (argument) => session.setModeParsing(argument),
      completions: (prefix) => session.completions(prefix),
      documentation: (name) => session.documentation(name),
      environment: () => session.environment(),
    };
  },
  createAccumulator() {
    return new (loadWasm().WasmStatementAccumulator)();
  },
  trailingComment: (line) => loadWasm().trailingComment(line),
  usesProgrammerNotation: (line) => loadWasm().usesProgrammerNotation(line),
  reference: () => loadWasm().reference(),
};

export const defaultBackend: EngineBackend = wasmBackend;
