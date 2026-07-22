//! `anzan-wasm` — the Anzan engine for JS hosts, a THIN binding over the
//! `anzan` crate (mirroring dorado's rust/wasm: never reimplement language
//! logic here). Coarse-grained by design: one boundary crossing per statement,
//! JSON strings across the boundary (the TS wrapper in ts/ parses and types
//! them). Sessions are stateful — `WasmCalculator` carries `ans`, variables,
//! functions, and the mode exactly like the app's log and the native CLIs.
#![forbid(unsafe_code)]

use anzan::{Calculator, EvalOutcome, LanguageMode, StatementAccumulator};
use serde_json::json;
use wasm_bindgen::prelude::*;

/// One calculation session (the log/CLI model): `ans`, variables, user
/// functions, and the language mode persist across `evaluate` calls.
#[wasm_bindgen]
pub struct WasmCalculator {
    inner: Calculator,
}

#[wasm_bindgen]
impl WasmCalculator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmCalculator {
        WasmCalculator {
            inner: Calculator::new(),
        }
    }

    /// Evaluate one statement. Returns a JSON string:
    /// `{"ok":true,"kind":"value|function|data|documentation|comment",
    ///   "description":…,"displayDescription":…,"rawBlock":…?}` or
    /// `{"ok":false,"error":…,"position":…?}`. `description` is the
    /// canonical, re-parseable form (what persists); `displayDescription`
    /// is the human echo (`$10.00`).
    pub fn evaluate(&mut self, line: &str) -> String {
        outcome_json(self.inner.evaluate(line)).to_string()
    }

    /// Run a multi-line script (the `.anzan` contract): statements split by
    /// the engine's accumulator (an open `( [ {` continues onto the next
    /// line), evaluated in this session, HALTING at the first error. Returns
    /// `{"results":[{"line":N,"statement":…, …outcome…}],"halted":bool}`.
    #[wasm_bindgen(js_name = runScript)]
    pub fn run_script(&mut self, source: &str) -> String {
        let statements = match StatementAccumulator::statements(source) {
            Ok(statements) => statements,
            Err(error) => {
                return json!({
                    "results": [error_json(&error)],
                    "halted": true,
                })
                .to_string()
            }
        };
        let mut results = Vec::new();
        let mut halted = false;
        for statement in statements {
            let outcome = self.inner.evaluate(&statement.text);
            let failed = outcome.is_err();
            let mut entry = outcome_json(outcome);
            entry["line"] = json!(statement.line);
            entry["statement"] = json!(statement.text);
            results.push(entry);
            if failed {
                halted = true;
                break; // halt like a script
            }
        }
        json!({ "results": results, "halted": halted }).to_string()
    }

    /// The language mode — "normal" | "programmer" | "finance".
    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> String {
        self.inner.mode.name().to_string()
    }

    #[wasm_bindgen(setter, js_name = mode)]
    pub fn set_mode(&mut self, mode: &str) -> Result<(), JsError> {
        match LanguageMode::from_name(&mode.to_lowercase()) {
            Some(mode) => {
                self.inner.mode = mode;
                Ok(())
            }
            None => Err(JsError::new(
                "unknown mode — use normal, programmer, or finance",
            )),
        }
    }

    /// Identifier completions for a prefix — JSON `[{"name":…}]` (the same
    /// engine autocomplete the apps and REPLs use).
    pub fn completions(&self, prefix: &str) -> String {
        let names: Vec<_> = self
            .inner
            .completions(prefix)
            .into_iter()
            .map(|c| json!({ "name": c.name }))
            .collect();
        json!(names).to_string()
    }

    /// The session's ENVIRONMENT — what the apps' inspector shows. JSON:
    /// `{"ans":{"description":…,"display":…}?, "variables":[{name,display,
    /// canonical}], "functions":[{name,source}], "dataTypes":[{name,
    /// declaration}]}`, each list sorted by name.
    pub fn environment(&self) -> String {
        let env = self.inner.environment();
        let mut variables: Vec<_> = env
            .user_variables()
            .iter()
            .map(|(name, value)| {
                json!({
                    "name": name,
                    "display": value.display_description(),
                    "canonical": value.to_string(),
                })
            })
            .collect();
        variables.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let mut functions: Vec<_> = env
            .all_user_functions()
            .iter()
            .map(|f| json!({ "name": f.name, "source": f.source }))
            .collect();
        functions.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let mut data_types: Vec<_> = env
            .user_data_types()
            .iter()
            .map(|(name, t)| json!({ "name": name, "declaration": t.source }))
            .collect();
        data_types.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let ans = env.ans();
        json!({
            "ans": {
                "description": ans.to_string(),
                "display": ans.display_description(),
            },
            "variables": variables,
            "functions": functions,
            "dataTypes": data_types,
        })
        .to_string()
    }

    /// Documentation for a name — JSON
    /// `{"signature":…,"summary":…,"examples":[…]}` or `null`.
    pub fn documentation(&self, name: &str) -> String {
        match self.inner.documentation_for(name) {
            Some(doc) => json!({
                "signature": doc.signature,
                "summary": doc.summary,
                "examples": doc.examples,
            })
            .to_string(),
            None => "null".to_string(),
        }
    }
}

