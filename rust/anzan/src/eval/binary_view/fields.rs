//! Bit-field format types: a `FieldSpec` (a named bit range in a format) and
//! the decoded `Field` (a range's value + how to render it — numeric/flags/
//! enum/gap).

use num_bigint::BigInt;
use num_traits::ToPrimitive;

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
