//! Tests for cell formatting and number rendering.

use super::*;

fn num(text: &str) -> BigDecimal {
    BigDecimal::parse(text).expect("test literal parses")
}

#[test]
fn number_groups_and_pads() {
    let fmt = NumberFormat::Number { decimals: 2 };
    assert_eq!(fmt.rendered(&num("1234567.5")), "1,234,567.50");
    // 12-digit integer part; the trailing .345 rounds banker's to .34.
    assert_eq!(fmt.rendered(&num("123456789012.345")), "123,456,789,012.34");
    // Banker's half-to-even; the rounded zero is unsigned, as in Swift.
    assert_eq!(fmt.rendered(&num("-0.005")), "0.00");
    assert_eq!(fmt.rendered(&num("-1234.005")), "-1,234.00");
    let whole = NumberFormat::Number { decimals: 0 };
    assert_eq!(whole.rendered(&num("1234.6")), "1,235");
}

#[test]
fn forty_digit_value_groups_exactly() {
    let fmt = NumberFormat::Number { decimals: 0 };
    let forty = "1234567890123456789012345678901234567890";
    assert_eq!(
        fmt.rendered(&num(forty)),
        "1,234,567,890,123,456,789,012,345,678,901,234,567,890"
    );
}

#[test]
fn currency_places_sign_before_symbol() {
    let dollars = NumberFormat::Currency {
        symbol: "$".into(),
        decimals: 2,
    };
    assert_eq!(dollars.rendered(&num("1234.5")), "$1,234.50");
    let euros = NumberFormat::Currency {
        symbol: "€".into(),
        decimals: 2,
    };
    assert_eq!(euros.rendered(&num("-2")), "-€2.00");
}

#[test]
fn percent_is_an_exponent_shift() {
    let fmt = NumberFormat::Percent { decimals: 2 };
    assert_eq!(fmt.rendered(&num("0.0825")), "8.25%");
    assert_eq!(fmt.rendered(&num("1")), "100.00%");
}

#[test]
fn date_renders_iso_from_serial() {
    // 20610 days after 1970-01-01 = 2026-06-06 (cross-checked against
    // the Swift CivilDate implementation).
    assert_eq!(NumberFormat::Date.rendered(&num("20610")), "2026-06-06");
    assert_eq!(NumberFormat::Date.rendered(&num("0")), "1970-01-01");
    assert_eq!(NumberFormat::Date.rendered(&num("-1")), "1969-12-31");
}

#[test]
fn hex_and_binary_fall_back_for_non_integers() {
    assert_eq!(NumberFormat::Hex.rendered(&num("195")), "0xC3");
    assert_eq!(NumberFormat::Binary.rendered(&num("195")), "0b1100_0011");
    assert_eq!(NumberFormat::Hex.rendered(&num("1.5")), "1.5");
    assert_eq!(NumberFormat::Binary.rendered(&num("1.5")), "1.5");
}

#[test]
fn adjusting_decimals_clamps_and_general_steps_into_number() {
    assert_eq!(
        NumberFormat::General.adjusting_decimals(1),
        NumberFormat::Number { decimals: 3 }
    );
    assert_eq!(
        NumberFormat::Number { decimals: 12 }.adjusting_decimals(5),
        NumberFormat::Number { decimals: 12 }
    );
    assert_eq!(
        NumberFormat::Percent { decimals: 0 }.adjusting_decimals(-1),
        NumberFormat::Percent { decimals: 0 }
    );
    assert_eq!(NumberFormat::Date.adjusting_decimals(1), NumberFormat::Date);
}

#[test]
fn default_format_is_default() {
    assert!(CellFormat::new().is_default());
    let mut bold = CellFormat::new();
    bold.bold = true;
    assert!(!bold.is_default());
}

#[test]
fn general_renders_the_canonical_value() {
    // General is the identity: exactly what the engine's own rendering shows.
    assert_eq!(NumberFormat::General.rendered(&num("1234.5")), "1234.5");
    assert_eq!(NumberFormat::General.rendered(&num("-0")), "0");
}

