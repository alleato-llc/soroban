//! The model behind the app's visual bit-field builder — pure and host-free,
//! so the UI view is just bindings over this. You claim a contiguous run of
//! the open bits (`claim`), describe the pending field with the `draft_*`
//! inputs, then `add_field`; the accumulated `fields` produce a `layout`
//! (`Vec<FieldSpec>`) that drives the editor and saves as a `Bits::BitFormat`.

use super::binary_view::FieldSpec;

/// The kind of the field being built — the builder's editable picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Numeric,
    Flags,
    Enumeration,
    Reserved,
    Unused,
}

impl FieldKind {
    /// Every kind, in the Swift `CaseIterable` order (the picker's rows).
    pub const ALL: [FieldKind; 5] = [
        FieldKind::Numeric,
        FieldKind::Flags,
        FieldKind::Enumeration,
        FieldKind::Reserved,
        FieldKind::Unused,
    ];

    /// The Swift raw value — the UI label; lowercased it names an anonymous
    /// gap field ("reserved" / "unused").
    pub fn raw_value(self) -> &'static str {
        match self {
            FieldKind::Numeric => "Numeric",
            FieldKind::Flags => "Flags",
            FieldKind::Enumeration => "Enum",
            FieldKind::Reserved => "Reserved",
            FieldKind::Unused => "Unused",
        }
    }
}

/// One field as the builder holds it (richer than `FieldSpec`: it keeps the
/// editable `kind` + raw label list + a stable id for the UI list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: usize,
    pub name: String,
    pub width: u32,
    pub kind: FieldKind,
    /// flags: per-bit names; enum: value labels.
    pub labels: Vec<String>,
    pub color_name: String,
    /// Numeric display radix (10 decimal, 16 hex).
    pub base: u32,
}

impl Field {
    /// The engine `FieldSpec` this field becomes — flags padded/truncated to
    /// the bit width, enum labels as-is, base dropped when decimal.
    pub fn spec(&self) -> FieldSpec {
        match self.kind {
            FieldKind::Numeric => {
                let spec = FieldSpec::new(&self.name, self.width).with_color(&self.color_name);
                if self.base == 10 {
                    spec
                } else {
                    spec.with_base(self.base)
                }
            }
            FieldKind::Flags => {
                let mut flags = self.labels.clone();
                while (flags.len() as u32) < self.width {
                    flags.push("?".to_string());
                }
                flags.truncate(self.width as usize);
                FieldSpec {
                    flags: Some(flags),
                    ..FieldSpec::new(&self.name, self.width).with_color(&self.color_name)
                }
            }
            FieldKind::Enumeration => FieldSpec {
                values: Some(self.labels.clone()),
                ..FieldSpec::new(&self.name, self.width).with_color(&self.color_name)
            },
            FieldKind::Reserved => FieldSpec::new(&self.name, self.width)
                .with_color(&self.color_name)
                .as_reserved(),
            FieldKind::Unused => FieldSpec::new(&self.name, self.width)
                .with_color(&self.color_name)
                .as_unused(),
        }
    }
}

/// The visual bit-field builder — Swift's `BinaryView.FormatBuilder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatBuilder {
    palette: Vec<String>,
    next_id: usize,
    fields: Vec<Field>,
    /// Bits claimed for the field about to be added (0 = none claimed).
    pending_width: u32,

    // The pending field's editable inputs (the view binds these directly).
    pub draft_name: String,
    pub draft_kind: FieldKind,
    /// Comma-separated, for flags/enum.
    pub draft_labels: String,
    pub draft_color: String,
    pub draft_base: u32,
}

impl FormatBuilder {
    pub fn new(palette: &[&str]) -> Self {
        let palette: Vec<String> = if palette.is_empty() {
            vec!["blue".to_string()]
        } else {
            palette.iter().map(|name| name.to_string()).collect()
        };
        let draft_color = palette[0].clone();
        FormatBuilder {
            palette,
            next_id: 0,
            fields: Vec::new(),
            pending_width: 0,
            draft_name: String::new(),
            draft_kind: FieldKind::Numeric,
            draft_labels: String::new(),
            draft_color,
            draft_base: 10,
        }
    }

