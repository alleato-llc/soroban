//! Direct unit coverage for the `Value` type — rendering (`Display` /
//! `display_description` / `display_text`), literal parsing round-trips,
//! numeric coercion, deep equality, and the small predicates
//! (`kind_name` / `is_record` / `map_value` / `contains_host`). These reach
//! the `Host` and `Function` variants that are awkward to hit through
//! `Calculator::evaluate`, and pin the re-parseable-`description` invariant
//! for every variant.

use super::*;
use crate::eval::fixed_decimal::{DecimalRounding, FixedDecimal, MAX_PRECISION};
use crate::eval::fixed_int::FixedInt;
use crate::BigDecimal;
use num_bigint::BigInt;

fn num(n: i64) -> Value {
    Value::Number(BigDecimal::from_int(n))
}

fn fixed_int(value: i64, bits: u32, signed: bool) -> FixedInt {
    FixedInt::new(BigInt::from(value), bits, signed).expect("valid fixed int")
}

fn fixed_decimal(text: &str, precision: i64, scale: i64) -> FixedDecimal {
    FixedDecimal::new(
        BigDecimal::parse(text).expect("valid decimal"),
        precision,
        scale,
        DecimalRounding::Bankers,
    )
    .expect("in range")
}

// A minimal host handle — only `type_name`/`description` are needed; the
// navigation methods keep their opt-out defaults. Exercises the `Host`
// arm of every match without wiring a spreadsheet.
struct StubHost {
    label: String,
}

impl HostObject for StubHost {
    fn type_name(&self) -> String {
        "Widget".to_string()
    }
    fn description(&self) -> String {
        format!("Widget({})", self.label)
    }
}

fn host(label: &str) -> Value {
    Value::Host(Rc::new(StubHost {
        label: label.to_string(),
    }))
}

fn person() -> Value {
    let mut boolean_fields = HashSet::new();
    boolean_fields.insert("active".to_string());
    Value::Record(RecordValue {
        type_name: "Person".to_string(),
        entries: vec![
            MapEntry::new("name", Value::String("Ada".to_string())),
            MapEntry::new("age", num(36)),
            MapEntry::new("active", num(1)),
        ],
        boolean_fields,
    })
}

// MARK: - description (Display) is canonical and re-parseable

#[test]
fn description_renders_each_variant_canonically() {
    assert_eq!(num(42).to_string(), "42");
    assert_eq!(Value::String("hi".to_string()).to_string(), "\"hi\"");
    assert_eq!(Value::Array(vec![num(1), num(2)]).to_string(), "[1, 2]");
    assert_eq!(
        Value::Map(vec![MapEntry::new("a", num(1)), MapEntry::new("b", num(2))]).to_string(),
        "{a: 1, b: 2}"
    );
    assert_eq!(
        Value::FixedInt(fixed_int(255, 32, true)).to_string(),
        "Int32(255)"
    );
    assert_eq!(host("gauge").to_string(), "Widget(gauge)");
}

#[test]
fn record_description_prints_bare_keys_and_boolean_words() {
    // Field names are identifiers (bare keys); the Boolean field renders
    // true/false, not 1/0 — and the whole thing is a constructor call.
    assert_eq!(
        person().to_string(),
        "Person(name: \"Ada\", age: 36, active: true)"
    );

    let mut boolean_fields = HashSet::new();
    boolean_fields.insert("on".to_string());
    let off = Value::Record(RecordValue {
        type_name: "Switch".to_string(),
        entries: vec![MapEntry::new("on", num(0))],
        boolean_fields,
    });
    assert_eq!(off.to_string(), "Switch(on: false)");
}

#[test]
fn map_description_quotes_keys_that_are_not_identifiers() {
    let map = Value::Map(vec![MapEntry::new("has space", num(1))]);
    assert_eq!(map.to_string(), "{\"has space\": 1}");
}

#[test]
fn fixed_decimal_description_round_trips_shortest_form() {
    // Natural-scale, max-precision, banker's → the 1-arg short form.
    assert_eq!(
        Value::FixedDecimal(fixed_decimal("0.5", MAX_PRECISION, 1)).to_string(),
        "Decimal(0.5)"
    );
    // A wider scale than the value's own → the 2-arg form, padded.
    assert_eq!(
        Value::FixedDecimal(fixed_decimal("0.5", MAX_PRECISION, 2)).to_string(),
        "Decimal(0.50, 2)"
    );
    // A declared precision → the full form.
    assert_eq!(
        Value::FixedDecimal(fixed_decimal("10.5", 5, 2)).to_string(),
        "Decimal(10.50, 5, 2)"
    );
}

