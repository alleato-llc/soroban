//! Bit-field format decoding/encoding: parse a layout from a loose map or a
//! typed `Bits::BitFormat` record, build format `Value`s from a layout, and
//! decode/update a register's fields against a layout.

use super::fields::{Field, FieldSpec};
use super::BinaryView;
use crate::eval::value::{MapEntry, Value};
use crate::BigDecimal;
use num_bigint::BigInt;

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
