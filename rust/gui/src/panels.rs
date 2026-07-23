//! Side/bottom panels and the two main bodies: reference, inspector,
//! the log view, and the grid view.

use iced::widget::{column, container, mouse_area, row, scrollable, text};
use iced::{Element, Length};
use rime::icons::glyph;
use rime::theme;
use rime::widgets::{button, card, grid, rename_bar, section, text_field};
use soroban_engine::{CellAddress, LanguageMode};

use crate::render::*;
use crate::{edit_bar_id, grid_editor_id, log_input_id, log_scroll_id, App, Message};

impl App {
    /// The reference window: every function, operator, and constant — the
    /// user's own first — with a live search filter.
    /// A sidebar panel's title row: the title on the left, a × on the right that
    /// closes the panel (fires `close`).
    pub(crate) fn panel_header<'a>(
        title: &'a str,
        close: Message,
        palette: &theme::Palette,
    ) -> Element<'a, Message> {
        row![
            text(title).size(15).color(palette.ink),
            container(button::icon(glyph::CLOSE, close))
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    }

    pub(crate) fn reference_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        let search = text_field(
            "Search the reference…",
            &self.reference_query,
            Message::ReferenceQueryChanged,
        );
        let mut list = column![].spacing(14);
        let groups = self.session.reference(&self.reference_query);
        if groups.is_empty() {
            list = list.push(text("No matches.").size(12).color(palette.muted));
        }
        for group in groups {
            let mut group_column = column![section(&group.title)].spacing(8);
            for entry in group.entries {
                group_column = group_column.push(
                    column![
                        text(entry.signature)
                            .font(font)
                            .size(12)
                            .color(palette.accent),
                        text(entry.summary).size(11).color(palette.muted),
                    ]
                    .spacing(2),
                );
            }
            list = list.push(group_column);
        }

        container(card(
            column![
                Self::panel_header("Reference", Message::ToggleReference, palette),
                search,
                scrollable(list).height(Length::Fill),
            ]
            .spacing(12),
        ))
        .width(Length::Fixed(320.0))
        .padding(20)
        .height(Length::Fill)
        .into()
    }

    /// The names inspector: every live variable (log vars, named cells, sheet 𝑖
    /// definitions), function, and data type — grouped into three sections like
    /// the original, each row tagged with its provenance (`log` or a clickable
    /// `B:2 ↗` that jumps to the cell).
    pub(crate) fn inspector_panel(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        let mut sections = column![Self::panel_header(
            "Environment",
            Message::ToggleInspector,
            palette
        )]
        .spacing(16);
        let groups = [
            ("VARIABLES", self.session.inspector_variables()),
            ("FUNCTIONS", self.session.inspector_functions()),
            ("DATA TYPES", self.session.inspector_data_types()),
        ];
        let mut any = false;
        for (title, rows) in groups {
            if rows.is_empty() {
                continue;
            }
            any = true;
            // A small-caps muted section heading, like the original.
            let mut group = column![text(title).size(11).color(palette.muted)].spacing(8);
            for row in rows {
                let mut line = column![row![
                    text(row.label).font(font).size(12).color(palette.accent),
                    container(origin_tag(row.origin, palette, font))
                        .width(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                ]
                .align_y(iced::Alignment::Center)]
                .spacing(1);
                if !row.detail.is_empty() {
                    line = line.push(text(row.detail).font(font).size(11).color(palette.muted));
                }
                group = group.push(line);
            }
            sections = sections.push(group);
        }
        if !any {
            sections = sections.push(
                text("Nothing defined yet — assign a variable or name a cell.")
                    .size(12)
                    .color(palette.muted),
            );
        }

        container(card(scrollable(sections).height(Length::Fill)))
            .width(Length::Fixed(260.0))
            .padding(20)
            .height(Length::Fill)
            .into()
    }

    pub(crate) fn log_view(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        // The log fills, oldest→newest, so the freshest result sits just above
        // the input — the terminal/REPL layout of the AppKit original.
        let size = self.base_font_size();
        let entries = self.session.entries(); // Ref over the shared log tape
                                              // Keep this slot a `container` in BOTH states (empty vs. populated) so the
                                              // input below it doesn't re-parent — and lose focus — on the first submit.
        let log_inner: Element<'_, Message> = if entries.is_empty() {
            self.empty_log(palette)
        } else {
            let mut items = column![].spacing(12);
            for entry in entries.iter() {
                items = items.push(entry_view(
                    &entry.input,
                    &entry.outcome,
                    palette,
                    size,
                    font,
                ));
            }
            scrollable(items.width(Length::Fill).padding([4, 8]))
                .id(log_scroll_id())
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        let log = container(log_inner)
            .width(Length::Fill)
            .height(Length::Fill);

        // The input is pinned to the BOTTOM, behind a `>` prompt; Enter submits
        // (no `=` button — the original has none). A mode affordance (cycles
        // Normal → Programmer → Scientific) sits at the left of the corner
        // icons (docs / grid), like the AppKit app's input-bar mode control.
        let mode_label = match self.session.language_mode() {
            LanguageMode::Normal => "Normal",
            LanguageMode::Programmer => "Programmer",
            LanguageMode::Scientific => "Scientific",
        };
        let input_bar = row![
            text(">").font(font).size(size + 2.0).color(palette.muted),
            text_field("Expression", self.session.input(), Message::InputChanged)
                .id(log_input_id())
                .on_submit(Message::Submit)
                .size(size)
                .font(font),
            button::ghost(mode_label, Message::CycleMode),
            button::icon(glyph::REFERENCE, Message::ToggleReference),
            button::icon(glyph::GRID, Message::ToggleView),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // The popup sits just ABOVE the bottom-anchored input (a REPL wants its
        // completions rising from the prompt, not dropping off-screen). The
        // input bar stays at a FIXED position in the column — a zero-height
        // placeholder stands in when there's no popup — so the text field never
        // shifts index and thus never loses focus mid-type (an iced tree-diff
        // quirk: a widget that changes tree position is rebuilt from scratch).
        let popup: Element<'_, Message> = self.suggestion_popup().unwrap_or_else(|| {
            // A zero-height CONTAINER (not a Space) so this slot is always the same
            // widget type as the real popup — the input below it never re-parents.
            container(iced::widget::Space::new().height(Length::Fixed(0.0))).into()
        });
        let bottom = column![popup, input_bar].spacing(4);
        column![log, bottom].spacing(12).into()
    }

    /// The empty-state: an invitation plus a few sample expressions that insert
    /// themselves into the input on click (the original's "double-click one").
    pub(crate) fn empty_log(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        const SAMPLES: [&str; 3] = [
            "map(n -> n * n, filter(x -> x % 2 == 0, seq(1, 20)))",
            "fact(52) / (fact(5) * fact(47))",
            "0.1 + 0.2",
        ];
        let mut column = column![text("Type an expression below — or click one:")
            .size(14)
            .color(palette.muted)]
        .spacing(10);
        for sample in SAMPLES {
            column = column.push(
                mouse_area(text(sample).font(font).size(14).color(palette.accent))
                    .on_press(Message::SampleClicked(sample.to_string()))
                    .interaction(iced::mouse::Interaction::Pointer),
            );
        }
        container(column).padding(12).height(Length::Fill).into()
    }

    pub(crate) fn grid_view(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        // The formula/edit bar: the active cell's address, then its raw content.
        // Click it (or just start typing) to edit; Enter commits, Esc cancels.
        let address_label = self
            .active_cell()
            .map(|address| address.to_string())
            .unwrap_or_else(|| "—".to_string());
        let edit_bar = row![
            container(text(address_label).font(font).size(13).color(palette.muted))
                .width(Length::Fixed(48.0))
                .center_y(Length::Shrink),
            // The name box (Excel-style): name the selected cell's location.
            container(
                text_field("name", &self.name_draft, Message::NameChanged)
                    .on_submit(Message::NameCommitted)
            )
            .width(Length::Fixed(150.0)),
            text_field(
                "Type a value or formula — click a cell to insert its reference",
                &self.edit_draft,
                Message::EditChanged
            )
            .id(edit_bar_id())
            .on_submit(Message::EditSubmitted)
            .font(font),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Controls now render inline in their cells (below); the header keeps the
        // formula/name bar and the autocomplete popup (dropping below the top-
        // anchored bar). Formatting moved to the right-click cell context menu
        // (see `cell_menu_items`) — the AppKit per-cell menu, not a fixed row.
        let mut header = column![edit_bar].spacing(12);
        if let Some(popup) = self.suggestion_popup() {
            header = header.push(popup);
        }

        let palette = *palette;
        let session = &self.session;
        // A data sheet is bounded by its table (≤10k rows); a grid sheet fills
        // the full 1000×26.
        let rows = session.visible_row_count();
        let cols = session.visible_column_count();
        let mut sheet = grid(rows, cols, move |row, col| {
            let address = CellAddress::new(col, row);
            render_cell(
                session.display_at(address),
                &session.cell_format(address),
                &palette,
            )
        })
        .offset(self.grid_offset)
        .selection(self.grid_selection)
        .column_widths(self.session.column_widths())
        .on_scroll(Message::GridScrolled)
        .on_select(Message::GridSelected)
        .on_activate(Message::EditActivated)
        .on_resize_column(Message::ColumnResized);

        // Host each control (slider / stepper / checkbox / dropdown) as an
        // interactive widget inside its own cell — the AppKit behavior — except
        // the cell currently being edited (the editor takes that one).
        let editing_cell = self.editing.then(|| self.active_cell()).flatten();
        for (address, display) in self.session.control_cells() {
            if Some(address) == editing_cell {
                continue;
            }
            if let Some(widget) = control_widget(address, display) {
                sheet = sheet.overlay(address.row, address.column, widget);
            }
        }

        // While editing, host an inline text editor over the active cell — the
        // cell edits in place (the AppKit behavior), mirroring the formula bar.
        if self.editing {
            if let Some((row, col)) = self.grid_selection.map(|s| s.anchor) {
                let editor = iced::widget::text_input("", &self.edit_draft)
                    .id(grid_editor_id())
                    .on_input(Message::EditChanged)
                    .on_submit(Message::EditSubmitted)
                    .padding(2)
                    .size(13)
                    .font(font);
                sheet = sheet.editor(row, col, editor);
            }
        }

        // A sheet-tab strip at the bottom-left, like the original's `Mortgage +`,
        // with a log/grid view-toggle icon pinned bottom-right (the AppKit app's
        // corner affordance). One tab per sheet: click switches, double-click
        // renames (inline bar), and the trailing "+" appends a new sheet.
        let active_index = self.session.active_sheet_index();
        let mut tabs_row = row![].spacing(2).align_y(iced::Alignment::Center);
        for (i, name) in self.session.sheet_names().into_iter().enumerate() {
            let color = if i == active_index {
                palette.accent
            } else {
                palette.muted
            };
            let tab = container(text(name).font(font).size(13).color(color)).padding([4, 10]);
            tabs_row = tabs_row.push(
                mouse_area(tab)
                    .on_press(Message::ActivateSheet(i))
                    .on_double_click(Message::BeginRenameSheet(i)),
            );
        }
        tabs_row = tabs_row.push(button::ghost("+", Message::AddSheet));

        let sheet_tab = row![
            container(tabs_row).style(move |_| container::background(palette.surface)),
            container(text("").size(1)).width(Length::Fill),
            button::icon(glyph::LOG, Message::ToggleView),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // While renaming, an inline field rides above the strip (Enter commits,
        // Escape cancels — see `EditCanceled`).
        let mut bottom = column![].spacing(4);
        if let Some(draft) = &self.sheet_rename_draft {
            bottom = bottom.push(rename_bar(
                "Rename sheet",
                "Sheet name",
                draft,
                Message::SheetRenameChanged,
                Message::SheetRenameCommitted,
            ));
        }
        bottom = bottom.push(sheet_tab);

        // A right-click anywhere on the grid opens the cell context menu (the
        // grid itself only consumes the LEFT button, so the secondary press
        // bubbles to this wrapper).
        let grid_area =
            mouse_area(container(sheet).height(Length::Fill)).on_right_press(Message::OpenCellMenu);

        column![header, grid_area, bottom].spacing(12).into()
    }
}
