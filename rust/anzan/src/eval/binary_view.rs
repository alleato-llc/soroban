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
use super::value::{MapEntry, Value};
use crate::BigDecimal;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};

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

// MARK: - Bit-field formats (named bit ranges)

/// A field in a format: a named bit range. Three flavors, mutually
/// exclusive: a plain NUMERIC field (`flags == None`, `values == None`); a
/// FLAGS field with per-bit names (high→low, count == width) giving each bit
/// a meaning — `owner` as `["r","w","x"]`; or an ENUM field whose unsigned
/// value indexes a label list — `mode` as `["idle","run","halt","max"]`
/// (value 1 → "run"). Plus the RESERVED / UNUSED gap markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: String,
    pub width: u32,
    pub flags: Option<Vec<String>>,
    pub values: Option<Vec<String>>,
    /// A presentational color NAME (the host maps it to a real color); None
    /// means "auto" (the host cycles a palette by position). Opaque to the
    /// engine — it never interprets it.
    pub color: Option<String>,
    /// The radix a NUMERIC field's value is displayed/entered in — 2, 8, 10,
    /// or 16. None (or 10) is decimal; the others read `0b…`/`0o…`/`0x…`.
    /// Presentation only, like `color` — ignored for flags/enum fields.
    pub base: Option<u32>,
    /// A RESERVED gap — locked, must-be-zero bits (display only).
    pub reserved: bool,
    /// An UNUSED gap — don't-care bits: unlabeled, but still editable.
    pub unused: bool,
}

impl FieldSpec {
    /// A plain numeric field — the Swift initializer's defaults. Builder
    /// methods refine it into the other flavors.
    pub fn new(name: impl Into<String>, width: u32) -> Self {
        Self {
            name: name.into(),
            width,
            flags: None,
            values: None,
            color: None,
            base: None,
            reserved: false,
            unused: false,
        }
    }

    pub fn with_flags(mut self, flags: &[&str]) -> Self {
        self.flags = Some(flags.iter().map(|f| f.to_string()).collect());
        self
    }

    pub fn with_values(mut self, values: &[&str]) -> Self {
        self.values = Some(values.iter().map(|v| v.to_string()).collect());
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn with_base(mut self, base: u32) -> Self {
        self.base = Some(base);
        self
    }

    pub fn as_reserved(mut self) -> Self {
        self.reserved = true;
        self
    }

    pub fn as_unused(mut self) -> Self {
        self.unused = true;
        self
    }
}

/// One named bit range decoded from the value. A format packs fields into
/// the LOW bits, listed high→low, so they read left-to-right in the grid as
/// `[unlabeled high bits][f1][f2]…[fN]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub width: u32,
    /// 0 = LSB.
    pub low_bit: u32,
    /// The field's unsigned value.
    pub value: BigInt,
    /// Per-bit flag names (high→low), None = not flags.
    pub flags: Option<Vec<String>>,
    /// Enum value labels (value indexes them), None = not enum.
    pub values: Option<Vec<String>>,
    /// Display radix for a numeric field (2/8/10/16), None = 10.
    pub base: Option<u32>,
    /// A locked, must-be-zero gap (display only).
    pub reserved: bool,
    /// A don't-care gap (unlabeled but editable).
    pub unused: bool,
}

