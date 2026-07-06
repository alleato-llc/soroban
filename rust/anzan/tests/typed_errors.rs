//! Typed-error EQUALITY — the contracts the shared Gherkin scenarios can't
//! express (they only match message substrings). Ports the error tables from
//! Swift's EvaluatorTests / UserFunctionTests / CellReferenceSyntaxTests and
//! pins `Display` formatting including caret columns. (Lexer/parser POSITION
//! equality lives beside the lexer/parser in src — `records_positions`,
//! `rejects_unknown_characters`, `errors_carry_positions`.)

use anzan::{Calculator, EngineError};

fn error_of(source: &str) -> EngineError {
    Calculator::new()
        .evaluate(source)
        .expect_err("expected a failure")
}

#[test]
fn surfaces_typed_errors() {
    let cases: &[(&str, EngineError)] = &[
        ("1 / 0", EngineError::DivisionByZero),
        (
            "y + 1",
            EngineError::UnknownVariable {
                name: "y".to_string(),
            },
        ),
        (
            "nope(1)",
            EngineError::UnknownFunction {
                name: "nope".to_string(),
            },
        ),
        (
            "abs(1, 2)",
            EngineError::ArityMismatch {
                function: "abs".to_string(),
                expected: "1".to_string(),
                got: 2,
            },
        ),
        ("sqrt(-1)", EngineError::domain("sqrt of a negative number")),
        ("ln(0)", EngineError::domain("ln needs a positive argument")),
        (
            "fact(1.5)",
            EngineError::domain("fact() needs a non-negative integer"),
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&error_of(source), expected, "for '{source}'");
    }
}

#[test]
fn user_function_arity_is_a_typed_mismatch() {
    let mut calculator = Calculator::new();
    calculator.evaluate("f(x) = x * 2").expect("defines");
    assert_eq!(
        calculator.evaluate("f(1, 2)").expect_err("arity failure"),
        EngineError::ArityMismatch {
            function: "f".to_string(),
            expected: "1".to_string(),
            got: 2,
        }
    );
}

#[test]
fn builtins_are_protected_with_the_exact_message() {
    let mut calculator = Calculator::new();
    assert_eq!(
        calculator.evaluate("abs(x) = x").expect_err("protected"),
        EngineError::domain("'abs' is a built-in function and can't be redefined")
    );
    // Reserved constants can't become functions either.
    assert!(calculator.evaluate("pi(x) = x").is_err());
}

#[test]
fn cell_references_without_a_sheet_fail_cleanly() {
    // The anzan crate has no grid — an unwired resolver must be a clean,
    // typed error, never a panic (the CLI depends on this).
    assert_eq!(
        error_of("A:1 + 1"),
        EngineError::domain("no sheet available for A:1")
    );
}

#[test]
fn display_renders_human_readable_messages_with_caret_columns() {
    // Hosts print `Display`; lex/parse errors speak in 1-based columns while
    // the payload keeps the 0-based character offset for the caret.
    assert_eq!(error_of("1 / 0").to_string(), "division by zero");

    let lex = error_of("1 @ 2");
    assert_eq!(
        lex,
        EngineError::LexError {
            message: "unexpected character '@'".to_string(),
            position: 2,
        }
    );
    assert_eq!(
        lex.to_string(),
        "syntax error at column 3: unexpected character '@'"
    );
    assert_eq!(lex.position(), Some(2));

    let parse = error_of("(1 + 2");
    assert_eq!(
        parse,
        EngineError::ParseError {
            message: "expected ')'".to_string(),
            position: 6,
        }
    );
    assert_eq!(parse.to_string(), "parse error at column 7: expected ')'");
    assert_eq!(parse.position(), Some(6));

    // Non-positional errors have no caret.
    assert_eq!(error_of("1 / 0").position(), None);
}

#[test]
fn arity_message_pluralizes() {
    // "expects 1 argument" vs "expects 3 arguments" — cosmetic, but hosts
    // show it verbatim.
    assert_eq!(
        error_of("abs(1, 2)").to_string(),
        "abs() expects 1 argument, got 2"
    );
}

#[test]
fn higher_order_misuse_fails_not_panics() {
    // Swift's HigherOrderTests.errorsAreTyped — every misuse is an Err.
    let cases = [
        "map(5, [1])",             // not a function
        "map(x -> x, 5)",          // not an array
        "filter(x -> \"a\", [1])", // predicate not numeric
        "sum(x -> x)",             // functions aren't numbers
        "(x -> x)[0]",             // not indexable
    ];
    for source in cases {
        assert!(
            Calculator::new().evaluate(source).is_err(),
            "'{source}' should fail"
        );
    }
    // Lambda arity, and runaway recursion through variables.
    let mut calculator = Calculator::new();
    calculator.evaluate("f = x -> x").expect("binds");
    assert!(calculator.evaluate("f(1, 2)").is_err());
    calculator.evaluate("r = x -> r(x)").expect("binds");
    assert!(calculator.evaluate("r(1)").is_err());
}

#[test]
fn from_json_depth_cap_is_an_error_not_a_crash() {
    // 300 unclosed arrays sails past the 256-level cap — the Swift side
    // pinned a SIGBUS here; the cap must answer with a typed error.
    let bomb = format!("fromJson(\"{}\")", "[".repeat(300));
    assert!(Calculator::new().evaluate(&bomb).is_err());
}

