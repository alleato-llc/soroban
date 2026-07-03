//! Session state: user variables plus the implicit `ans`. Built-in constants
//! live here too, shadowed from assignment by the parser's reserved-name
//! check.
//!
//! In Swift this is a reference type because evaluation can re-enter the
//! calculator; the Rust evaluator threads `&mut EvaluationEnvironment`
//! explicitly, with host re-entry mediated by the Calculator (single-threaded
//! discipline in both worlds). The Swift name avoided colliding with
//! SwiftUI's `@Environment`; kept for cross-referencing the two codebases.

use super::data_type::DataType;
use super::value::{MapEntry, Value};
use crate::ast::{Expression, Parameter, TypeAnnotation};
use crate::BigDecimal;
use std::collections::HashMap;

#[derive(Default)]
pub struct EvaluationEnvironment {
    variables: HashMap<String, Value>,
    /// Result of the most recent successful evaluation.
    ans: Option<Value>,
    /// Bumped on every variable/function mutation (not on `ans`). Lets
    /// callers detect "did this evaluation change session state?" by
    /// comparing two integers instead of snapshotting maps.
    change_count: u64,
    /// The namespace whose body is currently evaluating, so a namespaced
    /// member resolves its siblings unqualified (`Bits::area` calls
    /// `perimeter`, finds `Bits::perimeter`). A stack: nested/cross-namespace
    /// calls push their own home, and a plain global function pushes `None`
    /// so it can't see a caller's siblings. Transient — empty outside
    /// function-body evaluation. Mirrors the cell ResolutionContext's
    /// current-sheet stack.
    namespace_context: Vec<Option<String>>,
    /// Keyed by lowercased name — function calls are case-insensitive,
    /// matching the built-in registry. Each name holds a list of OVERLOADS:
    /// at most one fully-untyped definition (redefinition replaces it) plus
    /// any number of typed definitions, distinguished by their parameter type
    /// signature (typed dispatch).
    functions: HashMap<String, Vec<UserFunction>>,
    /// Keyed by lowercased name — constructor calls are case-insensitive,
    /// like functions (with which they share the call namespace; the
    /// evaluator rejects cross-collisions).
    data_types: HashMap<String, DataType>,
    /// Imported namespaces, in import order — their members are reachable
    /// unqualified. Persisted in the workbook (restored after namespaces).
    imports: Vec<String>,
    /// The source line of each `namespace … { … }` evaluated, in order —
    /// replayed on workbook open to re-register the namespace's members.
    /// Reopening appends, so replay reconstructs the accumulated namespace.
    namespace_source_lines: Vec<String>,
}