#[test]
fn fixed_decimal_description_shows_non_default_rounding() {
    let half_up = FixedDecimal::new(
        BigDecimal::parse("1.25").expect("decimal"),
        5,
        2,
        DecimalRounding::HalfUp,
    )
    .expect("in range");
    assert_eq!(
        Value::FixedDecimal(half_up).to_string(),
        "Decimal(1.25, 5, 2, Rounding.HalfUp)"
    );
}

#[test]
fn function_values_render_reparseably() {
    assert_eq!(
        Value::Function(FunctionValue::builtin("abs".to_string())).to_string(),
        "abs"
    );
    assert_eq!(
        Value::Function(FunctionValue::user("double".to_string())).to_string(),
        "double"
    );

    // Lambda bodies render via `source_text` — conservatively parenthesized
    // (the contract is re-parseability, not prettiness).
    let doubled = crate::Parser::parse("x * 2", crate::LanguageMode::Normal).expect("parses");
    let lambda = FunctionValue::lambda(vec!["x".to_string()], doubled);
    assert_eq!(Value::Function(lambda).to_string(), "(x) -> (x * 2)");
}

#[test]
fn debug_delegates_to_display() {
    assert_eq!(format!("{:?}", num(7)), "Value(7)");
}

// MARK: - display_description / display_text (the clean, human-facing form)

#[test]
fn display_description_unwraps_fixed_values() {
    // A fixed-width int shows its plain integer, a fixed decimal its
    // scale-padded value — NOT the constructor call — while numbers/strings
    // are unchanged.
    assert_eq!(
        Value::FixedInt(fixed_int(255, 32, true)).display_description(),
        "255"
    );
    assert_eq!(
        Value::FixedDecimal(fixed_decimal("10.5", 5, 2)).display_description(),
        "10.50"
    );
    assert_eq!(num(3).display_description(), "3");
    assert_eq!(
        Value::String("q".to_string()).display_description(),
        "\"q\""
    );
}

#[test]
fn display_description_recurses_into_arrays_and_maps() {
    let nested = Value::Array(vec![Value::FixedInt(fixed_int(5, 8, true)), num(2)]);
    assert_eq!(nested.display_description(), "[5, 2]");
    let map = Value::Map(vec![MapEntry::new(
        "n",
        Value::FixedInt(fixed_int(9, 16, false)),
    )]);
    assert_eq!(map.display_description(), "{n: 9}");
}

#[test]
fn display_text_strips_string_quotes_only() {
    assert_eq!(Value::String("Ada".to_string()).display_text(), "Ada");
    // Everything else is its display_description — a fixed int as a plain
    // number, so `"n=" + Int8(5)` concatenates cleanly.
    assert_eq!(Value::FixedInt(fixed_int(5, 8, true)).display_text(), "5");
    assert_eq!(num(4).display_text(), "4");
}

// MARK: - parsing / literal folding

#[test]
fn parsing_folds_literal_shapes() {
    assert_eq!(Value::parsing("42"), Some(num(42)));
    assert_eq!(
        Value::parsing("-5"),
        Some(Value::Number(BigDecimal::from_int(-5)))
    );
    assert_eq!(
        Value::parsing("\"hi\""),
        Some(Value::String("hi".to_string()))
    );
    assert_eq!(
        Value::parsing("[1, 2]"),
        Some(Value::Array(vec![num(1), num(2)]))
    );
    assert_eq!(
        Value::parsing("{a: 1}"),
        Some(Value::Map(vec![MapEntry::new("a", num(1))]))
    );
}

#[test]
fn parsing_folds_a_builtin_reference_but_not_a_user_name() {
    // "f = abs" persists as "abs" — a builtin reference folds…
    assert_eq!(
        Value::parsing("abs"),
        Some(Value::Function(FunctionValue::builtin("abs".to_string())))
    );
    // …but a bare non-builtin name is a variable reference: not a literal.
    assert_eq!(Value::parsing("someUserFn"), None);
}

