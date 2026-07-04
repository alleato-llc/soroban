//! The read-only `History` reflection API — the calculation log as an
//! iterable array of entry handles, so a LOG-LINE expression can inspect
//! what came before (`last(History).value`,
//! `sum(map(entry -> entry.value, History))`).
//!
//! `History` is LOG-ONLY: the resolver hands back the array only on the log
//! path (`in_log`); in a CELL the name is simply unknown, so it degrades to
//! a text label (Anzan's unknownVariable → text rule) rather than erroring —
//! a cell may legitimately hold a header literally named "History". The
//! reason for the gate is reproducibility: the log is GLOBAL session state,
//! not the workbook, so a cell reading it wouldn't be reproducible or
//! portable.
//!
//! The host feeds the log through `LogSource` (host-neutral `LogRecord`s);
//! the app conforms its log store, and engine tests use a stub. Each entry's
//! `kind`/`referencesCells` are DERIVED here by parsing the stored input (a
//! function definition is logged as a value of its signature, so the outcome
//! alone can't classify it — the input parse can).

use anzan::ast::Expression;
use anzan::eval::evaluator::Reentry;
use anzan::{HostObject, LanguageMode, Parser, Value};
use std::rc::Rc;

/// One log line, host-neutral. `value` is the typed result (number/string)
/// when the line produced one, `None` otherwise (errors, comments,
/// definitions).
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// Verbatim expression — intent + replay source.
    pub input: String,
    /// Displayed outcome string — always present.
    pub text: String,
    /// Typed value, only for value-producing lines.
    pub value: Option<Value>,
    pub is_error: bool,
    pub is_comment: bool,
    /// Display-only output (man()/JSON/host dumps) — no value.
    pub is_info: bool,
    pub note: String,
}

/// The host's read interface to the log — the "clean host API underneath".
pub trait LogSource {
    fn records(&self) -> Vec<LogRecord>;
}

/// Builds `History` — an array of entry handles, oldest → newest (a plain
/// `Value::Array`, NOT a host object, so len/`[i]`/map/filter/first/last
/// work natively). Called by the resolver only on the log path; the gate
/// lives there, not here.
pub fn value_from(source: &Rc<dyn LogSource>) -> Value {
    Value::Array(
        source
            .records()
            .into_iter()
            .map(|record| Value::Host(Rc::new(HistoryEntryObject::new(record))))
            .collect(),
    )
}

/// One entry handle: `.input` / `.value` / `.text` / `.kind` / `.isError` /
/// `.referencesCells` / `.note`.
pub struct HistoryEntryObject {
    record: LogRecord,
}

impl HistoryEntryObject {
    pub fn new(record: LogRecord) -> Self {
        Self { record }
    }

    /// The input parsed as an expression (`None` if it doesn't parse — an
    /// error line, say). A leading `=` is tolerated like the log itself.
    fn parsed(&self) -> Option<Expression> {
        let mut line = self.record.input.trim();
        if let Some(stripped) = line.strip_prefix('=') {
            line = stripped;
        }
        Parser::parse(line, LanguageMode::Normal).ok()
    }

    /// "value" | "error" | "comment" | "info" | "function" | "datatype".
    /// Errors/comments/info come from the outcome flags; function/datatype
    /// need the input parse (a definition is logged as a value of its
    /// signature). "info" is display-only output (man()/JSON/host dumps) —
    /// `.value` is `None`.
    fn kind(&self) -> &'static str {
        if self.record.is_error {
            return "error";
        }
        if self.record.is_comment {
            return "comment";
        }
        if self.record.is_info {
            return "info"; // man()/JSON/host dumps — display-only
        }
        match self.parsed() {
            Some(Expression::FunctionDefinition { .. }) => "function",
            Some(Expression::DataDefinition { .. }) => "datatype",
            _ => "value",
        }
    }

    /// Provenance: did this line read a cell / named cell? (the "source"
    /// flag).
    fn references_cells(&self) -> bool {
        self.parsed()
            .is_some_and(|expression| expression.contains_cell_reference())
    }
}

impl HostObject for HistoryEntryObject {
    fn type_name(&self) -> String {
        "LogEntry".to_string()
    }

    fn description(&self) -> String {
        format!("LogEntry({})", self.record.input)
    }

    fn member(&self, _host: Reentry<'_, '_>, name: &str) -> Option<Value> {
        match name {
            "input" => Some(Value::String(self.record.input.clone())),
            "text" => Some(Value::String(self.record.text.clone())),
            // `None` for non-value lines → guard with .kind.
            "value" => self.record.value.clone(),
            "kind" => Some(Value::String(self.kind().to_string())),
            "isError" => Some(Value::bool(self.record.is_error)),
            "referencesCells" => Some(Value::bool(self.references_cells())),
            "note" => Some(Value::String(self.record.note.clone())),
            _ => None,
        }
    }

    // is_equal: the trait default (type_name + description) — the house
    // pattern; the description carries the entry's input.
}
