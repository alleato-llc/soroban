//! The binary bit-editor: the editable register draft, width picker, bit-format
//! presets/layouts, the visual format builder, and field decode/encode.

use super::*;

impl Session {
    // MARK: Binary editor (slice ⑤)

    /// The last computed result — the value the bit editor edits.
    fn ans(&self) -> Value {
        self.calculator.borrow().environment().ans()
    }

    /// (Re)build the bit-editor draft from `ans` at the preferred width. Called
    /// when the editor opens and after each submit, so it tracks the latest
    /// result until you flip a bit (which stages a draft of its own). The active
    /// format layout carries over.
    pub fn refresh_binary(&mut self) {
        self.binary = BinaryView::make(&self.ans(), self.binary_width).ok();
    }

    /// Flip bit `index` (0 = LSB) of the draft, staging a new pattern.
    pub fn flip_binary_bit(&mut self, index: usize) {
        if let Some(view) = &self.binary {
            if (index as u32) < view.width {
                self.binary = Some(view.flipping_bit(index as u32));
            }
        }
    }

    /// The editor's current state: the editable grid, or why `ans` can't be
    /// edited as bits. `bits` is **LSB-first** (`bits[0]` = bit 0), matching the
    /// widget and `flip_binary_bit` — `BinaryView::bits()` is MSB-first, so we
    /// reverse it here.
    pub fn binary_status(&self) -> BinaryStatus {
        if let Some(view) = &self.binary {
            let mut bits = view.bits(); // MSB-first from the engine…
            bits.reverse(); // …flipped to the widget's LSB-first contract.
            return BinaryStatus::Editable {
                bits,
                value: view.value().display_description(),
                hex: BigDecimal::new(view.pattern.clone(), 0)
                    .hex_text()
                    .unwrap_or_default(),
                width: view.width,
                signed: view.signed(),
                locked: view.width_locked(),
            };
        }
        let reason = match BinaryView::make(&self.ans(), self.binary_width) {
            Ok(_) => "Compute a value, then open the bit editor.".to_string(),
            Err(reason) => binary_reason(reason),
        };
        BinaryStatus::Unavailable(reason)
    }

    /// The register widths offered in the picker (empty when the editor is
    /// closed or the value is locked to a fixed width). A width too small to
    /// hold the current value or the active format is `enabled: false`.
    pub fn binary_widths(&self) -> Vec<BinaryWidth> {
        let Some(view) = &self.binary else {
            return Vec::new();
        };
        if view.width_locked() {
            return Vec::new();
        }
        let floor = view.minimum_width().max(self.layout_min_width());
        BINARY_EDITABLE_WIDTHS
            .into_iter()
            .map(|bits| BinaryWidth {
                bits,
                enabled: bits >= floor,
                active: bits == view.width,
            })
            .collect()
    }

    /// Re-open the draft at `width` (keeping the current value and format).
    /// Ignored when the value can't be represented, or is locked to its width.
    pub fn set_binary_width(&mut self, width: u32) {
        let Some(view) = &self.binary else { return };
        if view.width_locked() || width < view.minimum_width().max(self.layout_min_width()) {
            return;
        }
        if let Ok(rebuilt) = BinaryView::make(&view.value(), width) {
            self.binary_width = width;
            self.binary = Some(rebuilt);
        }
    }

    /// The names of the built-in format presets, in menu order (always
    /// available — the picker offers them whenever the editor is open).
    pub fn binary_preset_names(&self) -> Vec<String> {
        BinaryEditorPresets::standard()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect()
    }

    /// The active bit-format's name, or `None` for a plain register.
    pub fn binary_format_name(&self) -> Option<String> {
        self.binary_format_name.clone()
    }