impl Field {
    /// The decoded meaning of a flag field: single-char flags read
    /// positionally with `-` for clear bits (`r-x`); multi-char flags list
    /// only the set ones (`ACK SYN`, or `—` when none). None for non-flags.
    pub fn flag_string(&self) -> Option<String> {
        let flags = self.flags.as_ref()?;
        if flags.iter().all(|f| f.chars().count() == 1) {
            return Some(
                flags
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        if self.is_set_from_top(i) {
                            name.clone()
                        } else {
                            "-".to_string()
                        }
                    })
                    .collect(),
            );
        }
        let set: Vec<String> = flags
            .iter()
            .enumerate()
            .filter(|(i, _)| self.is_set_from_top(*i))
            .map(|(_, name)| name.clone())
            .collect();
        Some(if set.is_empty() {
            "—".to_string()
        } else {
            set.join(" ")
        })
    }

    /// The decoded label of an ENUM field: the value indexes the label list
    /// (`mode` value 2 of `["idle","run","halt","max"]` → "halt"). A value
    /// past the list shows the raw number. None for non-enum.
    pub fn enum_string(&self) -> Option<String> {
        let values = self.values.as_ref()?;
        match self.value.to_usize() {
            Some(index) if index < values.len() => Some(values[index].clone()),
            _ => Some(self.value.to_string()),
        }
    }

    /// A numeric field's value spelled in its display base — `0x1b` (hex),
    /// `0o33` (octal), `0b11011` (binary), or plain decimal. Used for both
    /// the readout and as the editable text.
    pub fn value_text(&self) -> String {
        match self.base.unwrap_or(10) {
            16 => format!("0x{:x}", self.value),
            8 => format!("0o{:o}", self.value),
            2 => format!("0b{:b}", self.value),
            _ => self.value.to_string(),
        }
    }

    /// The field's human-readable decode — enum label, flag string, or the
    /// numeric value in its base — whichever applies.
    pub fn label(&self) -> String {
        self.enum_string()
            .or_else(|| self.flag_string())
            .unwrap_or_else(|| self.value_text())
    }

    /// Is the flag at position `i` (0 = the field's high bit) set?
    pub fn is_set_from_top(&self, i: usize) -> bool {
        (&self.value >> (self.width as usize - 1 - i)) & BigInt::from(1) == BigInt::from(1)
    }
}

impl BinaryView {
    /// Parse a layout from either a MAP — each entry's value a positive
    /// integer bit WIDTH (`owner: 3`), an array of per-bit FLAG names
    /// (`owner: ["r","w","x"]`, width = count), or a richer field map
    /// (`{bits, base}` numeric, `{bits, values}` enum, `{bits, reserved}` /
    /// `{bits, unused}` gap) — OR a typed `Bits::BitFormat` RECORD with a
    /// `fields` list of `BitField` records (`name`, `bits`, `kind`, `flags`,
    /// `values`, `color`, `base`), read structurally by field name. Insertion
    /// order is preserved (first = highest field). `None` if it's neither
    /// shape.
    pub fn layout(value: &Value) -> Option<Vec<FieldSpec>> {
        match value {
            Value::Map(entries) => {
                if entries.is_empty() {
                    return None;
                }
                let mut layout: Vec<FieldSpec> = Vec::new();
                for entry in entries {
                    match &entry.value {
                        Value::Number(n) => {
                            let width = integer_width(n)?;
                            layout.push(FieldSpec::new(&entry.key, width));
                        }
                        Value::Array(items) => {
                            if items.is_empty() {
                                return None;
                            }
                            let names = string_items(items)?;
                            let width = names.len() as u32;
                            layout.push(FieldSpec {
                                flags: Some(names),
                                ..FieldSpec::new(&entry.key, width)
                            });
                        }
                        Value::Map(inner) => {
                            // A richer field map: `{bits, base}` numeric,
                            // `{bits, values}` enum, or `{bits, reserved}` /
                            // `{bits, unused}` gap.
                            let Some(Value::Number(n)) = member(inner, "bits") else {
                                return None;
                            };
                            let width = integer_width(n)?;
                            if flag_set(member(inner, "reserved")) {
                                layout.push(FieldSpec::new(&entry.key, width).as_reserved());
                            } else if flag_set(member(inner, "unused")) {
                                layout.push(FieldSpec::new(&entry.key, width).as_unused());
                            } else if let Some(values) =
                                string_list(member(inner, "values")).filter(|v| !v.is_empty())
                            {
                                layout.push(FieldSpec {
                                    values: Some(values),
                                    ..FieldSpec::new(&entry.key, width)
                                });
                            } else {
                                layout.push(FieldSpec {
                                    base: normalized_base(member(inner, "base")),
                                    ..FieldSpec::new(&entry.key, width)
                                });
                            }
                        }
                        _ => return None,
                    }
                }
                Some(layout)
            }

            Value::Record(record) => {
                // A BitFormat-shaped record: a `fields` list of BitField
                // records. Each field is a flags / enum / numeric field —
                // chosen by `kind` when the record carries it, else derived
                // from which list is non-empty.
                let Some(Value::Array(field_values)) = member(&record.entries, "fields") else {
                    return None;
                };
                if field_values.is_empty() {
                    return None;
                }
                let mut layout: Vec<FieldSpec> = Vec::new();
                for field_value in field_values {
                    let Value::Record(field) = field_value else {
                        return None;
                    };
                    let Some(Value::String(name)) = member(&field.entries, "name") else {
                        return None;
                    };
                    let kind = match member(&field.entries, "kind") {
                        Some(Value::String(k)) => Some(k.as_str()),
                        _ => None,
                    };
                    let color = match member(&field.entries, "color") {
                        Some(Value::String(c)) if !c.is_empty() => Some(c.clone()),
                        _ => None,
                    };
                    let base = normalized_base(member(&field.entries, "base"));
                    let flags = string_list(member(&field.entries, "flags"));
                    let values = string_list(member(&field.entries, "values"));
                    let bits_width = match member(&field.entries, "bits") {
                        Some(Value::Number(n)) => integer_width(n),
                        _ => None,
                    };
                    if kind == Some("reserved") {
                        let width = bits_width?;
                        layout.push(FieldSpec {
                            color,
                            ..FieldSpec::new(name, width).as_reserved()
                        });
                    } else if kind == Some("unused") {
                        let width = bits_width?;
                        layout.push(FieldSpec {
                            color,
                            ..FieldSpec::new(name, width).as_unused()
                        });
                    } else if matches!(kind, Some("flags") | None)
                        && flags.as_ref().is_some_and(|f| !f.is_empty())
                    {
                        let flags = flags.unwrap();
                        let width = flags.len() as u32;
                        layout.push(FieldSpec {
                            flags: Some(flags),
                            color,
                            ..FieldSpec::new(name, width)
                        });
                    } else if matches!(kind, Some("enum") | None)
                        && values.as_ref().is_some_and(|v| !v.is_empty())
                        && bits_width.is_some()
                    {
                        let width = bits_width?;
                        layout.push(FieldSpec {
                            values,
                            color,
                            ..FieldSpec::new(name, width)
                        });
                    } else if let Some(width) = bits_width {
                        layout.push(FieldSpec {
                            color,
                            base,
                            ..FieldSpec::new(name, width)
                        });
                    } else {
                        return None;
                    }
                }
                Some(layout)
            }

            _ => None,
        }
    }