#[test]
fn fixed_handles_values_smaller_than_the_first_decimal() {
    // point_position <= 0: the integer part is a bare "0" and the fraction is
    // zero-padded up to the significant digits.
    let four = NumberFormat::Number { decimals: 4 };
    assert_eq!(four.rendered(&num("0.0025")), "0.0025");
    // Positive exponent path: 100000 normalizes to 1e5, rebuilt as digits.
    let whole = NumberFormat::Number { decimals: 0 };
    assert_eq!(whole.rendered(&num("100000")), "100,000");
    // A value that needs no grouping (≤ 3 integer digits).
    assert_eq!(whole.rendered(&num("12")), "12");
}

#[test]
fn currency_and_percent_edges() {
    let money = NumberFormat::Currency {
        symbol: "$".into(),
        decimals: 2,
    };
    // Zero renders unsigned, with the symbol.
    assert_eq!(money.rendered(&num("0")), "$0.00");
    let percent = NumberFormat::Percent { decimals: 0 };
    assert_eq!(percent.rendered(&num("-0.5")), "-50%");
}

#[test]
fn date_falls_back_when_beyond_any_calendar() {
    // A serial too large for i64 can't be a civil date — show the number.
    let huge = "12345678901234567890";
    assert_eq!(NumberFormat::Date.rendered(&num(huge)), huge);
    // A negative-year (BCE) serial exercises the signed padding.
    assert_eq!(NumberFormat::Date.rendered(&num("-800000")), "-0221-09-04");
}

#[test]
fn adjusting_decimals_covers_currency_and_radix_formats() {
    assert_eq!(
        NumberFormat::Currency {
            symbol: "€".into(),
            decimals: 2
        }
        .adjusting_decimals(1),
        NumberFormat::Currency {
            symbol: "€".into(),
            decimals: 3
        }
    );
    // Radix formats carry no decimals — the stepper leaves them unchanged.
    assert_eq!(NumberFormat::Hex.adjusting_decimals(1), NumberFormat::Hex);
    assert_eq!(
        NumberFormat::Binary.adjusting_decimals(-3),
        NumberFormat::Binary
    );
}

// MARK: Codec — the compact, Swift-compatible JSON shape

fn round_trip(format: &CellFormat) -> CellFormat {
    let json = serde_json::to_string(format).expect("serializes");
    serde_json::from_str(&json).expect("deserializes")
}

#[test]
fn every_style_flag_and_format_round_trips() {
    // Every non-default flag, color, alignment, and NumberFormat variant
    // survives the compact codec unchanged.
    for number_format in [
        NumberFormat::General,
        NumberFormat::Number { decimals: 3 },
        NumberFormat::Currency {
            symbol: "£".into(),
            decimals: 2,
        },
        NumberFormat::Percent { decimals: 1 },
        NumberFormat::Date,
        NumberFormat::Hex,
        NumberFormat::Binary,
    ] {
        let format = CellFormat {
            bold: true,
            italic: true,
            underline: true,
            strikethrough: true,
            alignment: CellAlignment::Center,
            text_color: Some(PaletteColor::Red),
            fill_color: Some(PaletteColor::Blue),
            number_format: number_format.clone(),
        };
        assert_eq!(
            round_trip(&format),
            format,
            "{number_format:?} lost fidelity"
        );
    }
}

#[test]
fn serialize_writes_only_non_default_fields() {
    // A default format is the empty object; nothing is written for it (that's
    // why `isDefault` entries are pruned from the sparse map).
    assert_eq!(serde_json::to_string(&CellFormat::default()).unwrap(), "{}");
    // A single flag writes a single entry.
    let italic = CellFormat {
        italic: true,
        ..CellFormat::default()
    };
    assert_eq!(
        serde_json::to_string(&italic).unwrap(),
        r#"{"italic":true}"#
    );
}

#[test]
fn decode_supplies_defaults_and_degrades_unknown_styles() {
    // An empty object decodes to the default format.
    assert_eq!(
        serde_json::from_str::<CellFormat>("{}").unwrap(),
        CellFormat::default()
    );
    // Currency without an explicit symbol/decimals falls back to $ and 2.
    assert_eq!(
        serde_json::from_str::<CellFormat>(r#"{"style":"currency"}"#)
            .unwrap()
            .number_format,
        NumberFormat::Currency {
            symbol: "$".into(),
            decimals: 2
        }
    );
    // A style string from a newer version degrades to General (never fails).
    assert_eq!(
        serde_json::from_str::<CellFormat>(r#"{"style":"quantum"}"#)
            .unwrap()
            .number_format,
        NumberFormat::General
    );
}
