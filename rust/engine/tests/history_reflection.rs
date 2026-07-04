//! The read-only `History` reflection API — the calculation log as an
//! iterable array of entry handles. Log-only: live on the log path, a text
//! label in a cell. Entry `kind`/`referencesCells` are derived from the
//! stored input. The port of HistoryReflectionTests.swift, case by case.

use soroban_engine::history_reflection::{HistoryEntryObject, LogRecord, LogSource};
use soroban_engine::{BigDecimal, Calculator, EvalOutcome, SheetStore, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// A stand-in log so the engine suite can exercise History without the app.
struct StubLog {
    records: Vec<LogRecord>,
}

impl LogSource for StubLog {
    fn records(&self) -> Vec<LogRecord> {
        self.records.clone()
    }
}

/// Keeps the store (the resolver captures it weakly) and log alive for a
/// test.
struct Harness {
    calculator: Rc<RefCell<Calculator>>,
    _store: SheetStore,
}

impl Harness {
    fn new(records: Vec<LogRecord>) -> Self {
        let calculator = Rc::new(RefCell::new(Calculator::new()));
        let store = SheetStore::new(Rc::clone(&calculator));
        store.set_log_source(Rc::new(StubLog { records }));
        Self {
            calculator,
            _store: store,
        }
    }

    fn eval(&self, input: &str) -> Value {
        match self
            .calculator
            .borrow_mut()
            .evaluate(input)
            .unwrap_or_else(|error| panic!("'{input}' failed: {error:?}"))
        {
            EvalOutcome::Value(value) => value,
            other => panic!("'{input}' produced a non-value outcome: {other:?}"),
        }
    }
}

fn num(value: i64) -> Value {
    Value::Number(BigDecimal::from_int(value))
}

fn string(text: &str) -> Value {
    Value::String(text.to_string())
}

/// The all-defaults record — customize per test via struct update syntax.
fn record(input: &str) -> LogRecord {
    LogRecord {
        input: input.to_string(),
        text: input.to_string(),
        value: None,
        is_error: false,
        is_comment: false,
        is_info: false,
        note: String::new(),
    }
}

// MARK: The array shape

#[test]
fn history_is_an_iterable_indexable_array() {
    let h = Harness::new(vec![
        LogRecord {
            value: Some(num(1)),
            ..record("1")
        },
        LogRecord {
            value: Some(num(2)),
            ..record("ans + 1")
        },
        LogRecord {
            value: Some(num(86)),
            ..record("ans * 43")
        },
    ]);
    assert_eq!(h.eval("len(History)"), num(3));
    assert_eq!(h.eval("History[0].value"), num(1));
    assert_eq!(h.eval("History[2].value"), num(86));
    // Last entry — "ans, but as an entry". Plain arrays are 0-based with no
    // negative indexing, so use the existing last()/first() builtins.
    assert_eq!(h.eval("last(History).input"), string("ans * 43"));
    assert_eq!(h.eval("first(History).value"), num(1));
    // Iterable: map/filter/reduce work because it's a real array. (`e` is
    // reserved — Euler's constant — so the lambda parameter is `entry`.)
    assert_eq!(h.eval("sum(map(entry -> entry.value, History))"), num(89));
}

// MARK: Entry fields

#[test]
fn entry_exposes_input_value_text_and_note() {
    // 8% logs the value 0.08; compare it inside Anzan to dodge host-side
    // BigDecimal-literal construction.
    let h = Harness::new(vec![LogRecord {
        value: Some(Value::Number(BigDecimal::parse("0.08").unwrap())),
        text: "0.08".to_string(),
        note: "tax".to_string(),
        ..record("8%")
    }]);
    assert_eq!(h.eval("History[0].input"), string("8%"));
    assert_eq!(h.eval("History[0].value == 0.08"), num(1));
    assert_eq!(h.eval("History[0].text"), string("0.08"));
    assert_eq!(h.eval("History[0].note"), string("tax"));
}

#[test]
fn string_results_carry_their_type() {
    let h = Harness::new(vec![LogRecord {
        value: Some(string("Q1")),
        ..record(r#"="Q" + 1"#)
    }]);
    assert_eq!(h.eval("History[0].value"), string("Q1"));
}

// MARK: kind derivation (input-parse + outcome flags)

#[test]
fn kind_classifies_each_line() {
    let h = Harness::new(vec![
        LogRecord {
            value: Some(num(108)),
            ..record("100 + 8")
        }, // value
        LogRecord {
            text: "#ERR".to_string(),
            is_error: true,
            ..record("1 / 0")
        }, // error
        LogRecord {
            text: "a note".to_string(),
            is_comment: true,
            ..record("# a note")
        }, // comment
        LogRecord {
            value: Some(string("f(x)")),
            ..record("f(x) = x^2")
        }, // function (logged as a value!)
        record("data Point { x: Number, y: Number }"), // datatype
        LogRecord {
            text: "[LogEntry(1)]".to_string(),
            is_info: true,
            ..record("History")
        }, // display-only (host dump)
    ]);
    assert_eq!(h.eval("History[0].kind"), string("value"));
    assert_eq!(h.eval("History[1].kind"), string("error"));
    assert_eq!(h.eval("History[2].kind"), string("comment"));
    assert_eq!(h.eval("History[3].kind"), string("function"));
    assert_eq!(h.eval("History[4].kind"), string("datatype"));
    assert_eq!(h.eval("History[5].kind"), string("info"));
    // isError is sugar for kind == "error".
    assert_eq!(h.eval("History[1].isError"), num(1));
    assert_eq!(h.eval("History[0].isError"), num(0));
}

// MARK: referencesCells provenance

#[test]
fn references_cells_flags_workbook_dependence() {
    let h = Harness::new(vec![
        LogRecord {
            value: Some(num(15)),
            ..record("A:1 + 10")
        },
        LogRecord {
            value: Some(num(4)),
            ..record("2 + 2")
        },
        LogRecord {
            value: Some(num(8)),
            ..record("'Projected Rate' * 2")
        },
    ]);
    assert_eq!(h.eval("History[0].referencesCells"), num(1));
    assert_eq!(h.eval("History[1].referencesCells"), num(0));
    // Named-cell references count too.
    assert_eq!(h.eval("History[2].referencesCells"), num(1));
}

// MARK: containsHost (why a host result is logged display-only)

#[test]
fn contains_host_detects_reflection_handles() {
    let handle = Value::Host(Rc::new(HistoryEntryObject::new(LogRecord {
        value: Some(num(1)),
        ..record("x")
    })));
    assert!(!num(1).contains_host());
    assert!(!Value::Array(vec![num(1), num(2)]).contains_host());
    // A plain string isn't a host.
    assert!(!string("[LogEntry(x)]").contains_host());
    assert!(handle.contains_host());
    // A handle nested anywhere → true (this is the `History` dump case).
    assert!(Value::Array(vec![num(1), handle]).contains_host());
}

// MARK: The log-only gate

#[test]
fn history_is_log_only() {
    let h = Harness::new(vec![LogRecord {
        value: Some(num(1)),
        ..record("1")
    }]);
    // On the log path the array resolves; in a cell it's None (→ text
    // label), while Workbook reflection stays available in both.
    let calculator = h.calculator.borrow();
    let resolve = calculator.resolvers.host_value.as_ref().unwrap();
    assert!(resolve("History", true).is_some());
    assert!(resolve("History", false).is_none());
    assert!(resolve("Workbook", false).is_some());
}

#[test]
fn history_unknown_without_a_log_source() {
    // No log source wired → History is simply unknown, even on the log path.
    let calculator = Rc::new(RefCell::new(Calculator::new()));
    let _store = SheetStore::new(Rc::clone(&calculator));
    let calculator = calculator.borrow();
    let resolve = calculator.resolvers.host_value.as_ref().unwrap();
    assert!(resolve("History", true).is_none());
}
