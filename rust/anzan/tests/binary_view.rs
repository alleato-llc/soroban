//! Port of the Swift `BinaryViewTests.swift` "Binary bit-editor view" suite —
//! the register model (width/sign policy, two's-complement, bit flips).

use anzan::{BinaryView, BinaryViewUnavailable, Calculator, EvalOutcome, Value};

fn value(s: &str) -> Value {
    let mut calc = Calculator::new();
    match calc.evaluate(s).expect("evaluates") {
        EvalOutcome::Value(v) => v,
        other => panic!("not a value: {other:?}"),
    }
}

fn view_width(s: &str, preferred_width: u32) -> BinaryView {
    match BinaryView::make(&value(s), preferred_width) {
        Ok(view) => view,
        Err(reason) => panic!("unexpectedly unavailable: {reason:?}"),
    }
}

fn view(s: &str) -> BinaryView {
    view_width(s, 32)
}

fn unavailable(s: &str) -> BinaryViewUnavailable {
    BinaryView::make(&value(s), 32).expect_err("unexpectedly available")
}

#[test]
fn fixed_int_uses_its_own_width_and_sign() {
    let v = view("Int8(5)");
    assert_eq!(v.width, 8);
    assert!(v.signed());
    // 0000_0101
    assert_eq!(
        v.bits(),
        [false, false, false, false, false, true, false, true]
    );
}

#[test]
fn unsigned_fixed_int_full_range() {
    let v = view("UInt8(255)");
    assert_eq!(v.width, 8);
    assert!(!v.signed());
    assert!(v.bits().iter().all(|&b| b)); // 1111_1111
}

#[test]
fn negative_fixed_int_is_twos_complement() {
    let v = view("Int8(-1)");
    assert!(v.bits().iter().all(|&b| b)); // 1111_1111
    assert_eq!(v.value().to_string(), "Int8(-1)"); // round-trips to its type
}

#[test]
fn plain_integer_is_unsigned_at_preferred_width_bumped_to_fit() {
    assert_eq!(view_width("255", 8).width, 8);
    assert_eq!(view_width("255", 32).width, 32); // preferred floor honored
    assert_eq!(view_width("256", 8).width, 16); // auto-bumped past 8
    assert_eq!(view("5").width, 32); // default preferred
}

#[test]
fn flipping_a_bit_changes_the_value_preserving_kind() {
    // Int8(5) flip bit 1 (0000_0101 → 0000_0111) = 7, still Int8.
    assert_eq!(
        view("Int8(5)").flipping_bit(1).value().to_string(),
        "Int8(7)"
    );
    // Plain 0 at 8-bit, set the high bit → 128 (unsigned), stays a number.
    assert_eq!(
        view_width("0", 8).flipping_bit(7).value().to_string(),
        "128"
    );
    // UInt8 high bit toggles within the width, never overflows.
    assert_eq!(
        view("UInt8(1)").flipping_bit(7).value().to_string(),
        "UInt8(129)"
    );
}

#[test]
fn flip_is_its_own_inverse() {
    let v = view("UInt8(0b1010)");
    assert_eq!(v.flipping_bit(3).flipping_bit(3).value(), v.value());
}

#[test]
fn non_integers_and_strings_are_unavailable() {
    assert_eq!(unavailable("10.5"), BinaryViewUnavailable::NotAnInteger);
    assert_eq!(unavailable("\"hi\""), BinaryViewUnavailable::NotAnInteger);
    // A decimal type, not bits.
    assert_eq!(
        unavailable("Decimal(10.5, 5, 2)"),
        BinaryViewUnavailable::NotAnInteger
    );
}

#[test]
fn negative_plain_number_suggests_a_typed_int() {
    assert_eq!(unavailable("0 - 5"), BinaryViewUnavailable::Negative);
}

#[test]
fn over_256_bits_is_too_wide() {
    assert_eq!(unavailable("2 ^ 300"), BinaryViewUnavailable::TooWide); // a 301-bit plain integer
}

#[test]
fn minimum_width_tracks_the_value() {
    // The narrowest editable width that holds the value — the UI grays out
    // smaller picker options below this.
    assert_eq!(view("5").minimum_width(), 8); // 3 bits → 8
    assert_eq!(view("255").minimum_width(), 8); // 8 bits → 8
    assert_eq!(view("256").minimum_width(), 16); // 9 bits → 16
    assert_eq!(view("2 ^ 100").minimum_width(), 128); // 101 bits → 128
}

#[test]
fn up_to_256_bits_edits() {
    assert_eq!(view("UInt256(1)").width, 256); // the widest fixed type
    assert_eq!(view("Int256(-1)").width, 256);
    assert_eq!(view("2 ^ 100").width, 128); // 101 bits → 128
    assert_eq!(view("2 ^ 200").width, 256); // 201 bits → 256
}
