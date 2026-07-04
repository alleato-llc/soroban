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
