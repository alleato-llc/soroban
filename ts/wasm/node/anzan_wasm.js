/* @ts-self-types="./anzan_wasm.d.ts" */

/**
 * One calculation session (the log/CLI model): `ans`, variables, user
 * functions, and the language mode persist across `evaluate` calls.
 */
class WasmCalculator {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmCalculatorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmcalculator_free(ptr, 0);
    }
    /**
     * Identifier completions for a prefix — JSON `[{"name":…}]` (the same
     * engine autocomplete the apps and REPLs use).
     * @param {string} prefix
     * @returns {string}
     */
    completions(prefix) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(prefix, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmcalculator_completions(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Documentation for a name — JSON
     * `{"signature":…,"summary":…,"examples":[…]}` or `null`.
     * @param {string} name
     * @returns {string}
     */
    documentation(name) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmcalculator_documentation(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * The session's ENVIRONMENT — what the apps' inspector shows. JSON:
     * `{"ans":{"description":…,"display":…}?, "variables":[{name,display,
     * canonical}], "functions":[{name,source}], "dataTypes":[{name,
     * declaration}]}`, each list sorted by name.
     * @returns {string}
     */
    environment() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmcalculator_environment(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Evaluate one statement. Returns a JSON string:
     * `{"ok":true,"kind":"value|function|data|documentation|comment",
     *   "description":…,"displayDescription":…,"rawBlock":…?}` or
     * `{"ok":false,"error":…,"position":…?}`. `description` is the
     * canonical, re-parseable form (what persists); `displayDescription`
     * is the human echo (`$10.00`).
     * @param {string} line
     * @returns {string}
     */
    evaluate(line) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(line, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmcalculator_evaluate(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * The language mode — "normal" | "programmer" | "scientific".
     * @returns {string}
     */
    get mode() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmcalculator_mode(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    constructor() {
        const ret = wasm.wasmcalculator_new();
        this.__wbg_ptr = ret;
        WasmCalculatorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Run a multi-line script (the `.anzan` contract): statements split by
     * the engine's accumulator (an open `( [ {` continues onto the next
     * line), evaluated in this session, HALTING at the first error. Returns
     * `{"results":[{"line":N,"statement":…, …outcome…}],"halted":bool}`.
     * @param {string} source
     * @returns {string}
     */
    runScript(source) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmcalculator_runScript(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * The Scientific-mode echo variant — "sci" (default) | "eng"
     * (`:mode scientific eng`). Display only; ignored outside scientific.
     * @returns {string}
     */
    get sciStyle() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmcalculator_sciStyle(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Applies a `:mode` command argument — "programmer", "scientific eng" —
     * through the engine's shared parse seam (`Calculator::set_mode_parsing`),
     * the same one the native CLIs, the GUI, and the spec use. Throws the
     * engine's own error text on an unknown mode/style (`:mode finance` gets
     * the currency-promotion hint).
     * @param {string} argument
     */
    setModeParsing(argument) {
        const ptr0 = passStringToWasm0(argument, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmcalculator_setModeParsing(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Setting rides the engine's one shared `:mode` parse seam
     * (`Calculator::set_mode_parsing`), so the mode list and the
     * unknown-mode errors (including the `finance` promotion hint) can
     * never drift from the native hosts'.
     * @param {string} mode
     */
    set mode(mode) {
        const ptr0 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmcalculator_set_mode(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {string} style
     */
    set sciStyle(style) {
        const ptr0 = passStringToWasm0(style, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmcalculator_set_sciStyle(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) WasmCalculator.prototype[Symbol.dispose] = WasmCalculator.prototype.free;
exports.WasmCalculator = WasmCalculator;

/**
 * The streaming statement splitter (pipes/REPLs): push physical lines, get
 * completed logical statements. `push` returns JSON
 * `{"text":…,"line":N}` or `"null"`; `finish` returns `"null"` or an error
 * object for an unterminated block.
 */
class WasmStatementAccumulator {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmStatementAccumulatorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmstatementaccumulator_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    finish() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmstatementaccumulator_finish(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {boolean}
     */
    isPending() {
        const ret = wasm.wasmstatementaccumulator_isPending(this.__wbg_ptr);
        return ret !== 0;
    }
    constructor() {
        const ret = wasm.wasmstatementaccumulator_new();
        this.__wbg_ptr = ret;
        WasmStatementAccumulatorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {string}
     */
    pendingText() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmstatementaccumulator_pendingText(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {string} line
     * @returns {string}
     */
    push(line) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(line, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wasmstatementaccumulator_push(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) WasmStatementAccumulator.prototype[Symbol.dispose] = WasmStatementAccumulator.prototype.free;
exports.WasmStatementAccumulator = WasmStatementAccumulator;

/**
 * The full builtin REFERENCE — what the apps' help browser (⌘/) lists. JSON
 * `[{"name":…,"category":…,"signature":…,"summary":…,"examples":[…]}]` in
 * registry order (categories arrive grouped).
 * @returns {string}
 */
function reference() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.reference();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}
exports.reference = reference;

/**
 * The CLI display heuristics, for the ts CLI's pretty mode.
 * @param {string} line
 * @returns {string | undefined}
 */
function trailingComment(line) {
    const ptr0 = passStringToWasm0(line, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.trailingComment(ptr0, len0);
    let v2;
    if (ret[0] !== 0) {
        v2 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v2;
}
exports.trailingComment = trailingComment;

/**
 * @param {string} line
 * @returns {boolean}
 */
function usesProgrammerNotation(line) {
    const ptr0 = passStringToWasm0(line, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.usesProgrammerNotation(ptr0, len0);
    return ret !== 0;
}
exports.usesProgrammerNotation = usesProgrammerNotation;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_92b29b0548f8b746: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_now_86c0d4ba3fa605b8: function() {
            const ret = Date.now();
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./anzan_wasm_bg.js": import0,
    };
}

const WasmCalculatorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmcalculator_free(ptr, 1));
const WasmStatementAccumulatorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmstatementaccumulator_free(ptr, 1));

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
function decodeText(ptr, len) {
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

const wasmPath = `${__dirname}/anzan_wasm_bg.wasm`;
const wasmBytes = require('fs').readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(wasmBytes);
let wasmInstance = new WebAssembly.Instance(wasmModule, __wbg_get_imports());
let wasm = wasmInstance.exports;
wasm.__wbindgen_start();