#[test]
fn parsing_folds_a_capture_free_lambda() {
    match Value::parsing("x -> x * 2") {
        Some(Value::Function(FunctionValue {
            kind: FunctionValueKind::Lambda { parameters, .. },
            captures,
        })) => {
            assert_eq!(parameters, vec!["x".to_string()]);
            assert!(
                captures.is_empty(),
                "persisted lambdas come back capture-free"
            );
        }
        other => panic!("expected a lambda function value, got {other:?}"),
    }
}

#[test]
fn parsing_rejects_anything_needing_evaluation() {
    // References and calls never persist as literal values.
    assert_eq!(Value::parsing("1 + 2"), None);
    assert_eq!(Value::parsing("A:1"), None);
    assert_eq!(Value::parsing("sum(1, 2)"), None);
    // A non-numeric unary minus doesn't fold.
    assert_eq!(Value::parsing("-x"), None);
    // Nonsense isn't a literal either.
    assert_eq!(Value::parsing("("), None);
}

#[test]
fn description_round_trips_through_parsing() {
    // The core invariant: description re-parses to an equal value.
    for value in [
        num(42),
        Value::String("hi".to_string()),
        Value::Array(vec![num(1), Value::String("x".to_string())]),
        Value::Map(vec![MapEntry::new("a", num(1)), MapEntry::new("b", num(2))]),
    ] {
        assert_eq!(
            Value::parsing(&value.to_string()),
            Some(value.clone()),
            "{value}"
        );
    }
}

// MARK: - as_number / flattened_numbers coercion

#[test]
fn as_number_coerces_the_numeric_family() {
    assert_eq!(num(7).as_number("+").unwrap(), BigDecimal::from_int(7));
    assert_eq!(
        Value::FixedInt(fixed_int(7, 32, true))
            .as_number("+")
            .unwrap(),
        BigDecimal::from_int(7)
    );
    assert_eq!(
        Value::FixedDecimal(fixed_decimal("2.5", 5, 2))
            .as_number("+")
            .unwrap(),
        BigDecimal::parse("2.5").unwrap()
    );
}

#[test]
fn as_number_names_the_context_and_kind_on_failure() {
    let error = Value::Array(vec![num(1)]).as_number("^").unwrap_err();
    assert_eq!(error.to_string(), "expected a number for ^, got an array");
}

#[test]
fn flattened_numbers_flattens_arrays_and_rejects_non_numbers() {
    let nested = Value::Array(vec![num(1), Value::Array(vec![num(2), num(3)])]);
    assert_eq!(
        nested.flattened_numbers("sum").unwrap(),
        vec![
            BigDecimal::from_int(1),
            BigDecimal::from_int(2),
            BigDecimal::from_int(3)
        ]
    );
    // Fixed values flatten to their numeric value.
    assert_eq!(
        Value::FixedInt(fixed_int(9, 8, false))
            .flattened_numbers("sum")
            .unwrap(),
        vec![BigDecimal::from_int(9)]
    );
    // Strings, maps, functions, records, hosts don't coerce.
    for bad in [
        Value::String("x".to_string()),
        Value::Map(vec![]),
        Value::Function(FunctionValue::builtin("abs".to_string())),
        person(),
        host("g"),
    ] {
        let error = bad.flattened_numbers("sum").unwrap_err();
        assert!(
            error.to_string().contains("sum() works on numbers"),
            "{error}"
        );
    }
}

// MARK: - deep equality

#[test]
fn maps_compare_order_insensitively() {
    let ab = Value::Map(vec![MapEntry::new("a", num(1)), MapEntry::new("b", num(2))]);
    let ba = Value::Map(vec![MapEntry::new("b", num(2)), MapEntry::new("a", num(1))]);
    assert_eq!(ab, ba);
    // Differing length is unequal even when the shared keys match.
    let a_only = Value::Map(vec![MapEntry::new("a", num(1))]);
    assert_ne!(ab, a_only);
    // A differing value at a shared key is unequal.
    let ab2 = Value::Map(vec![MapEntry::new("a", num(1)), MapEntry::new("b", num(9))]);
    assert_ne!(ab, ab2);
}

