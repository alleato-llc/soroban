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
     * The language mode — "normal" | "programmer" | "finance".
     */
    mode: string;
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
 * The CLI display heuristics, for the ts CLI's pretty mode.
 */
export function trailingComment(line: string): string | undefined;

export function usesProgrammerNotation(line: string): boolean;
