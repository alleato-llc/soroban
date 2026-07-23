/* tslint:disable */
/* eslint-disable */

/**
 * One calculation session (the log/CLI model): `ans`, variables, user
 * functions, and the language mode persist across `evaluate` calls.
 */
export class WasmCalculator {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Identifier completions for a prefix — JSON `[{"name":…}]` (the same
     * engine autocomplete the apps and REPLs use).
     */
    completions(prefix: string): string;
    /**
     * Documentation for a name — JSON
     * `{"signature":…,"summary":…,"examples":[…]}` or `null`.
     */
    documentation(name: string): string;
    /**
     * The session's ENVIRONMENT — what the apps' inspector shows. JSON:
     * `{"ans":{"description":…,"display":…}?, "variables":[{name,display,
     * canonical}], "functions":[{name,source}], "dataTypes":[{name,
     * declaration}]}`, each list sorted by name.
     */
    environment(): string;
    /**
     * Evaluate one statement. Returns a JSON string:
     * `{"ok":true,"kind":"value|function|data|documentation|comment",
     *   "description":…,"displayDescription":…,"rawBlock":…?}` or
     * `{"ok":false,"error":…,"position":…?}`. `description` is the
     * canonical, re-parseable form (what persists); `displayDescription`
     * is the human echo (`$10.00`).
     */
    evaluate(line: string): string;
    constructor();
    /**
     * Run a multi-line script (the `.anzan` contract): statements split by
     * the engine's accumulator (an open `( [ {` continues onto the next
     * line), evaluated in this session, HALTING at the first error. Returns
     * `{"results":[{"line":N,"statement":…, …outcome…}],"halted":bool}`.
     */
    runScript(source: string): string;
    /**
     * Applies a `:mode` command argument — "programmer", "scientific eng" —
     * through the engine's shared parse seam (`Calculator::set_mode_parsing`),
     * the same one the native CLIs, the GUI, and the spec use. Throws the
     * engine's own error text on an unknown mode/style (`:mode finance` gets
     * the currency-promotion hint).
     */
    setModeParsing(argument: string): void;
    /**
     * The language mode — "normal" | "programmer" | "scientific".
     */
    mode: string;
    /**
     * The Scientific-mode echo variant — "sci" (default) | "eng"
     * (`:mode scientific eng`). Display only; ignored outside scientific.
     */
    sciStyle: string;
}

/**
 * The streaming statement splitter (pipes/REPLs): push physical lines, get
 * completed logical statements. `push` returns JSON
 * `{"text":…,"line":N}` or `"null"`; `finish` returns `"null"` or an error
 * object for an unterminated block.
 */
export class WasmStatementAccumulator {
    free(): void;
    [Symbol.dispose](): void;
    finish(): string;
    isPending(): boolean;
    constructor();
    pendingText(): string;
    push(line: string): string;
}

/**
 * The full builtin REFERENCE — what the apps' help browser (⌘/) lists. JSON
 * `[{"name":…,"category":…,"signature":…,"summary":…,"examples":[…]}]` in
 * registry order (categories arrive grouped).
 */
export function reference(): string;

/**
 * The CLI display heuristics, for the ts CLI's pretty mode.
 */
export function trailingComment(line: string): string | undefined;

export function usesProgrammerNotation(line: string): boolean;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmcalculator_free: (a: number, b: number) => void;
    readonly __wbg_wasmstatementaccumulator_free: (a: number, b: number) => void;
    readonly reference: () => [number, number];
    readonly trailingComment: (a: number, b: number) => [number, number];
    readonly usesProgrammerNotation: (a: number, b: number) => number;
    readonly wasmcalculator_completions: (a: number, b: number, c: number) => [number, number];
    readonly wasmcalculator_documentation: (a: number, b: number, c: number) => [number, number];
    readonly wasmcalculator_environment: (a: number) => [number, number];
    readonly wasmcalculator_evaluate: (a: number, b: number, c: number) => [number, number];
    readonly wasmcalculator_mode: (a: number) => [number, number];
    readonly wasmcalculator_new: () => number;
    readonly wasmcalculator_runScript: (a: number, b: number, c: number) => [number, number];
    readonly wasmcalculator_sciStyle: (a: number) => [number, number];
    readonly wasmcalculator_setModeParsing: (a: number, b: number, c: number) => [number, number];
    readonly wasmcalculator_set_mode: (a: number, b: number, c: number) => [number, number];
    readonly wasmcalculator_set_sciStyle: (a: number, b: number, c: number) => [number, number];
    readonly wasmstatementaccumulator_finish: (a: number) => [number, number];
    readonly wasmstatementaccumulator_isPending: (a: number) => number;
    readonly wasmstatementaccumulator_new: () => number;
    readonly wasmstatementaccumulator_pendingText: (a: number) => [number, number];
    readonly wasmstatementaccumulator_push: (a: number, b: number, c: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