#[test]
fn a_record_never_equals_a_plain_map() {
    let as_map = Value::Map(vec![
        MapEntry::new("name", Value::String("Ada".to_string())),
        MapEntry::new("age", num(36)),
        MapEntry::new("active", num(1)),
    ]);
    assert_ne!(person(), as_map);
    // Records equal records with equal type + entries.
    assert_eq!(person(), person());
}

#[test]
fn fixed_values_compare_by_numeric_value() {
    // Int8(5) == 5, and Int8(5) == Int16(5) — it's the number 5.
    assert_eq!(Value::FixedInt(fixed_int(5, 8, true)), num(5));
    assert_eq!(num(5), Value::FixedInt(fixed_int(5, 8, true)));
    assert_eq!(
        Value::FixedInt(fixed_int(5, 8, true)),
        Value::FixedInt(fixed_int(5, 16, false))
    );
    // A fixed decimal equals its numeric value both ways.
    let d = Value::FixedDecimal(fixed_decimal("2.5", 5, 2));
    assert_eq!(d, Value::Number(BigDecimal::parse("2.5").unwrap()));
    assert_eq!(Value::Number(BigDecimal::parse("2.5").unwrap()), d);
    assert_eq!(d, Value::FixedDecimal(fixed_decimal("2.5", 10, 4)));
}

#[test]
fn hosts_compare_by_type_and_description() {
    // The default is_equal: same type name + same description.
    assert_eq!(host("a"), host("a"));
    assert_ne!(host("a"), host("b"));
}

#[test]
fn functions_and_cross_type_comparisons() {
    assert_eq!(
        Value::Function(FunctionValue::builtin("abs".to_string())),
        Value::Function(FunctionValue::builtin("abs".to_string()))
    );
    assert_ne!(
        Value::Function(FunctionValue::builtin("abs".to_string())),
        Value::Function(FunctionValue::user("abs".to_string()))
    );
    // Unrelated kinds are never equal.
    assert_ne!(num(1), Value::String("1".to_string()));
    assert_ne!(Value::Array(vec![num(1)]), Value::Map(vec![]));
    assert_ne!(host("a"), num(1));
}

// MARK: - predicates

#[test]
fn kind_name_names_every_variant() {
    assert_eq!(num(1).kind_name(), "a number");
    assert_eq!(Value::String("x".to_string()).kind_name(), "a string");
    assert_eq!(Value::Array(vec![]).kind_name(), "an array");
    assert_eq!(Value::Map(vec![]).kind_name(), "a map");
    assert_eq!(
        Value::Function(FunctionValue::builtin("abs".to_string())).kind_name(),
        "a function"
    );
    assert_eq!(person().kind_name(), "a Person");
    assert_eq!(Value::FixedInt(fixed_int(1, 8, true)).kind_name(), "a Int8");
    assert_eq!(
        Value::FixedDecimal(fixed_decimal("1.0", 5, 2)).kind_name(),
        "a Decimal(5, 2)"
    );
    assert_eq!(host("g").kind_name(), "a Widget");
}

#[test]
fn is_record_is_true_only_for_records() {
    assert!(person().is_record());
    assert!(!Value::Map(vec![]).is_record());
    assert!(!num(1).is_record());
}

#[test]
fn map_value_reads_maps_and_records_and_misses_cleanly() {
    assert_eq!(
        person().map_value("name"),
        Some(&Value::String("Ada".to_string()))
    );
    let map = Value::Map(vec![MapEntry::new("k", num(9))]);
    assert_eq!(map.map_value("k"), Some(&num(9)));
    // Missing key and non-map value → None (no panic).
    assert_eq!(map.map_value("absent"), None);
    assert_eq!(num(1).map_value("k"), None);
}

#[test]
fn bool_constructs_one_and_zero() {
    assert_eq!(Value::bool(true), num(1));
    assert_eq!(Value::bool(false), num(0));
}

#[test]
fn contains_host_finds_handles_at_any_depth() {
    assert!(host("g").contains_host());
    assert!(Value::Array(vec![num(1), host("g")]).contains_host());
    assert!(Value::Map(vec![MapEntry::new("h", host("g"))]).contains_host());
    // Nested but host-free values are clean.
    assert!(
        !Value::Array(vec![num(1), Value::Map(vec![MapEntry::new("a", num(2))])]).contains_host()
    );
    assert!(!person().contains_host());
    assert!(!Value::FixedInt(fixed_int(1, 8, true)).contains_host());
}
