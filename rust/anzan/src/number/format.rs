//! Thousands-grouped rendering — the port of `BigDecimal+Format.swift`. All of
//! it is pure string/BigInt math (no f64, no locale formatter), so a 40-digit
//! value groups exactly.
//!
//! This lives in `anzan` (not the hosting layer) because finance-mode literals
//! echo their own grouping: `138,561 * 9%` answers `12,470.49`. The sheet's
//! `NumberFormat` renders through the same helpers, so a formatted cell and a
//! finance-mode result can never drift apart.

use super::BigDecimal;

impl BigDecimal {
    /// "1234567" → "1,234,567". Takes the bare digits of an integer part.
    pub fn grouping(integer: &str) -> String {
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

    /// Sign + grouped integer part + fraction padded/rounded to exactly
    /// `decimals` places (banker's, via `rounded_to_places`).
    pub fn grouped_text(&self, decimals: i64) -> String {
        let rounded = self.rounded_to_places(decimals);
        let (sign, integer, mut fraction) = rounded.parts();
        if (fraction.len() as i64) < decimals {
            fraction.push_str(&"0".repeat((decimals - fraction.len() as i64) as usize));
        }
        let grouped = Self::grouping(&integer);
        if decimals > 0 {
            format!("{sign}{grouped}.{fraction}")
        } else {
            format!("{sign}{grouped}")
        }
    }

    /// Grouped at the value's OWN number of decimals — no padding, no
    /// rounding. `138561` → "138,561"; `12470.49` → "12,470.49".
    /// Scientific-notation values (past `Display`'s threshold) pass through
    /// ungrouped, since there is no integer run to group.
    pub fn grouped_text_natural(&self) -> String {
        let plain = self.to_string();
        if plain.contains('e') || plain.contains('E') {
            return plain;
        }
        let (sign, integer, fraction) = self.parts();
        let grouped = Self::grouping(&integer);
        if fraction.is_empty() {
            format!("{sign}{grouped}")
        } else {
            format!("{sign}{grouped}.{fraction}")
        }
    }

    /// Splits into sign, bare integer digits, and bare fraction digits.
    fn parts(&self) -> (&'static str, String, String) {
        let digits = self.significand().magnitude().to_string();
        let sign = if self.is_negative() { "-" } else { "" };
        let exponent = self.exponent();
        if exponent >= 0 {
            return (
                sign,
                format!("{digits}{}", "0".repeat(exponent as usize)),
                String::new(),
            );
        }
        let point_position = digits.len() as i64 + exponent;
        if point_position <= 0 {
            return (
                sign,
                "0".to_string(),
                format!("{}{digits}", "0".repeat((-point_position) as usize)),
            );
        }
        let index = point_position as usize;
        (
            sign,
            digits[..index].to_string(),
            digits[index..].to_string(),
        )
    }
}
