//! Per-cell presentation: text style + number format. Display-only — the
//! underlying value stays exact; formulas, references, and TSV copy always
//! see the raw value. Stored sparsely on `Sheet.formats` (a default format
//! is pruned, never stored) and persisted per sheet in workbooks.

use anzan::BigDecimal;

/// Semantic palette colors. Stored by NAME so the app can map them to system
/// colors that adapt to light/dark — per-cell absolute RGB would fight the
/// switchable themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

impl PaletteColor {
    /// The Swift side's `CaseIterable`, in declaration order.
    pub const ALL: [PaletteColor; 7] = [
        PaletteColor::Red,
        PaletteColor::Orange,
        PaletteColor::Yellow,
        PaletteColor::Green,
        PaletteColor::Blue,
        PaletteColor::Purple,
        PaletteColor::Gray,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CellAlignment {
    /// The grid's automatic rule: text left, numbers right, errors centered.
    #[default]
    Auto,
    Left,
    Center,
    Right,
}

impl CellAlignment {
    /// The Swift side's `CaseIterable`, in declaration order.
    pub const ALL: [CellAlignment; 4] = [
        CellAlignment::Auto,
        CellAlignment::Left,
        CellAlignment::Center,
        CellAlignment::Right,
    ];
}

/// How a numeric cell value renders. All rendering is pure string/BigInt
/// math — no floating point, no locale formatter — so formatted display
/// stays as exact as the engine (a 40-digit value groups correctly).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum NumberFormat {
    #[default]
    General,
    /// Fixed decimals with thousands grouping: 1234567.5 → "1,234,567.50".
    Number {
        decimals: i64,
    },
    /// "$1,234.50" / "-€2.00" — the symbol is stored, so a workbook renders
    /// identically on machines with different locales.
    Currency {
        symbol: String,
        decimals: i64,
    },
    /// ×100 with a % sign — an exact exponent shift: 0.0825 → "8.25%".
    Percent {
        decimals: i64,
    },
    /// Day serials (the engine's date representation) as "2026-06-06".
    Date,
    /// Programmer display: integers as "0xC3" / "0b1100_0011". Display-only
    /// like everything here — the value stays an exact decimal, references
    /// see the number. Non-integers fall back to plain rendering.
    Hex,
    Binary,
}

impl NumberFormat {
    pub const DECIMALS_RANGE: std::ops::RangeInclusive<i64> = 0..=12;

    pub fn rendered(&self, value: &BigDecimal) -> String {
        match self {
            NumberFormat::General => value.to_string(),
            NumberFormat::Number { decimals } => Self::fixed(value, *decimals),
            NumberFormat::Currency { symbol, decimals } => {
                let magnitude = if value.is_negative() {
                    -value
                } else {
                    value.clone()
                };
                format!(
                    "{}{symbol}{}",
                    if value.is_negative() { "-" } else { "" },
                    Self::fixed(&magnitude, *decimals)
                )
            }
            NumberFormat::Percent { decimals } => {
                let scaled = BigDecimal::new(value.significand().clone(), value.exponent() + 2);
                format!("{}%", Self::fixed(&scaled, *decimals))
            }
            NumberFormat::Date => {
                let Some(serial) = value.rounded_to_places(0).int_value() else {
                    return value.to_string(); // beyond any calendar — show the number
                };
                let (year, month, day) = Self::civil_from_serial(serial);
                format!(
                    "{}-{}-{}",
                    Self::padded(year, 4),
                    Self::padded(month, 2),
                    Self::padded(day, 2)
                )
            }
            NumberFormat::Hex => value.hex_text().unwrap_or_else(|| value.to_string()),
            NumberFormat::Binary => value.binary_text().unwrap_or_else(|| value.to_string()),
        }
    }

    /// The format with `delta` more (or fewer) decimals — the menu's
    /// Increase/Decrease Decimals stepper. General steps into Number.
    pub fn adjusting_decimals(&self, delta: i64) -> NumberFormat {
        fn clamped(d: i64) -> i64 {
            d.clamp(
                *NumberFormat::DECIMALS_RANGE.start(),
                *NumberFormat::DECIMALS_RANGE.end(),
            )
        }
        match self {
            NumberFormat::General => NumberFormat::Number {
                decimals: clamped(2 + delta),
            },
            NumberFormat::Number { decimals } => NumberFormat::Number {
                decimals: clamped(decimals + delta),
            },
            NumberFormat::Currency { symbol, decimals } => NumberFormat::Currency {
                symbol: symbol.clone(),
                decimals: clamped(decimals + delta),
            },
            NumberFormat::Percent { decimals } => NumberFormat::Percent {
                decimals: clamped(decimals + delta),
            },
            NumberFormat::Date => NumberFormat::Date,
            NumberFormat::Hex => NumberFormat::Hex,
            NumberFormat::Binary => NumberFormat::Binary,
        }
    }

    /// Sign + grouped integer part + fraction padded/rounded to exactly
    /// `decimals` places (banker's, via `rounded_to_places`).
    fn fixed(value: &BigDecimal, decimals: i64) -> String {
        let rounded = value.rounded_to_places(decimals);
        let digits = rounded.significand().magnitude().to_string();
        let sign = if rounded.is_negative() { "-" } else { "" };
        let exponent = rounded.exponent();

        let (integer, mut fraction) = if exponent >= 0 {
            (
                format!("{digits}{}", "0".repeat(exponent as usize)),
                String::new(),
            )
        } else {
            let point_position = digits.len() as i64 + exponent;
            if point_position <= 0 {
                (
                    "0".to_string(),
                    format!("{}{digits}", "0".repeat((-point_position) as usize)),
                )
            } else {
                let index = point_position as usize;
                (digits[..index].to_string(), digits[index..].to_string())
            }
        };
        if (fraction.len() as i64) < decimals {
            fraction.push_str(&"0".repeat((decimals - fraction.len() as i64) as usize));
        }
        let grouped = Self::grouped(&integer);
        if decimals > 0 {
            format!("{sign}{grouped}.{fraction}")
        } else {
            format!("{sign}{grouped}")
        }
    }

    /// "1234567" → "1,234,567".
    fn grouped(integer: &str) -> String {
        if integer.len() <= 3 {
            return integer.to_string();
        }
        let mut out: Vec<char> = Vec::with_capacity(integer.len() + integer.len() / 3);
        for (offset, ch) in integer.chars().rev().enumerate() {
            if offset > 0 && offset % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        out.iter().rev().collect()
    }

    fn padded(n: i64, width: usize) -> String {
        let text = n.abs().to_string();
        let padded = if text.len() < width {
            format!("{}{text}", "0".repeat(width - text.len()))
        } else {
            text
        };
        if n < 0 {
            format!("-{padded}")
        } else {
            padded
        }
    }

    /// days since 1970-01-01 → (y, m, d). Howard Hinnant's civil-from-days,
    /// matching `CivilDate.civil(fromSerial:)` on the Swift side (re-derived
    /// locally — the anzan crate's copy is private to its dates module).
    fn civil_from_serial(serial: i64) -> (i64, i64, i64) {
        let z = serial + 719_468;
        let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
        let doe = z - era * 146_097; // [0, 146096]
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
        let mp = (5 * doy + 2) / 153; // [0, 11]
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        (y + if month <= 2 { 1 } else { 0 }, month, day)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub alignment: CellAlignment,
    pub text_color: Option<PaletteColor>,
    pub fill_color: Option<PaletteColor>,
    pub number_format: NumberFormat,
}

impl CellFormat {
    pub fn new() -> Self {
        Self::default()
    }

    /// Default formats are pruned from the sparse per-sheet map.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
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
}
