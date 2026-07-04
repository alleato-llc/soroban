//! Engine-API contracts the Gherkin features can't express — ports of
//! Swift's CalculatorTests (facade, change tracking) and CompletionTests
//! (autocomplete candidates, point-mode heuristic, word extraction,
//! programmer-notation detection). These are host seams: the app and CLI
//! call them directly, so their exact shapes are pinned here.

use anzan::{BigDecimal, Calculator, Completion, CompletionKind, EvalOutcome, Value};

#[test]
fn success_updates_ans_but_definitions_do_not() {
    let mut calculator = Calculator::new();
    assert_eq!(
        calculator.evaluate("6 * 7").expect("evaluates"),
        EvalOutcome::Value(Value::Number(BigDecimal::from_int(42)))
    );
    assert_eq!(
        calculator.environment().ans(),
        Value::Number(BigDecimal::from_int(42))
    );

    calculator.evaluate("100").expect("evaluates");
    calculator.evaluate("g(x) = x + 1").expect("defines");
    assert_eq!(
        calculator.environment().ans(),
        Value::Number(BigDecimal::from_int(100)),
        "definitions must not touch ans"
    );
}

#[test]
fn set_and_remove_user_variable_off_log() {
    // The inspector's rename/delete path — no history line involved.
    let mut calculator = Calculator::new();
    calculator.evaluate("perm = 42").expect("assigns");
    assert_eq!(
        calculator.environment().user_variables().get("perm"),
        Some(&Value::Number(BigDecimal::from_int(42)))
    );
    calculator.set_user_variable("acl", Value::Number(BigDecimal::from_int(42)));
    calculator.remove_user_variable("perm");
    assert!(calculator
        .environment()
        .user_variables()
        .get("perm")
        .is_none());
    assert_eq!(
        calculator.environment().user_variables().get("acl"),
        Some(&Value::Number(BigDecimal::from_int(42)))
    );
    calculator.remove_user_variable("acl");
    assert!(calculator
        .environment()
        .user_variables()
        .get("acl")
        .is_none());
}

#[test]
fn errors_come_back_as_failures() {
    let mut calculator = Calculator::new();
    assert!(calculator.evaluate("1 +").is_err());
    assert!(calculator.evaluate("").is_err());
}

#[test]
fn mutations_bump_change_count_plain_evaluations_dont() {
    // WorkbookManager's dirty detection rides this counter — don't go back
    // to snapshotting dictionaries.
    let mut calculator = Calculator::new();
    let start = calculator.environment().change_count();

    calculator.evaluate("1 + 1").expect("evaluates");
    assert_eq!(
        calculator.environment().change_count(),
        start,
        "ans doesn't count"
    );

    calculator.evaluate("x = 5").expect("assigns");
    assert_eq!(calculator.environment().change_count(), start + 1);

    calculator.evaluate("x = 5").expect("assigns");
    assert_eq!(
        calculator.environment().change_count(),
        start + 1,
        "same value — no change"
    );

    calculator.evaluate("x = 6").expect("assigns");
    assert_eq!(calculator.environment().change_count(), start + 2);

    calculator.evaluate("f(a) = a * 2").expect("defines");
    assert_eq!(
        calculator.environment().change_count(),
        start + 3,
        "definitions count"
    );

    let before_replace = calculator.environment().change_count();
    calculator
        .environment_mut()
        .replace_user_variables(Default::default());
    calculator
        .environment_mut()
        .replace_user_functions(Default::default());
    assert_eq!(calculator.environment().change_count(), before_replace + 2);
}

// MARK: Autocomplete

#[test]
fn completes_functions_case_insensitively() {
    let names: Vec<String> = Calculator::new()
        .completions("PER")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(
        names,
        [
            "percent",
            "percentChange",
            "percentile",
            "percentOf",
            "perm"
        ]
    );
}

