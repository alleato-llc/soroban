//! What an expression evaluates to. Numbers are the historical core;
//! strings, arrays, and maps arrived with structure support. Values are
//! immutable — there is no element assignment, only whole-variable rebinding
//! — and they nest freely (arrays of maps of arrays…).
//!
//! The canonical `description` re-parses to an equal value, which is how
//! structured variables persist in workbooks (the same string mechanism
//! numbers always used).

use super::fixed_decimal::FixedDecimal;
use super::fixed_int::FixedInt;
use crate::ast::{key_literal, quoted, Expression};
use crate::{BigDecimal, EngineError, LanguageMode, Parser};
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

#[derive(Clone)]
pub enum Value {
    Number(BigDecimal),
    String(String),
    Array(Vec<Value>),
    /// Insertion-ordered key/value pairs. Keys are unique (the parser
    /// rejects duplicates) and case-sensitive, like variables.
    Map(Vec<MapEntry>),
    /// A function as a value — a bare name (`map(double, arr)`) or a lambda
    /// (`map(x -> x * 2, arr)`). Applied by the higher-order builtins.
    Function(FunctionValue),
    /// An instance of a user-declared `data` type — map-shaped (member
    /// access, keys/values, HOFs all work) but tagged with its type and
    /// canonicalized to declaration order by the constructor.
    Record(RecordValue),
    /// A bounded, checked integer (`Int32(v)` / `UInt8(v)`, or
    /// `Int(v, bits)`) — a number with a declared width. Coerces to its
    /// decimal value outside typed arithmetic; typed arithmetic (the mixing
    /// matrix) lives in the evaluator.
    FixedInt(FixedInt),
    /// A bounded, checked fixed-precision decimal
    /// (`Decimal(v, precision, scale)`) — SQL DECIMAL(p,s) / the money type.
    /// Coerces to its decimal value outside typed arithmetic; the mixing
    /// matrix lives in the evaluator.
    FixedDecimal(FixedDecimal),
    /// An opaque, HOST-implemented handle navigated through a uniform
    /// protocol (`.member`/`[…]`/`.method(…)`). Anzan never knows what it is
    /// — the host (e.g. the spreadsheet's Workbook/Worksheet/Cell reflection)
    /// provides the implementations. Absent in hosts that don't inject any
    /// (the CLI).
    Host(Rc<dyn HostObject>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Value({self})")
    }
}

/// The payload of `Value::Record`. Carries what rendering/serialization
/// needs (no back-reference to the full DataType — instances outlive
/// redefinitions).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordValue {
    /// The declaring type's name, as declared ("Person").
    pub type_name: String,
    /// Field values in declaration order.
    pub entries: Vec<MapEntry>,
    /// Fields declared Boolean — held as 1/0, rendered and serialized as
    /// true/false.
    pub boolean_fields: HashSet<String>,
}

impl RecordValue {
    /// "true"/"false" for Boolean fields, canonical text otherwise.
    fn field_text(&self, entry: &MapEntry) -> String {
        if self.boolean_fields.contains(&entry.key) {
            if let Value::Number(flag) = &entry.value {
                return if flag.is_zero() { "false" } else { "true" }.to_string();
            }
        }
        entry.value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    pub key: String,
    pub value: Value,
}

impl MapEntry {
    pub fn new(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}

impl Value {
    /// Comparison results and `true`/`false` are numbers (1/0), matching the
    /// engine's long-standing truthiness convention.
    pub fn bool(holds: bool) -> Value {
        Value::Number(if holds {
            BigDecimal::one()
        } else {
            BigDecimal::zero()
        })
    }

    /// "a number", "an array", … for error messages.
    pub fn kind_name(&self) -> String {
        match self {
            Self::Number(_) => "a number".to_string(),
            Self::String(_) => "a string".to_string(),
            Self::Array(_) => "an array".to_string(),
            Self::Map(_) => "a map".to_string(),
            Self::Function(_) => "a function".to_string(),
            Self::Record(record) => format!("a {}", record.type_name),
            Self::FixedInt(f) => format!("a {}", f.type_name()),
            Self::FixedDecimal(d) => format!("a {}", d.type_name()),
            Self::Host(object) => format!("a {}", object.type_name()),
        }
    }