// MARK: Operator application (apply_op / compare / subscript_value)

#[test]
fn numeric_operators_reject_non_numeric_operands() {
    // Every operator but `+` (which concatenates against a string) needs
    // numbers; the error names the operator symbol and the offending kind.
    assert_eq!(
        error_of("\"a\" - 1"),
        EngineError::domain("expected a number for -, got a string")
    );
    assert_eq!(
        error_of("[1] * 2"),
        EngineError::domain("expected a number for *, got an array")
    );
    assert_eq!(
        error_of("\"a\" ^ 2"),
        EngineError::domain("expected a number for ^, got a string")
    );
    // `+` against a non-string, non-numeric operand is still a type error
    // (concatenation needs a string on one side).
    assert!(error_of("[1] + 2")
        .to_string()
        .contains("expected a number for +"));
}

#[test]
fn ordering_comparisons_require_numbers() {
    // `==`/`!=` work on any values, but `< > <= >=` coerce to numbers.
    assert_eq!(
        error_of("\"a\" < 1"),
        EngineError::domain("expected a number for <, got a string")
    );
    assert_eq!(
        error_of("[1] >= [2]"),
        EngineError::domain("expected a number for >=, got an array")
    );
}

#[test]
fn truthiness_requires_a_number() {
    // if()'s condition (and reduction bounds) must be numeric.
    assert_eq!(
        error_of("if(\"x\", 1, 2)"),
        EngineError::domain("expected a number for the if() condition, got a string")
    );
}

#[test]
fn subscript_type_errors_are_precise() {
    // A map/record subscripted by a non-string key.
    assert_eq!(
        error_of("{a: 1}[0]"),
        EngineError::domain("map keys are strings — e.g. m[\"name\"], got a number")
    );
    // A missing key in a map.
    assert_eq!(
        error_of("{a: 1}[\"b\"]"),
        EngineError::domain("no key 'b' in map")
    );
    // Indexing a plain number isn't allowed.
    assert_eq!(
        error_of("5[0]"),
        EngineError::domain("a number can't be indexed")
    );
    // Out-of-range array/string indices report the container size.
    assert_eq!(
        error_of("[1, 2][5]"),
        EngineError::domain("index 5 is out of range (array has 2 elements)")
    );
    assert_eq!(
        error_of("\"ab\"[9]"),
        EngineError::domain("index 9 is out of range (string has 2 characters)")
    );
}

#[test]
fn record_subscript_missing_field_lists_the_fields() {
    let mut calculator = Calculator::new();
    calculator
        .evaluate("data Point { x: Number, y: Number }")
        .expect("declares");
    calculator
        .evaluate("p = Point(x: 1, y: 2)")
        .expect("constructs");
    let error = calculator
        .evaluate("p[\"z\"]")
        .expect_err("no such field")
        .to_string();
    assert!(error.contains("Point has no field 'z'"), "{error}");
    assert!(error.contains("x, y"), "lists the real fields: {error}");
}

// MARK: Fixed-width integer / fixed-precision decimal mixing matrix

#[test]
fn fixed_int_mixing_matrix_errors() {
    // Signed and unsigned never combine.
    assert!(error_of("Int8(1) + UInt8(1)")
        .to_string()
        .contains("signed and unsigned never combine"));
    // A fractional plain number can't adopt a fixed-width type.
    assert!(error_of("Int8(1) + 1.5")
        .to_string()
        .contains("needs whole numbers"));
    // Overflow errors rather than wraps.
    assert!(error_of("Int8(127) + Int8(1)")
        .to_string()
        .contains("out of range"));
    // Cross-family: a decimal can't combine with a fixed-width int.
    assert!(error_of("Int8(1) + Decimal(1.5)")
        .to_string()
        .contains("with a fixed-width integer"));
    // A fixed-width base needs a non-negative integer exponent.
    assert_eq!(
        error_of("Int8(2) ^ -1"),
        EngineError::domain("a fixed-width base needs a non-negative integer exponent")
    );
}

#[test]
fn fixed_decimal_mixing_matrix_errors() {
    // Different rounding modes never reconcile.
    assert!(
        error_of("Decimal(1.0, 5, 2) + Decimal(1.0, 5, 2, Rounding.HalfUp)")
            .to_string()
            .contains("different rounding")
    );
    // Power is unsupported on decimals (`^` is power in normal mode). Modulo
    // shares this match arm but is unreachable via the parser — normal `%` is
    // postfix percent and programmer `%` lowers to the `mod` builtin (which
    // coerces the decimal to a plain number), so no `BinaryOperator::Modulo`
    // ever reaches `apply_binary`; only power exercises the arm.
    assert!(error_of("Decimal(2.0, 5, 2) ^ 2")
        .to_string()
        .contains("doesn't support ^ (power)"));
    // Cross-family: a non-numeric operand can't combine with a decimal.
    // (An `Int8` mix routes through the FixedInt hook, checked first — see
    // fixed_int_mixing_matrix_errors — so an array exercises the decimal's
    // own operand guard here.)
    assert!(error_of("[1] - Decimal(2.0, 5, 2)")
        .to_string()
        .contains("with a fixed-precision decimal"));
}