impl EvaluationEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ans(&self) -> Value {
        self.ans
            .clone()
            .unwrap_or(Value::Number(BigDecimal::zero()))
    }

    pub(crate) fn set_ans(&mut self, value: Value) {
        self.ans = Some(value);
    }

    pub fn change_count(&self) -> u64 {
        self.change_count
    }

    pub(crate) fn current_namespace(&self) -> Option<&str> {
        self.namespace_context.last().and_then(|n| n.as_deref())
    }

    pub(crate) fn enter_namespace(&mut self, namespace: Option<String>) {
        self.namespace_context.push(namespace);
    }

    pub(crate) fn leave_namespace(&mut self) {
        self.namespace_context.pop();
    }

    /// Variable lookup — reserved constants first (case-insensitive),
    /// then user variables (case-sensitive).
    pub fn get(&self, name: &str) -> Option<Value> {
        match name.to_lowercase().as_str() {
            "ans" => Some(self.ans()),
            "pi" | "π" => Some(Value::Number(constants::pi())),
            "tau" | "τ" => Some(Value::Number(constants::tau())),
            "e" => Some(Value::Number(constants::e())),
            "true" => Some(Value::Number(BigDecimal::one())),
            "false" => Some(Value::Number(BigDecimal::zero())),
            "json" => Some(constants::json()),
            "rounding" => Some(constants::rounding()),
            _ => self.variables.get(name).cloned(),
        }
    }

    pub fn set(&mut self, name: &str, value: Value) {
        if self.variables.get(name) != Some(&value) {
            self.variables.insert(name.to_string(), value);
            self.change_count += 1;
        }
    }

    pub(crate) fn remove_variable(&mut self, name: &str) {
        if self.variables.remove(name).is_some() {
            self.change_count += 1;
        }
    }

    /// User-defined variables, for display in the UI.
    pub fn user_variables(&self) -> &HashMap<String, Value> {
        &self.variables
    }

    /// Replaces all user variables wholesale — used when opening a workbook.
    /// Namespace constants (qualified `M::c` names) are owned by namespace
    /// replay, not the flat variable map — preserved across a wholesale
    /// replace; `clear_namespace_variables()` drops stale ones.
    pub fn replace_user_variables(&mut self, new_variables: HashMap<String, Value>) {
        let qualified: Vec<(String, Value)> = self
            .variables
            .iter()
            .filter(|(k, _)| k.contains("::"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        self.variables = new_variables;
        for (k, v) in qualified {
            self.variables.entry(k).or_insert(v);
        }
        self.change_count += 1;
    }

    /// Drops every namespace-qualified constant (`M::c`) — called at session
    /// restore before namespaces replay, so a removed namespace's constants
    /// don't linger (mirrors clearing functions/types).
    pub fn clear_namespace_variables(&mut self) {
        self.variables.retain(|k, _| !k.contains("::"));
        self.change_count += 1;
    }

    // MARK: User-defined functions

    /// A single representative definition for the name (the first) — for
    /// "does a function named X exist", signature display, man(), etc. Typed
    /// dispatch uses `overloads`.
    pub fn function(&self, name: &str) -> Option<&UserFunction> {
        self.functions.get(&name.to_lowercase())?.first()
    }

    /// All overloads for a name, in definition order — used by call dispatch.
    pub fn overloads(&self, name: &str) -> &[UserFunction] {
        self.functions
            .get(&name.to_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Defining over your own function is allowed (iteration); collisions
    /// with built-ins are rejected upstream in the evaluator. A new
    /// definition replaces any existing one with the SAME dispatch signature
    /// (untyped defs share one slot — redefinition replaces); differing typed
    /// signatures coexist.
    pub(crate) fn define_function(&mut self, function: UserFunction) {
        let key = function.name.to_lowercase();
        let list = self.functions.entry(key).or_default();
        let signature = function.dispatch_signature();
        list.retain(|f| f.dispatch_signature() != signature);
        list.push(function);
        self.change_count += 1;
    }

    /// Records the original input line for the MOST RECENTLY defined overload
    /// of the name (the one just defined) — for workbook serialization.
    pub(crate) fn set_function_source(&mut self, source: &str, name: &str) {
        if let Some(list) = self.functions.get_mut(&name.to_lowercase()) {
            if let Some(last) = list.last_mut() {
                last.source = source.to_string();
            }
        }
    }

    /// Name → a representative definition (the first overload). Lossy for
    /// names with several typed overloads; `all_user_functions` is complete.
    pub fn user_functions(&self) -> HashMap<String, UserFunction> {
        self.functions
            .iter()
            .filter_map(|(k, v)| v.first().map(|f| (k.clone(), f.clone())))
            .collect()
    }

    /// Every user function, all overloads — the complete set.
    pub fn all_user_functions(&self) -> Vec<&UserFunction> {
        self.functions.values().flatten().collect()
    }

    pub fn replace_user_functions(&mut self, new_functions: HashMap<String, UserFunction>) {
        self.functions.clear();
        for (key, function) in new_functions {
            self.functions
                .entry(key.to_lowercase())
                .or_default()
                .push(function);
        }
        self.change_count += 1;
    }

    // MARK: User-declared data types

    pub fn data_type(&self, name: &str) -> Option<&DataType> {
        self.data_types.get(&name.to_lowercase())
    }

    /// Redeclaring your own type is allowed (iteration); collisions with
    /// built-ins and functions are rejected upstream in the evaluator.
    pub(crate) fn define_data_type(&mut self, data_type: DataType) {
        self.data_types
            .insert(data_type.name.to_lowercase(), data_type);
        self.change_count += 1;
    }

    /// Records the original input line for workbook serialization (and the
    /// trailing `# doc comment` riding on it).
    pub(crate) fn set_data_type_source(&mut self, source: &str, name: &str) {
        if let Some(t) = self.data_types.get_mut(&name.to_lowercase()) {
            t.source = source.to_string();
        }
    }

    pub fn user_data_types(&self) -> &HashMap<String, DataType> {
        &self.data_types
    }

    pub fn replace_user_data_types(&mut self, new_types: HashMap<String, DataType>) {
        self.data_types = new_types;
        self.change_count += 1;
    }

    // MARK: Namespace imports (docs/MODULES.md 2b)

    pub fn imported_namespaces(&self) -> &[String] {
        &self.imports
    }

    pub fn namespace_sources(&self) -> &[String] {
        &self.namespace_source_lines
    }

    pub(crate) fn record_namespace_source(&mut self, source: &str) {
        self.namespace_source_lines.push(source.to_string());
        self.change_count += 1;
    }

    pub fn clear_namespace_sources(&mut self) {
        self.namespace_source_lines.clear();
    }

    /// The simple member names (types + functions + constants) declared in a
    /// namespace — for the import conflict check. Derives from the values'
    /// names (original case), not the lowercased keys.
    pub(crate) fn member_names(&self, namespace: &str) -> Vec<String> {
        let prefix = format!("{}::", namespace.to_lowercase());
        let mut names: Vec<String> = Vec::new();
        for data_type in self.data_types.values() {
            if data_type.name.to_lowercase().starts_with(&prefix) {
                names.push(data_type.name[prefix.len()..].to_string());
            }
        }
        for list in self.functions.values() {
            for function in list {
                if function.name.to_lowercase().starts_with(&prefix) {
                    names.push(function.name[prefix.len()..].to_string());
                }
            }
        }
        for key in self.variables.keys() {
            if key.to_lowercase().starts_with(&prefix) {
                names.push(key[prefix.len()..].to_string());
            }
        }
        names
    }

    /// Record an import (idempotent — re-importing is a no-op).
    pub(crate) fn add_import(&mut self, namespace: &str) {
        if self
            .imports
            .iter()
            .any(|i| i.eq_ignore_ascii_case(namespace))
        {
            return;
        }
        self.imports.push(namespace.to_string());
        self.change_count += 1;
    }

    pub fn clear_imports(&mut self) {
        self.imports.clear();
    }

    /// Resolve an unqualified name through the imports → its qualified form
    /// when an import provides it as a function, type, or constant. The
    /// import conflict check keeps this unambiguous (no two imports, and no
    /// global, share a name).
    pub(crate) fn imported_name(&self, name: &str) -> Option<String> {
        if name.contains("::") {
            return None;
        }
        for namespace in &self.imports {
            let qualified = format!("{namespace}::{name}");
            if self.function(&qualified).is_some()
                || self.data_type(&qualified).is_some()
                || self.variables.contains_key(&qualified)
            {
                return Some(qualified);
            }
        }
        None
    }
}

/// A user-defined function: `f(x, y) = body`. The body is the parsed AST
/// (composable — it may call other user functions, resolved at call time);
/// `source` keeps the original definition line for saving into workbooks —
/// including any trailing `# doc comment`, which is how user documentation
/// persists with zero extra storage.
#[derive(Debug, Clone, PartialEq)]
pub struct UserFunction {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub body: Expression,
    pub source: String,
}

impl UserFunction {
    pub fn new(name: String, parameters: Vec<Parameter>, body: Expression, source: String) -> Self {
        Self {
            name,
            parameters,
            body,
            source,
        }
    }

    /// The trailing `# …` comment of the definition, if any — the user's own
    /// documentation, shown by man()/the reference window.
    pub fn documentation(&self) -> Option<String> {
        let hash = self.source.find('#')?;
        let text = self.source[hash + 1..].trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }

    /// Display form: `f(x, y)` — or `dist(p: Point)` when params are typed.
    pub fn signature(&self) -> String {
        let params: Vec<String> = self.parameters.iter().map(|p| p.rendered()).collect();
        format!("{}({})", self.name, params.join(", "))
    }

    /// The overload's dispatch key: `None` when every parameter is untyped
    /// (all such definitions of a name share one slot, so redefinition
    /// replaces); otherwise the parameter type sequence, so differing typed
    /// signatures coexist as overloads.
    pub(crate) fn dispatch_signature(&self) -> Option<Vec<Option<TypeAnnotation>>> {
        if self.parameters.iter().any(|p| p.annotation.is_some()) {
            Some(
                self.parameters
                    .iter()
                    .map(|p| p.annotation.clone())
                    .collect(),
            )
        } else {
            None
        }
    }

    /// True if this definition participates in typed dispatch (any param
    /// typed).
    pub fn is_typed(&self) -> bool {
        self.parameters.iter().any(|p| p.annotation.is_some())
    }
}

/// Constants at well past working precision (60 digits).
pub(crate) mod constants {
    use super::{MapEntry, Value};
    use crate::BigDecimal;

    pub(crate) fn pi() -> BigDecimal {
        BigDecimal::parse("3.14159265358979323846264338327950288419716939937510582097494")
            .expect("pi literal")
    }

    pub(crate) fn tau() -> BigDecimal {
        BigDecimal::parse("6.28318530717958647692528676655900576839433879875021164194989")
            .expect("tau literal")
    }

    pub(crate) fn e() -> BigDecimal {
        BigDecimal::parse("2.71828182845904523536028747135266249775724709369995957496697")
            .expect("e literal")
    }

    /// toJson's options namespace: `Json.Pretty` / `Json.Compact` — named
    /// constants instead of a magic boolean (user decision). Plain string
    /// values riding in a constant map, so `toJson(x, "pretty")` works too.
    pub(crate) fn json() -> Value {
        Value::Map(vec![
            MapEntry::new("Pretty", Value::String("pretty".to_string())),
            MapEntry::new("Compact", Value::String("compact".to_string())),
        ])
    }

    /// Decimal()'s rounding namespace: `Rounding.Bankers` /
    /// `Rounding.HalfUp` — same pattern as `Json`.
    pub(crate) fn rounding() -> Value {
        Value::Map(vec![
            MapEntry::new("Bankers", Value::String("bankers".to_string())),
            MapEntry::new("HalfUp", Value::String("halfUp".to_string())),
        ])
    }
}
