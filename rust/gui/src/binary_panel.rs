//! The binary bit-editor strip and its visual format builder.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use rime::icons::{self, glyph};
use rime::theme;
use rime::widgets::{bit_grid, button, card, select, text_field, BitBand};
use soroban_engine::FormatBuilderFieldKind;
use soroban_gui::session::{BinaryFieldKind, BinaryStatus};

use crate::render::*;
use crate::{App, Message};

impl App {
    /// The binary bit-editor strip: value + hex header, a width picker, a
    /// bit-format dropdown, and a clickable bit grid tinted by the active
    /// format's named fields, plus a Use button that drops the value into the
    /// input.
    pub(crate) fn binary_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        let content: Element<'_, Message> = match self.session.binary_status() {
            BinaryStatus::Editable {
                bits,
                value,
                hex,
                width,
                signed,
                // `binary_widths()` is already empty for a locked (fixed-width)
                // value, so the width picker hides itself; nothing to do here.
                locked: _,
            } => {
                let caption = format!(
                    "{value}   {hex}   ·   {width}-bit {}",
                    if signed { "signed" } else { "unsigned" }
                );

                // The format picker: "None", every preset, then saved formats.
                let none = "None".to_string();
                let mut options = vec![none.clone()];
                options.extend(self.session.binary_preset_names());
                options.extend(self.session.saved_format_names());
                let current = self.session.binary_format_name().unwrap_or(none.clone());
                let format_picker = select(options, Some(current), move |chosen: String| {
                    if chosen == "None" {
                        Message::SetBinaryFormat(None)
                    } else {
                        Message::SetBinaryFormat(Some(chosen))
                    }
                });
                // "Build" (new) and, when a format is active, "Edit" it.
                let mut build_actions =
                    row![button::ghost("Build…", Message::BeginBuildFormat(false))].spacing(6);
                if self.session.binary_format_name().is_some() {
                    build_actions =
                        build_actions.push(button::ghost("Edit…", Message::BeginBuildFormat(true)));
                }

                let header = row![
                    text(caption).font(font).size(13).color(palette.accent),
                    container(row![build_actions, format_picker].spacing(8))
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                    button::secondary("Use in input", Message::UseBinary),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center);

                // The width picker (hidden when the value is locked to its
                // width): one chip per width, disabled below the minimum.
                let widths = self.session.binary_widths();
                let mut layout = column![header].spacing(12);
                if !widths.is_empty() {
                    let mut chips = row![].spacing(6);
                    for w in widths {
                        let mut chip =
                            iced::widget::button(text(w.bits.to_string()).size(12).center())
                                .padding([4, 10])
                                .style(width_chip_style(w.active, w.enabled, *palette));
                        if w.enabled && !w.active {
                            chip = chip.on_press(Message::SetBinaryWidth(w.bits));
                        }
                        chips = chips.push(chip);
                    }
                    layout = layout.push(chips);
                }

                // Decode the active format into named bands for the grid; rime
                // cycles its palette by position (owner=blue, group=green, …),
                // matching the AppKit app's field coloring.
                let bands: Vec<BitBand> = self
                    .session
                    .binary_fields()
                    .into_iter()
                    .map(|f| {
                        let label = if f.label.is_empty() {
                            f.name.clone()
                        } else {
                            format!("{} {}", f.name, f.label)
                        };
                        BitBand::new(label, f.low_bit as usize, f.width as usize)
                    })
                    .collect();

                layout = layout.push(scrollable(bit_grid(bits, bands, Message::BitToggled)));
                // Per-field editors below the grid (enum pickers, numeric
                // inputs, flag chips) — empty for a plain register.
                if let Some(fields) = self.binary_fields_view(palette) {
                    layout = layout.push(fields);
                }
                // The visual builder, when Build…/Edit… is open.
                if let Some(builder) = self.format_builder_view(width, palette) {
                    layout = layout.push(builder);
                }
                layout.into()
            }
            BinaryStatus::Unavailable(reason) => text(reason).size(13).color(palette.muted).into(),
        };

        container(card(
            column![
                Self::panel_header("Binary", Message::ToggleBinary, palette),
                content
            ]
            .spacing(12),
        ))
        .padding(iced::Padding {
            top: 0.0,
            right: 20.0,
            bottom: 20.0,
            left: 20.0,
        })
        .into()
    }

    /// Rebuild the numeric-field draft map from the register's current values,
    /// so a text input can borrow its value from `self` (living as long as the
    /// view) and re-syncs after any bit/width/format/field change.
    pub(crate) fn sync_binary_field_drafts(&mut self) {
        self.binary_field_drafts = self
            .session
            .binary_fields()
            .into_iter()
            .filter(|f| matches!(f.kind, BinaryFieldKind::Numeric | BinaryFieldKind::Unused))
            .map(|f| (f.name, f.value_text))
            .collect();
    }

    /// The per-field editor strip for the active bit-format: one card per field
    /// carrying the right control — a picker for an enum, a text input for a
    /// numeric field, clickable chips for flags, a dimmed lock for a reserved
    /// gap. `None` when no format is applied.
    pub(crate) fn binary_fields_view(
        &self,
        palette: &theme::Palette,
    ) -> Option<Element<'_, Message>> {
        // Own the palette (it's Copy) so the field-card closures don't borrow it
        // — the returned Element then borrows only `self`.
        let palette = *palette;
        let fields = self.session.binary_fields();
        if fields.is_empty() {
            return None;
        }
        let mut cards = row![].spacing(10);
        for f in fields {
            let name = f.name.clone();
            let header = text(format!(
                "{} [{}:{}]",
                f.name,
                f.low_bit + f.width - 1,
                f.low_bit
            ))
            .size(11)
            .color(palette.muted);
            let editor: Element<'_, Message> = match f.kind {
                BinaryFieldKind::Enum => {
                    let options = f.options.clone();
                    let selected = f.selected.and_then(|i| options.get(i).cloned());
                    let lookup = options.clone();
                    let field_name = name.clone();
                    select(options, selected, move |chosen: String| {
                        let index = lookup.iter().position(|o| *o == chosen).unwrap_or(0);
                        Message::SetBinaryField(field_name.clone(), index.to_string())
                    })
                    .into()
                }
                BinaryFieldKind::Numeric | BinaryFieldKind::Unused => {
                    // The value is borrowed from the drafts map (kept in sync
                    // with the register), so it lives as long as this Element.
                    let value: &str = self
                        .binary_field_drafts
                        .get(&name)
                        .map(String::as_str)
                        .unwrap_or("");
                    let submit_text = value.to_string();
                    let input_name = name.clone();
                    let submit_name = name.clone();
                    text_field("", value, move |text| {
                        Message::BinaryFieldInput(input_name.clone(), text)
                    })
                    .on_submit(Message::SetBinaryField(submit_name, submit_text))
                    .into()
                }
                BinaryFieldKind::Flags => {
                    let mut chips = row![].spacing(4);
                    for bit in f.flags {
                        chips = chips.push(
                            iced::widget::button(
                                column![
                                    text(bit.name).size(10).center(),
                                    text(if bit.set { "1" } else { "0" }).size(12).center(),
                                ]
                                .align_x(iced::Alignment::Center),
                            )
                            .padding([2, 6])
                            .on_press(Message::BitToggled(bit.bit as usize))
                            .style(width_chip_style(bit.set, true, palette)),
                        );
                    }
                    chips.into()
                }
                BinaryFieldKind::Reserved => text(format!("reserved · {}", f.value_text))
                    .size(12)
                    .color(palette.muted)
                    .into(),
            };
            cards = cards.push(
                container(column![header, editor].spacing(4))
                    .padding(8)
                    .style(move |_theme| container::background(palette.surface)),
            );
        }
        Some(scrollable(cards).into())
    }

    /// The visual format builder (Build new… / Edit current…): claim a run of
    /// the free bits, describe the pending field (name / kind / labels / base),
    /// Add it; the committed fields list with per-row remove, then Apply
    /// (transient) or Save (named). `None` unless the builder is open.
    pub(crate) fn format_builder_view(
        &self,
        register_width: u32,
        palette: &theme::Palette,
    ) -> Option<Element<'_, Message>> {
        let palette = *palette;
        let builder = self.session.format_builder()?;
        let free = builder.free_bits(register_width);

        // Claim buttons: 1..=free bits (capped for a sane row width).
        let mut claim = row![text("Claim").size(12).color(palette.muted)]
            .spacing(4)
            .align_y(iced::Alignment::Center);
        for bits in 1..=free.min(16) {
            let active = builder.pending_width() == bits;
            claim = claim.push(
                iced::widget::button(text(bits.to_string()).size(12).center())
                    .padding([2, 8])
                    .on_press(Message::BuilderClaim(bits))
                    .style(width_chip_style(active, true, palette)),
            );
        }

        // Draft inputs: name (unless a gap), kind picker, labels (flags/enum),
        // base (numeric), Add.
        let kinds: Vec<String> = FormatBuilderFieldKind::ALL
            .iter()
            .map(|k| k.raw_value().to_string())
            .collect();
        let kind_picker = select(
            kinds,
            Some(builder.draft_kind.raw_value().to_string()),
            |chosen: String| {
                let kind = FormatBuilderFieldKind::ALL
                    .into_iter()
                    .find(|k| k.raw_value() == chosen)
                    .unwrap_or(FormatBuilderFieldKind::Numeric);
                Message::BuilderDraftKind(kind)
            },
        );
        let mut draft = row![].spacing(6).align_y(iced::Alignment::Center);
        if !builder.is_gap_kind() {
            draft = draft.push(
                text_field("field name", &builder.draft_name, Message::BuilderDraftName)
                    .width(Length::Fixed(140.0)),
            );
        }
        draft = draft.push(kind_picker);
        if matches!(
            builder.draft_kind,
            FormatBuilderFieldKind::Flags | FormatBuilderFieldKind::Enumeration
        ) {
            draft = draft.push(
                text_field(
                    "labels, comma-separated",
                    &builder.draft_labels,
                    Message::BuilderDraftLabels,
                )
                .width(Length::Fixed(220.0)),
            );
        }
        if matches!(builder.draft_kind, FormatBuilderFieldKind::Numeric) {
            let dec = builder.draft_base == 10;
            draft = draft
                .push(
                    iced::widget::button(text("dec").size(11))
                        .padding([2, 8])
                        .on_press(Message::BuilderDraftBase(10))
                        .style(width_chip_style(dec, true, palette)),
                )
                .push(
                    iced::widget::button(text("hex").size(11))
                        .padding([2, 8])
                        .on_press(Message::BuilderDraftBase(16))
                        .style(width_chip_style(!dec, true, palette)),
                );
        }
        draft = draft.push(button::secondary("Add field", Message::BuilderAddField));

        // Committed fields, each with a remove button.
        let mut fields = column![].spacing(4);
        for f in builder.fields() {
            fields = fields.push(
                row![
                    text(format!(
                        "{} · {} bits · {}",
                        f.name,
                        f.width,
                        f.kind.raw_value()
                    ))
                    .size(12)
                    .color(palette.ink),
                    iced::widget::button(icons::icon(glyph::CLOSE).size(11))
                        .padding([1, 6])
                        .on_press(Message::BuilderRemoveField(f.id)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }

        let footer = row![
            text(format!("{free} free")).size(12).color(palette.muted),
            button::ghost("Apply", Message::ApplyBuiltFormat),
            text_field(
                "save as…",
                &self.builder_save_name,
                Message::BuilderSaveName
            )
            .width(Length::Fixed(120.0)),
            button::secondary("Save", Message::SaveFormat),
            button::ghost("Cancel", Message::CancelBuildFormat),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let panel = column![
            text("Build format").size(13).color(palette.ink),
            claim,
            draft,
            fields,
            footer,
        ]
        .spacing(8);
        Some(
            container(panel)
                .padding(10)
                .style(move |_theme| container::background(palette.surface))
                .into(),
        )
    }
}