    /// Build a loose-map format `Value` from an explicit layout — the
    /// general constructor that also encodes enum / reserved / unused fields
    /// (which the homogeneous `format_map` / `flag_format_map` /
    /// `numeric_format_map` can't). Round-trips through `layout`.
    pub fn format_value(layout: &[FieldSpec]) -> Value {
        Value::Map(
            layout
                .iter()
                .map(|spec| {
                    let value = if spec.reserved {
                        Value::Map(vec![
                            MapEntry::new("bits", number(spec.width)),
                            MapEntry::new("reserved", number(1)),
                        ])
                    } else if spec.unused {
                        Value::Map(vec![
                            MapEntry::new("bits", number(spec.width)),
                            MapEntry::new("unused", number(1)),
                        ])
                    } else if let Some(flags) = spec.flags.as_ref().filter(|f| !f.is_empty()) {
                        Value::Array(flags.iter().map(|f| Value::String(f.clone())).collect())
                    } else if let Some(values) = spec.values.as_ref().filter(|v| !v.is_empty()) {
                        Value::Map(vec![
                            MapEntry::new("bits", number(spec.width)),
                            MapEntry::new(
                                "values",
                                Value::Array(
                                    values.iter().map(|v| Value::String(v.clone())).collect(),
                                ),
                            ),
                        ])
                    } else if let Some(base) = spec.base {
                        Value::Map(vec![
                            MapEntry::new("bits", number(spec.width)),
                            MapEntry::new("base", number(base)),
                        ])
                    } else {
                        number(spec.width)
                    };
                    MapEntry::new(spec.name.clone(), value)
                })
                .collect(),
        )
    }