    // MARK: Derived

    /// The committed fields, in order (read-only; mutate via the methods).
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Bits claimed for the field about to be added (0 = none claimed).
    pub fn pending_width(&self) -> u32 {
        self.pending_width
    }

    pub fn committed_width(&self) -> u32 {
        self.fields.iter().map(|f| f.width).sum()
    }

    pub fn free_bits(&self, register_width: u32) -> u32 {
        register_width.saturating_sub(self.committed_width())
    }

    pub fn layout(&self) -> Vec<FieldSpec> {
        self.fields.iter().map(Field::spec).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Reserved and Unused are nameless "gap" fields (no name required).
    pub fn is_gap_kind(&self) -> bool {
        matches!(self.draft_kind, FieldKind::Reserved | FieldKind::Unused)
    }

    pub fn can_add_field(&self) -> bool {
        self.pending_width >= 1 && (self.is_gap_kind() || !self.draft_name.trim().is_empty())
    }

    // MARK: Mutation

    /// Claim a `bits`-wide pending group; clicking the same far edge clears
    /// it.
    pub fn claim(&mut self, bits: u32) {
        self.pending_width = if self.pending_width == bits { 0 } else { bits };
    }

    /// Commit the pending field from the draft inputs, then reset the draft
    /// (advancing the default color so successive fields differ). No-op when
    /// `can_add_field` is false.
    pub fn add_field(&mut self) {
        let trimmed = self.draft_name.trim().to_string();
        if self.pending_width < 1 || (!self.is_gap_kind() && trimmed.is_empty()) {
            return;
        }
        let name = if self.is_gap_kind() && trimmed.is_empty() {
            self.draft_kind.raw_value().to_lowercase()
        } else {
            trimmed
        };
        let labels = if matches!(self.draft_kind, FieldKind::Flags | FieldKind::Enumeration) {
            Self::parse_labels(&self.draft_labels)
        } else {
            Vec::new()
        };
        self.fields.push(Field {
            id: self.next_id,
            name,
            width: self.pending_width,
            kind: self.draft_kind,
            labels,
            color_name: self.draft_color.clone(),
            base: self.draft_base,
        });
        self.next_id += 1;
        self.reset_draft();
    }

    pub fn remove(&mut self, id: usize) {
        self.fields.retain(|f| f.id != id);
        self.pending_width = 0;
    }

    pub fn recolor(&mut self, id: usize, name: &str) {
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == id) {
            field.color_name = name.to_string();
        }
    }

    /// Rebuild from an existing layout, so an active format can be tweaked.
    pub fn seed(&mut self, layout: &[FieldSpec]) {
        self.fields = layout
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let color = spec
                    .color
                    .clone()
                    .unwrap_or_else(|| self.palette[i % self.palette.len()].clone());
                let (kind, labels, base) = if spec.reserved {
                    (FieldKind::Reserved, Vec::new(), 10)
                } else if spec.unused {
                    (FieldKind::Unused, Vec::new(), 10)
                } else if let Some(flags) = &spec.flags {
                    (FieldKind::Flags, flags.clone(), 10)
                } else if let Some(values) = &spec.values {
                    (FieldKind::Enumeration, values.clone(), 10)
                } else {
                    (FieldKind::Numeric, Vec::new(), spec.base.unwrap_or(10))
                };
                Field {
                    id: i,
                    name: spec.name.clone(),
                    width: spec.width,
                    kind,
                    labels,
                    color_name: color,
                    base,
                }
            })
            .collect();
        self.next_id = self.fields.len();
        self.reset_draft();
    }

    fn reset_draft(&mut self) {
        self.pending_width = 0;
        self.draft_name.clear();
        self.draft_kind = FieldKind::Numeric;
        self.draft_labels.clear();
        self.draft_base = 10;
        self.draft_color = self.palette[self.fields.len() % self.palette.len()].clone();
    }

    fn parse_labels(text: &str) -> Vec<String> {
        text.split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .collect()
    }
}