impl Default for WasmCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// The streaming statement splitter (pipes/REPLs): push physical lines, get
/// completed logical statements. `push` returns JSON
/// `{"text":…,"line":N}` or `"null"`; `finish` returns `"null"` or an error
/// object for an unterminated block.
#[wasm_bindgen]
pub struct WasmStatementAccumulator {
    inner: StatementAccumulator,
}

#[wasm_bindgen]
impl WasmStatementAccumulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmStatementAccumulator {
        WasmStatementAccumulator {
            inner: StatementAccumulator::new(),
        }
    }

    pub fn push(&mut self, line: &str) -> String {
        match self.inner.push(line) {
            Some(statement) => {
                json!({ "text": statement.text, "line": statement.line }).to_string()
            }
            None => "null".to_string(),
        }
    }

    #[wasm_bindgen(js_name = isPending)]
    pub fn is_pending(&self) -> bool {
        self.inner.is_pending()
    }

    #[wasm_bindgen(js_name = pendingText)]
    pub fn pending_text(&self) -> String {
        self.inner.pending_text()
    }

    pub fn finish(&mut self) -> String {
        match self.inner.finish() {
            Ok(()) => "null".to_string(),
            Err(error) => error_json(&error).to_string(),
        }
    }
}

impl Default for WasmStatementAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// The full builtin REFERENCE — what the apps' help browser (⌘/) lists. JSON
/// `[{"name":…,"category":…,"signature":…,"summary":…,"examples":[…]}]` in
/// registry order (categories arrive grouped).
#[wasm_bindgen]
pub fn reference() -> String {
    let entries: Vec<_> = anzan::FunctionRegistry::standard()
        .all()
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "category": f.category.heading(),
                "signature": f.signature,
                "summary": f.summary,
                "examples": f.examples,
            })
        })
        .collect();
    json!(entries).to_string()
}

/// The CLI display heuristics, for the ts CLI's pretty mode.
#[wasm_bindgen(js_name = trailingComment)]
pub fn trailing_comment(line: &str) -> Option<String> {
    Calculator::trailing_comment(line)
}

#[wasm_bindgen(js_name = usesProgrammerNotation)]
pub fn uses_programmer_notation(line: &str) -> bool {
    Calculator::uses_programmer_notation(line)
}

fn outcome_json(outcome: Result<EvalOutcome, anzan::EngineError>) -> serde_json::Value {
    match outcome {
        Ok(outcome) => {
            let kind = match &outcome {
                EvalOutcome::Value(_) => "value",
                EvalOutcome::FunctionDefined { .. } => "function",
                EvalOutcome::DataDefined { .. } => "data",
                EvalOutcome::Documentation(_) => "documentation",
                EvalOutcome::Comment(_) => "comment",
            };
            let mut entry = json!({
                "ok": true,
                "kind": kind,
                "description": outcome.to_string(),
                "displayDescription": outcome.display_description(),
            });
            if let Some(block) = outcome.raw_block() {
                entry["rawBlock"] = json!(block);
            }
            entry
        }
        Err(error) => error_json(&error),
    }
}

fn error_json(error: &anzan::EngineError) -> serde_json::Value {
    match error.position() {
        Some(position) => json!({ "ok": false, "error": error.to_string(), "position": position }),
        None => json!({ "ok": false, "error": error.to_string() }),
    }
}