    /// True for a `data` record instance — the trigger for operator-overload
    /// lookup (plain numeric/string math skips it).
    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    /// The numeric payload, or a type error naming the context:
    /// "expected a number for ^, got an array".
    pub fn as_number(&self, context: &str) -> Result<BigDecimal, EngineError> {
        match self {
            Self::Number(value) => Ok(value.clone()),
            // A fixed-width int reads as its numeric value outside typed
            // arithmetic (comparison, truthiness, numeric functions). Typed
            // arithmetic — the mixing matrix + checked overflow — is
            // intercepted in the evaluator.
            Self::FixedInt(f) => Ok(f.decimal()),
            Self::FixedDecimal(d) => Ok(d.value.clone()),
            _ => Err(EngineError::domain(format!(
                "expected a number for {context}, got {}",
                self.kind_name()
            ))),
        }
    }

    /// Numbers carried by this value, arrays flattened recursively — how
    /// numeric functions consume structured arguments (`sum(arr)` behaves
    /// like `sum(A:1..A:9)`). Strings and maps don't coerce.
    pub fn flattened_numbers(&self, function: &str) -> Result<Vec<BigDecimal>, EngineError> {
        match self {
            Self::Number(value) => Ok(vec![value.clone()]),
            Self::FixedInt(f) => Ok(vec![f.decimal()]),
            Self::FixedDecimal(d) => Ok(vec![d.value.clone()]),
            Self::Array(items) => {
                let mut numbers = Vec::with_capacity(items.len());
                for item in items {
                    numbers.extend(item.flattened_numbers(function)?);
                }
                Ok(numbers)
            }
            Self::String(_)
            | Self::Map(_)
            | Self::Function(_)
            | Self::Record(_)
            | Self::Host(_) => Err(EngineError::domain(format!(
                "{function}() works on numbers — got {}",
                self.kind_name()
            ))),
        }
    }

    /// Map/record field lookup (case-sensitive, like variables).
    pub fn map_value(&self, key: &str) -> Option<&Value> {
        let entries = match self {
            Self::Map(entries) => entries,
            Self::Record(record) => &record.entries,
            _ => return None,
        };
        entries.iter().find(|e| e.key == key).map(|e| &e.value)
    }
}

impl PartialEq for Value {
    /// Deep equality. Maps compare order-insensitively — `{a: 1, b: 2}`
    /// equals `{b: 2, a: 1}` — because entry order is presentation, not
    /// data.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => a == b,
            (Self::Map(a), Self::Map(b)) => {
                a.len() == b.len()
                    && a.iter().all(|entry| {
                        b.iter().find(|e| e.key == entry.key).map(|e| &e.value)
                            == Some(&entry.value)
                    })
            }
            (Self::Function(a), Self::Function(b)) => a == b,
            // Entries compare in order — constructors canonicalize to
            // declaration order, so equal records have equal layouts.
            (Self::Record(a), Self::Record(b)) => a == b,
            (Self::Host(a), Self::Host(b)) => a.is_equal(b.as_ref()),
            // Fixed-width ints compare by numeric value — `Int8(5) == 5` and
            // `Int8(5) == Int16(5)` are both true (it's the number 5).
            (Self::FixedInt(a), Self::FixedInt(b)) => a.value == b.value,
            (Self::FixedInt(a), Self::Number(b)) => a.decimal() == *b,
            (Self::Number(a), Self::FixedInt(b)) => *a == b.decimal(),
            // Fixed-precision decimals compare by numeric value too.
            (Self::FixedDecimal(a), Self::FixedDecimal(b)) => a.value == b.value,
            (Self::FixedDecimal(a), Self::Number(b)) => a.value == *b,
            (Self::Number(a), Self::FixedDecimal(b)) => *a == b.value,
            _ => false,
        }
    }
}

