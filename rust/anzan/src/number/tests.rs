//! Port of BigDecimalTests.swift — the same cases, same expected strings.

use super::*;

fn num(s: &str) -> BigDecimal {
    BigDecimal::parse(s).unwrap_or_else(|| panic!("'{s}' should parse"))
}

// MARK: BigDecimal parsing

#[test]
fn parses_literals() {
    for (input, expected) in [
        ("123", "123"),
        ("-1.5", "-1.5"),
        ("1_000", "1000"),
        ("2.5e-3", "0.0025"),
        ("2.5E3", "2500"),
        ("0.000", "0"),
        (".5", "0.5"),
        ("-0.25", "-0.25"),
    ] {
        assert_eq!(num(input).to_string(), expected, "parsing '{input}'");
    }
}

#[test]
fn rejects_malformed() {
    for input in ["", "-", "1.2.3", "1e", "abc", "1e2.5"] {
        assert!(BigDecimal::parse(input).is_none(), "'{input}' should not parse");
    }
}

// MARK: BigDecimal arithmetic

#[test]
fn decimal_addition_is_exact() {
    // The whole reason this type exists.
    assert_eq!(num("0.1") + num("0.2"), num("0.3"));
}

#[test]
fn subtraction() {
    assert_eq!(num("1") - num("0.9"), num("0.1"));
    assert_eq!(num("2.5") - num("2.5"), BigDecimal::zero());
}

#[test]
fn multiplication_is_exact() {
    assert_eq!(num("1.5") * num("2.5"), num("3.75"));
    assert_eq!(num("-0.001") * num("1000"), num("-1"));
}

#[test]
fn exact_division() {
    assert_eq!(num("1").div(&num("4")).unwrap(), num("0.25"));
    assert_eq!(num("-10").div(&num("4")).unwrap(), num("-2.5"));
}

#[test]
fn repeating_division_carries_working_precision() {
    let third = num("1").div(&num("3")).unwrap();
    let text = third.to_string();
    let digits: String = text.chars().skip_while(|&c| c == '0' || c == '.').collect();
    assert_eq!(digits.len(), 50);
    assert!(digits.chars().all(|c| c == '3'), "got {text}");
}

#[test]
fn division_by_zero_throws() {
    assert_eq!(
        num("1").div(&BigDecimal::zero()),
        Err(EngineError::DivisionByZero)
    );
}

#[test]
fn modulo_matches_dividend_sign() {
    assert_eq!(num("7").rem(&num("3")).unwrap(), num("1"));
    assert_eq!(num("-7").rem(&num("3")).unwrap(), num("-1"));
    assert_eq!(num("7.5").rem(&num("2")).unwrap(), num("1.5"));
}

#[test]
fn comparison_across_exponents() {
    assert!(num("0.5") < num("2"));
    assert!(num("-3") < num("0.001"));
    assert_eq!(num("100"), BigDecimal::new(BigInt::from(1), 2));
}

// MARK: BigDecimal rounding

#[test]
fn rounds_to_places() {
    assert_eq!(num("2.345").rounded_to_places(2), num("2.34")); // banker's: half to even
    assert_eq!(num("2.355").rounded_to_places(2), num("2.36"));
    assert_eq!(num("2.3449").rounded_to_places(2), num("2.34"));
    assert_eq!(num("-2.345").rounded_to_places(2), num("-2.34"));
    assert_eq!(num("1234").rounded_to_places(-2), num("1200"));
}

#[test]
fn rounds_to_significant_digits() {
    assert_eq!(num("123456").rounded_to_significant_digits(3), num("123000"));
    assert_eq!(num("0.0012349").rounded_to_significant_digits(3), num("0.00123"));
}

// MARK: BigDecimal powers and roots

#[test]
fn integer_powers_are_exact() {
    assert_eq!(num("2").power(10).unwrap(), num("1024"));
    assert_eq!(num("0.1").power(3).unwrap(), num("0.001"));
    assert_eq!(num("-2").power(3).unwrap(), num("-8"));
    assert_eq!(num("2").power(-2).unwrap(), num("0.25"));
    assert_eq!(num("5").power(0).unwrap(), BigDecimal::one());
}

#[test]
fn zero_to_zero_is_undefined() {
    assert!(BigDecimal::zero().power(0).is_err());
}

#[test]
fn huge_powers_are_rejected() {
    assert!(num("9").power(999_999_999).is_err());
}

#[test]
fn exact_square_roots() {
    assert_eq!(num("9").square_root().unwrap(), num("3"));
    assert_eq!(num("2.25").square_root().unwrap(), num("1.5"));
    assert_eq!(num("0.0001").square_root().unwrap(), num("0.01"));
    assert_eq!(BigDecimal::zero().square_root().unwrap(), BigDecimal::zero());
}

#[test]
fn sqrt_two_to_working_precision() {
    let root = num("2").square_root().unwrap();
    // First 50 digits of sqrt(2).
    assert!(root
        .to_string()
        .starts_with("1.414213562373095048801688724209698078569671875376"));
}

#[test]
fn sqrt_of_negative_throws() {
    assert!(num("-4").square_root().is_err());
}

// MARK: BigDecimal formatting

#[test]
fn plain_formatting() {
    assert_eq!(num("1500").to_string(), "1500");
    assert_eq!(num("0.030").to_string(), "0.03");
    assert_eq!(num("-12.50").to_string(), "-12.5");
}

#[test]
fn scientific_for_extremes() {
    assert_eq!(num("1e40").to_string(), "1e+40");
    assert_eq!(num("-2.5e-40").to_string(), "-2.5e-40");
}

#[test]
fn double_round_trip() {
    let value = BigDecimal::from_f64(0.25).unwrap();
    assert_eq!(value, num("0.25"));
    assert_eq!(num("12.5").to_f64(), 12.5);
    assert!(BigDecimal::from_f64(f64::INFINITY).is_none());
    assert!(BigDecimal::from_f64(f64::NAN).is_none());
}

// MARK: Precision context (the Rust scoping mechanism itself)

#[test]
fn precision_context_scopes_and_restores() {
    assert_eq!(PrecisionContext::current(), 50);
    PrecisionContext::with(10, || {
        assert_eq!(PrecisionContext::current(), 10);
        let third = num("1").div(&num("3")).unwrap();
        assert_eq!(third.to_string(), "0.3333333333");
    });
    assert_eq!(PrecisionContext::current(), 50);
}
