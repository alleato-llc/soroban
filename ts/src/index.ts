// @alleato/anzan — Anzan, the exact calculation language, for JS hosts.
// A typed wrapper over the engine backend (the Rust engine compiled to
// WebAssembly): the backend speaks JSON strings across the wasm boundary;
// this module parses them ONCE into discriminated unions, so consumers only
// ever see typed values. Sessions are stateful, exactly like the app's log
// and the native CLIs: `ans`, variables, user functions, and the mode carry
// across `evaluate` calls.

import {
  defaultBackend,
  type BackendAccumulator,
  type BackendSession,
  type EngineBackend,
} from "./backend.js";

export {
  wasmBackend,
  defaultBackend,
  type EngineBackend,
  type BackendSession,
  type BackendAccumulator,
} from "./backend.js";

/** The language dialect — see docs/MODES.md. */
export type Mode = "normal" | "programmer" | "finance";

/** What a successful statement produced. */
export type EvalKind = "value" | "function" | "data" | "documentation" | "comment";

export interface EvalSuccess {
  ok: true;
  kind: EvalKind;
  /** The canonical, re-parseable form — what persists (`Money(10, "USD")`). */
  description: string;
  /** The human echo — how the log and CLI show it (`$10.00`). */
  displayDescription: string;
  /** A multi-line string result, raw (pretty JSON and friends) — hosts print
   * this as a block instead of one line of `\n` escapes. */
  rawBlock?: string;
}

export interface AnzanError {
  ok: false;
  error: string;
  /** Character offset of the error in the statement, when the engine has one
   * (the caret column every host renders). */
  position?: number;
}

export type EvalOutcome = EvalSuccess | AnzanError;

/** One logical statement out of the splitter: physical lines join while a
 * `( [ {` is open. `line` is the 1-based first physical line. */
export interface Statement {
  text: string;
  line: number;
}

/** One entry of a script run. `line`/`statement` are absent only for a
 * source-level split error (an unterminated block at end of input). */
export type ScriptEntry = EvalOutcome & { line?: number; statement?: string };

export interface ScriptResult {
  results: ScriptEntry[];
  /** True when the run stopped at an error (script semantics — the remaining
   * statements did not run). */
  halted: boolean;
}

export interface Completion {
  name: string;
}

export interface FunctionDoc {
  signature: string;
  summary: string;
  examples: string[];
}

/** One calculation session. */
export class Calculator {
  private readonly session: BackendSession;

  constructor(backend: EngineBackend = defaultBackend) {
    this.session = backend.createSession();
  }

  /** Evaluate one statement in this session. */
  evaluate(line: string): EvalOutcome {
    return JSON.parse(this.session.evaluate(line)) as EvalOutcome;
  }

  /** Run a multi-line source as a script (the `.anzan` contract): statements
   * split by the engine's accumulator, evaluated in this session, halting at
   * the first error. */
  runScript(source: string): ScriptResult {
    return JSON.parse(this.session.runScript(source)) as ScriptResult;
  }

  get mode(): Mode {
    return this.session.getMode() as Mode;
  }

  set mode(mode: Mode) {
    this.session.setMode(mode);
  }

  /** Identifier completions for a prefix — the same engine autocomplete the
   * apps and REPLs use. */
  completions(prefix: string): Completion[] {
    return JSON.parse(this.session.completions(prefix)) as Completion[];
  }

  /** Documentation for a builtin, special form, or user function. */
  documentation(name: string): FunctionDoc | null {
    return JSON.parse(this.session.documentation(name)) as FunctionDoc | null;
  }
}

/** The streaming statement splitter (pipes/REPLs): push physical lines, get
 * completed logical statements as they close. */
export class StatementAccumulator {
  private readonly inner: BackendAccumulator;

  constructor(backend: EngineBackend = defaultBackend) {
    this.inner = backend.createAccumulator();
  }

  /** Feed one physical line; returns the completed logical statement, or
   * `null` while a bracket is still open. */
  push(line: string): Statement | null {
    return JSON.parse(this.inner.push(line)) as Statement | null;
  }

  isPending(): boolean {
    return this.inner.isPending();
  }

  pendingText(): string {
    return this.inner.pendingText();
  }

  /** End of input: `null` when clean, or the error for an unterminated
   * block. */
  finish(): AnzanError | null {
    return JSON.parse(this.inner.finish()) as AnzanError | null;
  }
}

/** Run a source in a FRESH session and return the typed results — the
 * one-call embedding form. */
export function runScript(source: string, backend: EngineBackend = defaultBackend): ScriptResult {
  return new Calculator(backend).runScript(source);
}

/** Split a source into its logical statements without evaluating anything.
 * Throws on an unterminated block at end of input. */
export function statements(source: string, backend: EngineBackend = defaultBackend): Statement[] {
  const accumulator = new StatementAccumulator(backend);
  const out: Statement[] = [];
  for (const line of source.split("\n")) {
    const statement = accumulator.push(line);
    if (statement) out.push(statement);
  }
  const error = accumulator.finish();
  if (error) throw new Error(error.error);
  return out;
}

/** The trailing `# comment` of a line — the CLI's pretty echo. */
export function trailingComment(line: string): string | undefined {
  return defaultBackend.trailingComment(line);
}

/** Whether a line speaks programmer notation (0x/0b, bit functions). */
export function usesProgrammerNotation(line: string): boolean {
  return defaultBackend.usesProgrammerNotation(line);
}