    /// Apply a named format (or `None` to clear back to a plain register): a
    /// built-in preset, else a saved custom format (a user variable holding a
    /// layout-shaped map). Bumps the register width up if the layout needs more
    /// bits. An unknown name is a no-op.
    pub fn apply_binary_format(&mut self, name: Option<&str>) {
        let Some(name) = name else {
            self.binary_layout = None;
            self.binary_format_name = None;
            return;
        };
        let layout = BinaryEditorPresets::standard()
            .into_iter()
            .find(|(preset, _)| *preset == name)
            .and_then(|(_, value)| BinaryView::layout(&value))
            .or_else(|| {
                let calc = self.calculator.borrow();
                calc.environment()
                    .user_variables()
                    .get(name)
                    .and_then(BinaryView::layout)
            });
        if let Some(layout) = layout {
            self.install_layout(name, layout);
        }
    }

    /// Make `layout` the active format under `name`, widening the register to
    /// fit if it's currently too narrow.
    fn install_layout(&mut self, name: &str, layout: Vec<BinaryFieldSpec>) {
        let needed = BinaryView::layout_width(&layout);
        self.binary_format_name = Some(name.to_string());
        self.binary_layout = Some(layout);
        if let Some(view) = &self.binary {
            if view.width < needed && !view.width_locked() {
                if let Some(fit) = BINARY_EDITABLE_WIDTHS.into_iter().find(|&w| w >= needed) {
                    self.set_binary_width(fit);
                }
            }
        }
    }

    /// The names of saved custom formats — user variables whose value decodes
    /// as a bit-format layout (the same "any map/record `layout` accepts" rule
    /// as the AppKit app). Sorted; offered in the picker after the presets.
    pub fn saved_format_names(&self) -> Vec<String> {
        let calc = self.calculator.borrow();
        let mut names: Vec<String> = calc
            .environment()
            .user_variables()
            .iter()
            .filter(|(_, value)| BinaryView::layout(value).is_some())
            .map(|(name, _)| name.clone())
            .collect();
        names.sort();
        names
    }

    // MARK: Format builder (Build new… / Edit current… / Save current…)

    /// Open the visual builder. With `seed_active`, it starts from the fields
    /// of the current format (Edit current…), else empty (Build new…).
    pub fn begin_format_build(&mut self, seed_active: bool) {
        let mut builder = FormatBuilder::new(&BINARY_PALETTE);
        if seed_active {
            if let Some(layout) = &self.binary_layout {
                builder.seed(layout);
            }
        }
        self.format_builder = Some(builder);
    }

    /// Close the builder without applying.
    pub fn cancel_format_build(&mut self) {
        self.format_builder = None;
    }

    /// The live builder, for the shell to render (fields, drafts, free bits).
    pub fn format_builder(&self) -> Option<&FormatBuilder> {
        self.format_builder.as_ref()
    }

    /// The live builder, for message handlers to drive (claim, add, remove,
    /// draft inputs).
    pub fn format_builder_mut(&mut self) -> Option<&mut FormatBuilder> {
        self.format_builder.as_mut()
    }

    /// Apply the builder's fields as the active format without saving —
    /// SpeedCrunch's transient "Apply" (the builder stays open).
    pub fn apply_built_format(&mut self) {
        let Some(builder) = &self.format_builder else {
            return;
        };
        if builder.is_empty() {
            return;
        }
        let layout = builder.layout();
        self.install_layout("Custom", layout);
    }

    /// Persist the builder's fields as a saved format named `name` (a user
    /// variable, so it rides the workbook), apply it, and close the builder.
    /// Returns false when the name is blank or no fields were built.
    pub fn save_format(&mut self, name: &str) -> bool {
        let name = name.trim().to_string();
        let Some(builder) = &self.format_builder else {
            return false;
        };
        if name.is_empty() || builder.is_empty() {
            return false;
        }
        let layout = builder.layout();
        // A loose-map value round-trips through `layout` and persists as a
        // workbook variable — set off-log so it never disturbs `ans`.
        let value = BinaryView::format_value(&layout);
        self.calculator.borrow_mut().set_user_variable(&name, value);
        self.format_builder = None;
        self.install_layout(&name, layout);
        self.revision += 1;
        true
    }

    /// Delete a saved format (removing its backing user variable). Clears the
    /// active format when it was the one deleted.
    pub fn delete_saved_format(&mut self, name: &str) {
        self.calculator.borrow_mut().remove_user_variable(name);
        if self.binary_format_name.as_deref() == Some(name) {
            self.binary_layout = None;
            self.binary_format_name = None;
        }
        self.revision += 1;
    }