/// A host-implemented value that Anzan navigates without understanding:
/// member access (`.name`), indexing (`[0]` / `["Budget"]`), and method
/// calls (`.cell("A", 2)`) all route here. The host returns plain `Value`s
/// (often immutable snapshots), keeping Anzan ignorant of grids/sheets/
/// files. Default implementations make every capability opt-in.
///
/// `Any` is a supertrait so hosts can recover their own concrete handles —
/// the mutation API resolves `updateCell(cell("A", 1), …)`'s first argument
/// back to a cell target by downcasting (the Swift side's `as? CellObject`).
pub trait HostObject: std::any::Any {
    /// For `kind_name` / error messages — e.g. "Worksheet".
    fn type_name(&self) -> String;
    /// Canonical display (need not re-parse — host handles aren't literals).
    fn description(&self) -> String;
    /// Navigation receives the re-entry pair — a host member read (a cell's
    /// `.value`) evaluates formulas against the same environment.
    fn member(&self, _host: crate::eval::evaluator::Reentry<'_, '_>, _name: &str) -> Option<Value> {
        None
    }
    fn index(&self, _host: crate::eval::evaluator::Reentry<'_, '_>, _key: &Value) -> Option<Value> {
        None
    }
    fn call(
        &self,
        _host: crate::eval::evaluator::Reentry<'_, '_>,
        method: &str,
        _arguments: &[Value],
    ) -> Result<Value, EngineError> {
        Err(EngineError::domain(format!(
            "{} has no method '{method}'",
            self.type_name()
        )))
    }
    /// Default: compare by display — host handles are read-only snapshots,
    /// so equal display means equal state.
    fn is_equal(&self, other: &dyn HostObject) -> bool {
        self.type_name() == other.type_name() && self.description() == other.description()
    }
}

impl fmt::Display for Value {
    /// Canonical, re-parseable rendering: `[1, 2]`,
    /// `{name: "Ada", age: 36}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(value) => write!(f, "{value}"),
            Self::String(text) => write!(f, "{}", quoted(text)),
            Self::Array(items) => {
                let body: Vec<String> = items.iter().map(|i| i.to_string()).collect();
                write!(f, "[{}]", body.join(", "))
            }
            Self::Map(entries) => {
                let body: Vec<String> = entries
                    .iter()
                    .map(|e| format!("{}: {}", key_literal(&e.key), e.value))
                    .collect();
                write!(f, "{{{}}}", body.join(", "))
            }
            Self::Function(function) => write!(f, "{function}"),
            Self::Record(record) => {
                // Constructor-call form — re-parses to an equal record while
                // the type is defined. Field names are identifiers, so keys
                // print bare; Boolean fields print true/false.
                let body: Vec<String> = record
                    .entries
                    .iter()
                    .map(|e| format!("{}: {}", e.key, record.field_text(e)))
                    .collect();
                write!(f, "{}({})", record.type_name, body.join(", "))
            }
            Self::FixedInt(fixed) => write!(f, "{}", fixed.description()),
            Self::FixedDecimal(d) => write!(f, "{}", d.description()),
            Self::Host(object) => write!(f, "{}", object.description()),
        }
    }
}

impl Value {
    /// A clean, human-facing rendering: identical to `description` EXCEPT a
    /// fixed-width int shows its plain integer (`343353`) and a
    /// fixed-precision decimal its scale-padded value (`10.50`) instead of
    /// the constructor call. `to_string` stays the canonical, re-parseable
    /// form — what persists, recalls, and copies (so the *type* survives a
    /// round trip); this is only what the log and the CLI ECHO. Recurses so
    /// fixed values nested in arrays/maps read cleanly too.
    pub fn display_description(&self) -> String {
        match self {
            Self::Number(_)
            | Self::String(_)
            | Self::Function(_)
            | Self::Record(_)
            | Self::Host(_) => self.to_string(),
            Self::Array(items) => {
                let body: Vec<String> = items.iter().map(|i| i.display_description()).collect();
                format!("[{}]", body.join(", "))
            }
            Self::Map(entries) => {
                let body: Vec<String> = entries
                    .iter()
                    .map(|e| format!("{}: {}", key_literal(&e.key), e.value.display_description()))
                    .collect();
                format!("{{{}}}", body.join(", "))
            }
            Self::FixedInt(f) => f.value.to_string(),
            Self::FixedDecimal(d) => d.text(),
        }
    }

    /// Bare text for concatenation and cell display — strings without their
    /// quotes; everything else its clean (`display_description`) form, so a
    /// fixed-width int concatenates as its plain number, not `Int32(…)`.
    pub fn display_text(&self) -> String {
        if let Self::String(text) = self {
            return text.clone();
        }
        self.display_description()
    }

    /// True if this value embeds a host reflection handle (`Workbook`, a
    /// `History` entry, …) anywhere. Such handles render with
    /// NON-re-parseable descriptions (`Workbook(…)`, `[LogEntry(…)]`), so a
    /// result carrying one is display-only — it must not be recalled or
    /// treated as a value (the same reason cells reject host/array results).
    pub fn contains_host(&self) -> bool {
        match self {
            Self::Host(_) => true,
            Self::Array(items) => items.iter().any(Value::contains_host),
            Self::Map(entries) => entries.iter().any(|e| e.value.contains_host()),
            Self::Number(_)
            | Self::String(_)
            | Self::Function(_)
            | Self::Record(_)
            | Self::FixedInt(_)
            | Self::FixedDecimal(_) => false,
        }
    }
}