    /// Build a format map from numeric (name, width) pairs — the inverse of
    /// `layout`.
    pub fn format_map(pairs: &[(&str, u32)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(name, width)| MapEntry::new(*name, number(*width)))
                .collect(),
        )
    }

    /// Build a format map from flag fields — each value is an array of
    /// per-bit flag names (`owner: ["r","w","x"]`).
    pub fn flag_format_map(pairs: &[(&str, &[&str])]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(name, flags)| {
                    MapEntry::new(
                        *name,
                        Value::Array(flags.iter().map(|f| Value::String(f.to_string())).collect()),
                    )
                })
                .collect(),
        )
    }

    /// Build a format map of numeric fields that carry a display BASE — each
    /// value is a `{bits, base}` map (`octet: {bits: 8, base: 16}`), the
    /// form `layout` reads back into a based numeric field.
    pub fn numeric_format_map(pairs: &[(&str, u32, u32)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(name, width, base)| {
                    MapEntry::new(
                        *name,
                        Value::Map(vec![
                            MapEntry::new("bits", number(*width)),
                            MapEntry::new("base", number(*base)),
                        ]),
                    )
                })
                .collect(),
        )
    }

    /// The total width a layout occupies.
    pub fn layout_width(layout: &[FieldSpec]) -> u32 {
        layout.iter().map(|f| f.width).sum()
    }

    /// Decode the current value into `layout`'s fields (high→low, matching
    /// the grid). Bits above the layout's total are simply unlabeled.
    pub fn fields(&self, layout: &[FieldSpec]) -> Vec<Field> {
        let mut top = Self::layout_width(layout) as i64;
        layout
            .iter()
            .map(|f| {
                let low = top - f.width as i64;
                top = low;
                let mask = (BigInt::from(1) << f.width) - 1;
                let value = if low >= 0 {
                    (&self.pattern >> (low as u32)) & mask
                } else {
                    BigInt::from(0)
                };
                Field {
                    name: f.name.clone(),
                    width: f.width,
                    low_bit: low.max(0) as u32,
                    value,
                    flags: f.flags.clone(),
                    values: f.values.clone(),
                    base: f.base,
                    reserved: f.reserved,
                    unused: f.unused,
                }
            })
            .collect()
    }

    /// A new view with field `name` set to `value` (clamped to the field's
    /// width), every other bit unchanged. Unknown name → unchanged.
    pub fn setting_field(&self, name: &str, value: &BigInt, layout: &[FieldSpec]) -> BinaryView {
        let mut top = Self::layout_width(layout) as i64;
        let register_mask = (BigInt::from(1) << self.width) - 1;
        for f in layout {
            let low = top - f.width as i64;
            top = low;
            if f.name != name || low < 0 {
                continue;
            }
            let low = low as u32;
            let field_mask = ((BigInt::from(1) << f.width) - 1) << low;
            let cleared = &self.pattern & (&register_mask ^ &field_mask);
            let placed = (value.max(&BigInt::from(0)) << low) & &field_mask;
            return BinaryView {
                kind: self.kind,
                width: self.width,
                pattern: cleared | placed,
            };
        }
        self.clone()
    }
}

fn number(value: u32) -> Value {
    Value::Number(BigDecimal::new(BigInt::from(value), 0))
}

fn member<'a>(entries: &'a [MapEntry], key: &str) -> Option<&'a Value> {
    entries.iter().find(|e| e.key == key).map(|e| &e.value)
}

fn string_items(items: &[Value]) -> Option<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    for item in items {
        let Value::String(name) = item else {
            return None;
        };
        names.push(name.clone());
    }
    Some(names)
}

fn string_list(value: Option<&Value>) -> Option<Vec<String>> {
    let Some(Value::Array(items)) = value else {
        return None;
    };
    string_items(items)
}

/// A positive integer bit width from a number, or `None`.
fn integer_width(n: &BigDecimal) -> Option<u32> {
    if !n.is_integer() {
        return None;
    }
    let width = n.int_value()?;
    if width >= 1 {
        u32::try_from(width).ok()
    } else {
        None
    }
}

/// A display radix from a `base` member — only 2/8/10/16 are honored; 10
/// and anything else collapse to `None` (decimal). Keeps a stray value from
/// picking a nonsense radix.
fn normalized_base(value: Option<&Value>) -> Option<u32> {
    let Some(Value::Number(n)) = value else {
        return None;
    };
    if !n.is_integer() {
        return None;
    }
    match n.int_value()? {
        2 => Some(2),
        8 => Some(8),
        16 => Some(16),
        _ => None,
    }
}

/// A boolean-ish loose-map flag — Anzan has no Bool, so "true" is the
/// number 1.
fn flag_set(value: Option<&Value>) -> bool {
    if let Some(Value::Number(n)) = value {
        return !n.is_zero();
    }
    false
}