    /// The active format's fields, decoded from the current value (empty for a
    /// plain register) — named ranges with their color, decoded readout, and
    /// everything the shell needs to render the right editor (a numeric input,
    /// an enum picker, or per-bit flag cells).
    pub fn binary_fields(&self) -> Vec<BinaryFieldView> {
        let (Some(view), Some(layout)) = (&self.binary, &self.binary_layout) else {
            return Vec::new();
        };
        let palette = BINARY_PALETTE;
        layout
            .iter()
            .zip(view.fields(layout))
            .enumerate()
            .map(|(index, (spec, field))| {
                let kind = if field.reserved {
                    BinaryFieldKind::Reserved
                } else if field.unused {
                    BinaryFieldKind::Unused
                } else if field.flags.is_some() {
                    BinaryFieldKind::Flags
                } else if field.values.is_some() {
                    BinaryFieldKind::Enum
                } else {
                    BinaryFieldKind::Numeric
                };
                // Enum selection: the value indexes the labels when in range.
                let (options, selected) = match &field.values {
                    Some(values) => {
                        let index = field.value.to_string().parse::<usize>().ok();
                        (values.clone(), index.filter(|&i| i < values.len()))
                    }
                    None => (Vec::new(), None),
                };
                // Flag bits, high→low, each with its absolute register bit.
                let flags = field
                    .flags
                    .as_ref()
                    .map(|names| {
                        names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| BinaryFlagBit {
                                name: name.clone(),
                                // flag i is the field's high bit minus i.
                                bit: field.low_bit + field.width - 1 - i as u32,
                                set: field.is_set_from_top(i),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                BinaryFieldView {
                    name: field.name.clone(),
                    low_bit: field.low_bit,
                    width: field.width,
                    color: spec
                        .color
                        .clone()
                        .or_else(|| Some(palette[index % palette.len()].to_string())),
                    label: field.label(),
                    kind,
                    value_text: field.value_text(),
                    options,
                    selected,
                    flags,
                    reserved: field.reserved,
                    unused: field.unused,
                }
            })
            .collect()
    }

    /// Set the field named `name` to `text`, parsed in the field's display base
    /// (a numeric field's `0x1b`/`755`, or an enum's selected index as a plain
    /// number). Clamped to the field's width by the engine. Returns false when
    /// there's no active format, no such field, or the text won't parse.
    pub fn set_binary_field(&mut self, name: &str, text: &str) -> bool {
        let (Some(view), Some(layout)) = (&self.binary, &self.binary_layout) else {
            return false;
        };
        let Some(spec) = layout.iter().find(|f| f.name == name) else {
            return false;
        };
        // Enum/flags carry no base; a numeric field reads in its own.
        let base = spec.base.unwrap_or(10);
        let Some(value) = BinaryView::parse(text, base) else {
            return false;
        };
        self.binary = Some(view.setting_field(name, &value, layout));
        true
    }

    /// The active format's total bit width (0 when none) — the register can't
    /// be narrower than this.
    fn layout_min_width(&self) -> u32 {
        self.binary_layout
            .as_ref()
            .map(|layout| BinaryView::layout_width(layout))
            .unwrap_or(0)
    }

    /// Drop the draft's value into the input line, ready to fold into an
    /// expression (the SpeedCrunch "Use" action).
    pub fn use_binary(&mut self) {
        if let Some(view) = &self.binary {
            self.input = view.value().display_description();
            self.history_cursor = None;
        }
    }
}

/// A human explanation of why a value can't be edited as bits.
fn binary_reason(reason: BinaryViewUnavailable) -> String {
    match reason {
        BinaryViewUnavailable::NotAnInteger => "The bit editor needs a whole number.".to_string(),
        BinaryViewUnavailable::Negative => {
            "Negative — wrap it in a signed Int type (e.g. Int32).".to_string()
        }
        BinaryViewUnavailable::TooWide => "Too wide — over 256 bits.".to_string(),
    }
}
