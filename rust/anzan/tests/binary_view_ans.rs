//! Port of the Swift `BinaryViewTests.swift` "ans-prefix continuation" suite —
//! SpeedCrunch-style leading-operator continuation, mode-aware.

use anzan::{Calculator, LanguageMode};

#[test]
fn leading_binary_operator_prefixes_ans() {
    let normal = LanguageMode::Normal;
    assert_eq!(
        Calculator::ans_prefixed("+5", normal).as_deref(),
        Some("ans+5")
    );
    assert_eq!(
        Calculator::ans_prefixed("*2", normal).as_deref(),
        Some("ans*2")
    );
    assert_eq!(
        Calculator::ans_prefixed("/4", normal).as_deref(),
        Some("ans/4")
    );
    assert_eq!(
        Calculator::ans_prefixed("^2", normal).as_deref(),
        Some("ans^2")
    );
    assert_eq!(
        Calculator::ans_prefixed("× 3", normal).as_deref(),
        Some("ans× 3")
    );
}

#[test]
fn minus_is_included_speedcrunch_style() {
    assert_eq!(
        Calculator::ans_prefixed("-5", LanguageMode::Normal).as_deref(),
        Some("ans-5")
    );
}

#[test]
fn leading_spaces_are_trimmed_before_prefixing() {
    assert_eq!(
        Calculator::ans_prefixed("  + 5", LanguageMode::Normal).as_deref(),
        Some("ans+ 5")
    );
}

#[test]
fn non_operator_leads_do_not_prefix() {
    assert_eq!(
        Calculator::ans_prefixed("5 + 3", LanguageMode::Normal),
        None
    );
    assert_eq!(
        Calculator::ans_prefixed("sqrt(2)", LanguageMode::Normal),
        None
    );
    assert_eq!(
        Calculator::ans_prefixed("(1+2)", LanguageMode::Normal),
        None
    );
    assert_eq!(Calculator::ans_prefixed("", LanguageMode::Normal), None);
    // ~ is unary prefix.
    assert_eq!(
        Calculator::ans_prefixed("~5", LanguageMode::Programmer),
        None
    );
}

#[test]
fn percent_and_bit_glyphs_are_operators_only_in_programmer() {
    // Normal: % is postfix percent, bit glyphs aren't operators → no prefix.
    assert_eq!(Calculator::ans_prefixed("%5", LanguageMode::Normal), None);
    assert_eq!(Calculator::ans_prefixed("<<2", LanguageMode::Normal), None);
    assert_eq!(Calculator::ans_prefixed("&3", LanguageMode::Normal), None);
    // Programmer: they lead a continuation.
    assert_eq!(
        Calculator::ans_prefixed("%5", LanguageMode::Programmer).as_deref(),
        Some("ans%5")
    );
    assert_eq!(
        Calculator::ans_prefixed("<<2", LanguageMode::Programmer).as_deref(),
        Some("ans<<2")
    );
    assert_eq!(
        Calculator::ans_prefixed("&3", LanguageMode::Programmer).as_deref(),
        Some("ans&3")
    );
}