#[test]
fn includes_constants_and_variables() {
    let mut calculator = Calculator::new();
    calculator.evaluate("price = 100").expect("assigns");
    let completions = calculator.completions("p");
    assert!(completions.contains(&Completion {
        name: "pi".to_string(),
        kind: CompletionKind::Constant,
    }));
    assert!(completions.contains(&Completion {
        name: "price".to_string(),
        kind: CompletionKind::Variable,
    }));
    assert!(completions.contains(&Completion {
        name: "pmt".to_string(),
        kind: CompletionKind::Function,
    }));
    // Sorted case-insensitively by name.
    let names: Vec<String> = completions.iter().map(|c| c.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn kind_badges() {
    assert_eq!(CompletionKind::Function.badge(), "ƒ");
    assert_eq!(CompletionKind::Variable.badge(), "var");
    assert_eq!(CompletionKind::Constant.badge(), "const");
}

#[test]
fn empty_prefix_and_exact_matches_yield_nothing() {
    let calculator = Calculator::new();
    assert!(calculator.completions("").is_empty());
    // "abs" is the only match for itself — nothing left to complete.
    assert!(calculator.completions("abs").is_empty());
    assert!(!calculator.completions("m").is_empty());
    assert!(calculator.completions("zzz").is_empty());
}

#[test]
fn exact_prefix_with_longer_siblings_still_completes() {
    // "percent" matches percent, percentChange, percentile, percentOf —
    // the exact match must not collapse the list.
    assert_eq!(Calculator::new().completions("percent").len(), 4);
}

#[test]
fn user_functions_complete() {
    let mut calculator = Calculator::new();
    calculator.evaluate("payback(x) = x / 12").expect("defines");
    let names: Vec<String> = calculator
        .completions("pay")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&"payback".to_string()));
}

// MARK: Point mode & word extraction

#[test]
fn expects_operand_point_mode_heuristic() {
    // Drafts that should accept a clicked cell reference…
    for draft in [
        "=", "B:1 +", "sum(", "sum(B:1,", "if(B:1 >", "2 *", "B:1..", "= B:1 ×", "1 ≤", "√",
    ] {
        assert!(Calculator::expects_operand(draft), "'{draft}'");
    }
    // …and drafts that should commit instead.
    for draft in ["", "   ", "Q1 revenue", "B:1 + B:2", "sum(B:1)", "42"] {
        assert!(!Calculator::expects_operand(draft), "'{draft}'");
    }
}

#[test]
fn trailing_identifier_extraction() {
    assert_eq!(Calculator::trailing_identifier("1 + sq"), "sq");
    assert_eq!(Calculator::trailing_identifier("rate_2"), "rate_2");
    assert_eq!(Calculator::trailing_identifier("2p"), "p"); // 2 is a literal
    assert_eq!(Calculator::trailing_identifier("1 + 2"), "");
    assert_eq!(Calculator::trailing_identifier(""), "");
    assert_eq!(Calculator::trailing_identifier("sqrt("), "");
    assert_eq!(Calculator::trailing_identifier("x + _tmp"), "_tmp");
}

#[test]
fn detects_programmer_lines() {
    // The hex-echo trigger: 0x/0b at a token boundary, or the base/bit
    // functions — display-only, so a heuristic is acceptable, but it must
    // not fire on ordinary arithmetic.
    for line in [
        "0xFF + 1",
        "bitAnd(a, b)",
        "fromBase(\"ff\", 16)",
        "x + 0b1010",
        "BITOR(1, 2)",
    ] {
        assert!(Calculator::uses_programmer_notation(line), "'{line}'");
    }
    for line in [
        "1 + 2",
        "10x",
        "a0b",
        "orbit(3)",
        "box = 5",
        "pmt(0.05, 12, 1)",
    ] {
        assert!(!Calculator::uses_programmer_notation(line), "'{line}'");
    }
}

#[test]
fn documentation_lists_user_definitions_before_builtins() {
    // The reference-window assembly: live "Your Functions"/"Your Data Types"
    // come first, then the built-in categories.
    let mut calculator = Calculator::new();
    calculator
        .evaluate("double(x) = x * 2")
        .expect("defines a function");
    calculator
        .evaluate("data Point { x: Number, y: Number }")
        .expect("defines a data type");

    let categories = calculator.documentation();
    let titles: Vec<&str> = categories.iter().map(|c| c.title.as_str()).collect();

    assert_eq!(titles.first(), Some(&"Your Functions"));
    assert!(titles.contains(&"Your Data Types"));
    assert!(
        titles.iter().position(|t| *t == "Your Functions")
            < titles.iter().position(|t| !t.starts_with("Your")),
        "user categories precede the built-ins: {titles:?}"
    );

    let functions = &categories[0].entries;
    assert!(functions.iter().any(|d| d.name == "double"));

    // A fresh calculator shows only the built-in categories.
    let bare = Calculator::new().documentation();
    assert!(bare.iter().all(|c| !c.title.starts_with("Your")));
}
