//! Per-cell presentation: text style + number format. Display-only — the
//! underlying value stays exact; formulas, references, and TSV copy always
//! see the raw value. Stored sparsely on `Sheet.formats` (a default format
//! is pruned, never stored) and persisted per sheet in workbooks.

use anzan::BigDecimal;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Semantic palette colors. Stored by NAME so the app can map them to system
/// colors that adapt to light/dark — per-cell absolute RGB would fight the
/// switchable themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// Sign + grouped integer part + fraction padded/rounded to exactly
    /// `decimals` places (banker's). Lives on `BigDecimal` in `anzan` because
    /// grouped literals echo the same grouping — sharing the one
    /// implementation is what keeps a formatted cell and a grouped result
    /// from ever drifting apart.
    fn fixed(value: &BigDecimal, decimals: i64) -> String {
        value.grouped_text(decimals)
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

// Compact codec — only non-default fields are written, and `NumberFormat` is
// flattened into `style`/`decimals`/`symbol`. Byte-for-byte the shape of Swift's
// `CellFormat: Codable` (Sheet/CellFormat.swift), so a `formats` object round-
// trips between the two apps. (Workbook JSON is written with sorted keys, so the
// entry order here doesn't affect the on-disk bytes.)
impl Serialize for CellFormat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if self.bold {
            map.serialize_entry("bold", &true)?;
        }
        if self.italic {
            map.serialize_entry("italic", &true)?;
        }
        if self.underline {
            map.serialize_entry("underline", &true)?;
        }
        if self.strikethrough {
            map.serialize_entry("strikethrough", &true)?;
        }
        if self.alignment != CellAlignment::Auto {
            map.serialize_entry("alignment", &self.alignment)?;
        }
        if let Some(color) = self.text_color {
            map.serialize_entry("textColor", &color)?;
        }
        if let Some(color) = self.fill_color {
            map.serialize_entry("fillColor", &color)?;
        }
        match &self.number_format {
            NumberFormat::General => {}
            NumberFormat::Number { decimals } => {
                map.serialize_entry("style", "number")?;
                map.serialize_entry("decimals", decimals)?;
            }
            NumberFormat::Currency { symbol, decimals } => {
                map.serialize_entry("style", "currency")?;
                map.serialize_entry("symbol", symbol)?;
                map.serialize_entry("decimals", decimals)?;
            }
            NumberFormat::Percent { decimals } => {
                map.serialize_entry("style", "percent")?;
                map.serialize_entry("decimals", decimals)?;
            }
            NumberFormat::Date => map.serialize_entry("style", "date")?,
            NumberFormat::Hex => map.serialize_entry("style", "hex")?,
            NumberFormat::Binary => map.serialize_entry("style", "binary")?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for CellFormat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Every field optional with a default — a `{}` object decodes to the
        // default format, and unknown `style` strings from a newer version
        // degrade to `General` (as Swift does).
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            bold: bool,
            #[serde(default)]
            italic: bool,
            #[serde(default)]
            underline: bool,
            #[serde(default)]
            strikethrough: bool,
            #[serde(default)]
            alignment: CellAlignment,
            #[serde(default, rename = "textColor")]
            text_color: Option<PaletteColor>,
            #[serde(default, rename = "fillColor")]
            fill_color: Option<PaletteColor>,
            #[serde(default)]
            style: Option<String>,
            #[serde(default)]
            decimals: Option<i64>,
            #[serde(default)]
            symbol: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let decimals = raw.decimals.unwrap_or(2);
        let number_format = match raw.style.as_deref() {
            Some("number") => NumberFormat::Number { decimals },
            Some("currency") => NumberFormat::Currency {
                symbol: raw.symbol.unwrap_or_else(|| "$".into()),
                decimals,
            },
            Some("percent") => NumberFormat::Percent { decimals },
            Some("date") => NumberFormat::Date,
            Some("hex") => NumberFormat::Hex,
            Some("binary") => NumberFormat::Binary,
            _ => NumberFormat::General, // unknown/absent styles degrade safely
        };
        Ok(CellFormat {
            bold: raw.bold,
            italic: raw.italic,
            underline: raw.underline,
            strikethrough: raw.strikethrough,
            alignment: raw.alignment,
            text_color: raw.text_color,
            fill_color: raw.fill_color,
            number_format,
        })
    }
}

#[cfg(test)]
mod tests;
