//! A read/edit view of an integer `Value`'s bits — the model behind the
//! app's binary bit-editor overlay (macOS-Calculator-style). Pure and
//! host-free: the width policy and two's-complement encoding live here so
//! the UI stays thin and the logic is tested.
//!
//! A `FixedInt` edits at its own declared width and signedness (full
//! two's-complement). A plain non-negative integer edits as an UNSIGNED
//! register at a chosen width (signed bit-editing is the job of the typed
//! `Int…` values). Widths are capped at 256 bits; wider values, negatives,
//! and non-integers are not editable and carry a reason the host can
//! explain.

use super::fixed_int::FixedInt;
use super::value::Value;
use crate::BigDecimal;
use num_bigint::BigInt;
use num_traits::Zero;

mod fields;
mod layout;

pub use fields::{Field, FieldSpec};

/// Why a value can't be bit-edited (the host shows the matching hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// A decimal/string/array/… — no bits to edit.
    NotAnInteger,
    /// A plain negative number — wrap it in a signed Int type.
    Negative,
    /// Needs more than 256 bits (a huge integer).
    TooWide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A bare Number, edited as an unsigned register.
    Plain,
    /// An Int…/UInt… value, edited in two's-complement.
    Fixed { signed: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryView {
    pub kind: Kind,
    pub width: u32,
    /// The unsigned bit pattern, always in `[0, 2^width)`.
    pub pattern: BigInt,
}

/// Display widths a plain integer may use (the bit grid). Capped at
/// `MAX_WIDTH`. Includes 48 (MAC addresses) beyond `FixedInt`'s type widths.
pub const EDITABLE_WIDTHS: [u32; 7] = [8, 16, 32, 48, 64, 128, 256];
pub const MAX_WIDTH: u32 = 256;

impl BinaryView {
    /// True for a signed fixed-width value (the high bit is the sign).
    pub fn signed(&self) -> bool {
        matches!(self.kind, Kind::Fixed { signed: true })
    }

    /// True when the value is locked to its own width — a fixed-width int edits
    /// only at its declared width, so a host hides the width picker. A plain
    /// register is free to change width.
    pub fn width_locked(&self) -> bool {
        matches!(self.kind, Kind::Fixed { .. })
    }

    /// The narrowest editable width that can hold the current value — the
    /// host grays out smaller picker options (they can't represent it). A
    /// fixed-width value is locked to its own width (its picker is hidden
    /// anyway).
    pub fn minimum_width(&self) -> u32 {
        match self.kind {
            Kind::Fixed { .. } => self.width,
            Kind::Plain => {
                let needed = if self.pattern.is_zero() {
                    1
                } else {
                    self.pattern.bits() as u32
                };
                EDITABLE_WIDTHS
                    .into_iter()
                    .find(|&w| w >= needed)
                    .unwrap_or(MAX_WIDTH)
            }
        }
    }

    /// The bits MSB→LSB, length `width` — index 0 of the vec is the high bit.
    pub fn bits(&self) -> Vec<bool> {
        (0..self.width)
            .rev()
            .map(|i| (&self.pattern >> i) & BigInt::from(1) != BigInt::from(0))
            .collect()
    }

    /// Parse an integer in `base` (2/8/10/16) — the inverse of a field's
    /// `value_text`. A `0x`/`0o`/`0b` prefix always wins over `base`, so a
    /// hex field accepts `1b` or `0x1b`. `None` on malformed input.
    pub fn parse(text: &str, base: u32) -> Option<BigInt> {
        let lower = text.to_lowercase();
        if let Some(body) = lower.strip_prefix("0x") {
            return BigInt::parse_bytes(body.as_bytes(), 16);
        }
        if let Some(body) = lower.strip_prefix("0o") {
            return BigInt::parse_bytes(body.as_bytes(), 8);
        }
        if let Some(body) = lower.strip_prefix("0b") {
            return BigInt::parse_bytes(body.as_bytes(), 2);
        }
        BigInt::parse_bytes(lower.as_bytes(), base)
    }

    /// The current value, reconstructed in its original kind (a fixed-width
    /// int keeps its type and signedness; a plain register is a Number).
    pub fn value(&self) -> Value {
        match self.kind {
            Kind::Plain => Value::Number(BigDecimal::new(self.pattern.clone(), 0)),
            Kind::Fixed { signed } => {
                let decoded = if signed && self.pattern >= (BigInt::from(1) << (self.width - 1)) {
                    &self.pattern - (BigInt::from(1) << self.width)
                } else {
                    self.pattern.clone()
                };
                // In range by construction — `width` is an allowed width and
                // `decoded` sits within it, so the validating constructor
                // cannot fail.
                Value::FixedInt(
                    FixedInt::new(decoded, self.width, signed).expect("in range by construction"),
                )
            }
        }
    }

    /// A new view with bit `index` (0 = LSB) flipped; same kind and width.
    pub fn flipping_bit(&self, index: u32) -> BinaryView {
        assert!(index < self.width, "bit index out of range");
        BinaryView {
            kind: self.kind,
            width: self.width,
            pattern: &self.pattern ^ (BigInt::from(1) << index),
        }
    }

    /// Build a view for `value`, displaying a plain integer at least
    /// `preferred_width` wide (auto-bumped to fit, ignored for a fixed-width
    /// int). The Swift default preferred width is 32.
    pub fn make(value: &Value, preferred_width: u32) -> Result<BinaryView, Unavailable> {
        match value {
            Value::FixedInt(f) => {
                if f.bits > MAX_WIDTH {
                    return Err(Unavailable::TooWide);
                }
                let pat = if f.value < BigInt::from(0) {
                    &f.value + (BigInt::from(1) << f.bits)
                } else {
                    f.value.clone()
                };
                Ok(BinaryView {
                    kind: Kind::Fixed { signed: f.signed },
                    width: f.bits,
                    pattern: pat,
                })
            }
            Value::Number(n) => {
                if !n.is_integer() {
                    return Err(Unavailable::NotAnInteger);
                }
                // exponent ≥ 0 for an integer.
                let magnitude = n.significand() * BigInt::from(10).pow(n.exponent() as u32);
                if magnitude < BigInt::from(0) {
                    return Err(Unavailable::Negative);
                }
                let needed = if magnitude.is_zero() {
                    1
                } else {
                    magnitude.bits() as u32
                };
                if needed > MAX_WIDTH {
                    return Err(Unavailable::TooWide);
                }
                let floor = preferred_width.clamp(1, MAX_WIDTH);
                let width = EDITABLE_WIDTHS
                    .into_iter()
                    .find(|&w| w >= needed && w >= floor)
                    .or_else(|| EDITABLE_WIDTHS.into_iter().find(|&w| w >= needed))
                    .unwrap_or(MAX_WIDTH);
                Ok(BinaryView {
                    kind: Kind::Plain,
                    width,
                    pattern: magnitude,
                })
            }
            _ => Err(Unavailable::NotAnInteger),
        }
    }
}