impl Value {
    /// Parses a persisted variable value back into a Value: the fast numeric
    /// path first (every pre-structures workbook), then literal folding for
    /// `[…]`/`{…}`/`"…"` forms. `None` for anything that isn't a pure
    /// literal — persisted values never contain references or calls.
    pub fn parsing(text: &str) -> Option<Self> {
        if let Some(number) = BigDecimal::parse(text) {
            return Some(Self::Number(number));
        }
        let expression = Parser::parse(text, LanguageMode::Normal).ok()?;
        Self::literal(&expression)
    }

    /// Folds an AST that consists only of literals (numbers, strings,
    /// arrays, maps, and negated numbers); `None` if anything needs
    /// evaluation.
    pub(crate) fn literal(expression: &Expression) -> Option<Value> {
        match expression {
            Expression::Number(value) => Some(Self::Number(value.clone())),
            Expression::StringLiteral(text) => Some(Self::String(text.clone())),
            Expression::UnaryMinus(inner) => {
                if let Expression::Number(value) = inner.as_ref() {
                    Some(Self::Number(-value))
                } else {
                    None
                }
            }
            Expression::ArrayLiteral(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(Self::literal(item)?);
                }
                Some(Self::Array(values))
            }
            Expression::MapLiteral(entries) => {
                let mut folded = Vec::with_capacity(entries.len());
                for entry in entries {
                    folded.push(MapEntry::new(
                        entry.key.clone(),
                        Self::literal(&entry.value)?,
                    ));
                }
                Some(Self::Map(folded))
            }
            Expression::Lambda { parameters, body } => {
                // Persisted lambdas come back capture-free (captured locals
                // can't serialize); globals keep resolving at call time.
                Some(Self::Function(FunctionValue::lambda(
                    parameters.clone(),
                    body.as_ref().clone(),
                )))
            }
            Expression::Variable(name)
                if crate::eval::registry::FunctionRegistry::standard().contains(name) =>
            {
                // A persisted builtin reference ("f = abs" saved as "abs").
                // References to USER functions can't fold here — they load
                // separately — and are dropped; lambdas cover that need.
                Some(Self::Function(FunctionValue::builtin(name.clone())))
            }
            _ => None,
        }
    }
}

/// A callable value. Bare names stay symbolic (re-resolved at call time, so
/// `f = double` then redefining `double` follows the new definition);
/// lambdas carry their AST plus captured locals.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionValue {
    pub kind: FunctionValueKind,
    /// Locals visible where a lambda was created (closure-by-value).
    /// Always empty for named references.
    pub captures: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionValueKind {
    /// A registry builtin, by name.
    Builtin(String),
    /// A user-defined function (resolved snapshot for display; calls
    /// re-resolve by name).
    User(String),
    /// `x -> x * 2` — parameters + body, with captured locals.
    Lambda {
        parameters: Vec<String>,
        body: Expression,
    },
}

impl FunctionValue {
    pub fn builtin(name: String) -> Self {
        Self {
            kind: FunctionValueKind::Builtin(name),
            captures: Vec::new(),
        }
    }

    pub fn user(name: String) -> Self {
        Self {
            kind: FunctionValueKind::User(name),
            captures: Vec::new(),
        }
    }

    pub fn lambda(parameters: Vec<String>, body: Expression) -> Self {
        Self {
            kind: FunctionValueKind::Lambda { parameters, body },
            captures: Vec::new(),
        }
    }

    pub fn lambda_with_captures(
        parameters: Vec<String>,
        body: Expression,
        captures: Vec<(String, Value)>,
    ) -> Self {
        Self {
            kind: FunctionValueKind::Lambda { parameters, body },
            captures,
        }
    }
}

impl fmt::Display for FunctionValue {
    /// Named references print as the name (re-parses to the same reference);
    /// lambdas print re-parseable source.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            FunctionValueKind::Builtin(name) | FunctionValueKind::User(name) => {
                write!(f, "{name}")
            }
            FunctionValueKind::Lambda { parameters, body } => {
                write!(f, "({}) -> {}", parameters.join(", "), body.source_text())
            }
        }
    }
}
