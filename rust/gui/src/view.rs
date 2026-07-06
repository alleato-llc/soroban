//! Top-level view assembly: the theme/font accessors, the input
//! subscription, the window title, the menu bar, and `view` itself.

use iced::widget::{column, container, row};
use iced::{event, keyboard, Element, Event, Font, Length, Subscription, Theme};
use rime::icons::glyph;
use rime::theme;
use rime::widgets::menu;
use rime::widgets::{button, context_menu, menu_bar_with_trailing, Menu, MenuItem};

use crate::{font_for, App, Message, ViewMode};
use crate::{shot, themes};

impl App {
    pub(crate) fn theme(&self) -> Theme {
        let name = self.theme_display_name();
        self.active_palette().iced_theme(name)
    }

    /// The active palette: the hand-edited one under `"Custom"`, else the named
    /// catalog palette (falling back to the default for an unknown name).
    pub(crate) fn active_palette(&self) -> theme::Palette {
        if self.theme_name == "Custom" {
            return self
                .custom_palette
                .unwrap_or_else(|| themes::palette(themes::default_name()));
        }
        themes::palette(&self.theme_name)
    }

    /// The theme's display name — the catalog name, or `"Custom"`, defaulting to
    /// the first catalog entry when unset (a fresh `App::default`).
    pub(crate) fn theme_display_name(&self) -> &str {
        if self.theme_name.is_empty() {
            themes::default_name()
        } else {
            &self.theme_name
        }
    }

    /// The effective base font size in points (a fresh `App::default` reads 0).
    pub(crate) fn base_font_size(&self) -> f32 {
        if self.font_size <= 0.0 {
            14.0
        } else {
            self.font_size
        }
    }

    /// The chosen monospace font for log/grid content (system default when unset).
    pub(crate) fn mono(&self) -> Font {
        font_for(&self.font_name)
    }

    /// Arrows navigate (history in the log, cells in the grid); Enter edits the
    /// selected cell, a bare character type-to-edits; ⌘\ toggles the view; ⌘Z /
    /// ⇧⌘Z undo & redo; ⌘C/⌘X/⌘V copy/cut/paste; Escape cancels an edit. The
    /// grid-only messages are gated in `update` (they no-op while editing / in
    /// the log), so a focused editor keeps its own keys.
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        use keyboard::key::Named;
        let keys = event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => match key {
                keyboard::Key::Named(Named::ArrowUp) => {
                    Some(Message::ArrowKey(-1, 0, modifiers.shift()))
                }
                keyboard::Key::Named(Named::ArrowDown) => {
                    Some(Message::ArrowKey(1, 0, modifiers.shift()))
                }
                keyboard::Key::Named(Named::ArrowLeft) => {
                    Some(Message::ArrowKey(0, -1, modifiers.shift()))
                }
                keyboard::Key::Named(Named::ArrowRight) => {
                    Some(Message::ArrowKey(0, 1, modifiers.shift()))
                }
                keyboard::Key::Named(Named::Enter) => Some(Message::GridEnter),
                keyboard::Key::Named(Named::Tab) => Some(Message::AcceptSuggestion),
                keyboard::Key::Named(Named::Escape) => Some(Message::EditCanceled),
                keyboard::Key::Character(character) if modifiers.command() => {
                    match character.as_str() {
                        "\\" => Some(Message::ToggleView),
                        "z" | "Z" if modifiers.shift() => Some(Message::Redo),
                        "z" | "Z" => Some(Message::Undo),
                        "n" | "N" => Some(Message::NewWorkbook),
                        "o" | "O" => Some(Message::OpenWorkbook),
                        "s" | "S" => Some(Message::SaveWorkbook),
                        "c" | "C" => Some(Message::Copy),
                        "x" | "X" => Some(Message::Cut),
                        "v" | "V" => Some(Message::Paste),
                        "," => Some(Message::OpenSettings),
                        // Font zoom (like the Swift app's View menu): ⌘= / ⌘+
                        // grow, ⌘- / ⌘_ shrink, ⌘0 resets — so the size is
                        // adjustable without opening Settings.
                        "=" | "+" => Some(Message::ZoomFont(1.0)),
                        "-" | "_" => Some(Message::ZoomFont(-1.0)),
                        "0" => Some(Message::ResetFontSize),
                        _ => None,
                    }
                }
                // A bare character (no ⌘/⌃/⌥) → type-to-edit in the grid.
                keyboard::Key::Character(character)
                    if !modifiers.command() && !modifiers.control() && !modifiers.alt() =>
                {
                    Some(Message::GridType(character.to_string()))
                }
                _ => None,
            },
            // Cursor position drives the auto-hiding menu bar (window-relative Y)
            // and anchors the right-click cell context menu (X, Y).
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::PointerMoved(position.x, position.y))
            }
            _ => None,
        });
        match shot::subscription(self) {
            Some(shot) => Subscription::batch([keys, shot]),
            None => keys,
        }
    }

    /// The window title carries the document name and unsaved-changes dot, like
    /// the AppKit original ("Soroban・算盤 — Untitled") — no in-window wordmark.
    pub(crate) fn window_title(&self) -> String {
        let name = self
            .file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        format!(
            "Soroban・算盤 — {name}{}",
            if self.is_dirty() { " •" } else { "" }
        )
    }

    /// The menu bar's File / Edit / View menus — the honest in-window stand-in
    /// for the macOS menu bar the AppKit app uses (iced has no system menu bar).
    /// Labels track state (Show Grid ↔ Show Log, Light ↔ Dark Theme).
    pub(crate) fn menus(&self) -> Vec<Menu<Message>> {
        let view_label = match self.mode {
            ViewMode::Log => "Show Grid",
            ViewMode::Grid => "Show Log",
        };
        vec![
            Menu::new(
                "File",
                vec![
                    MenuItem::shortcut("New", "⌘N", Message::NewWorkbook),
                    MenuItem::shortcut("Open…", "⌘O", Message::OpenWorkbook),
                    MenuItem::shortcut("Save", "⌘S", Message::SaveWorkbook),
                    MenuItem::separator(),
                    MenuItem::shortcut("Open CSV…", "", Message::OpenCsv),
                    MenuItem::separator(),
                    MenuItem::shortcut("Settings…", "⌘,", Message::OpenSettings),
                ],
            ),
            Menu::new(
                "Edit",
                vec![
                    MenuItem::shortcut("Undo", "⌘Z", Message::Undo),
                    MenuItem::shortcut("Redo", "⇧⌘Z", Message::Redo),
                    MenuItem::separator(),
                    MenuItem::shortcut("Copy", "⌘C", Message::Copy),
                    MenuItem::shortcut("Cut", "⌘X", Message::Cut),
                    MenuItem::shortcut("Paste", "⌘V", Message::Paste),
                ],
            ),
            Menu::new(
                "Sheet",
                vec![
                    MenuItem::action("Add Sheet", Message::AddSheet),
                    MenuItem::action(
                        "Rename Sheet…",
                        Message::BeginRenameSheet(self.session.active_sheet_index()),
                    ),
                    MenuItem::action("Delete Sheet", Message::DeleteSheet),
                    MenuItem::separator(),
                    MenuItem::action("Open CSV…", Message::OpenCsv),
                ],
            ),
            Menu::new(
                "View",
                vec![
                    MenuItem::shortcut(view_label, "⌘\\", Message::ToggleView),
                    MenuItem::separator(),
                    MenuItem::action("Names", Message::ToggleInspector),
                    MenuItem::action("Reference", Message::ToggleReference),
                    MenuItem::action("Bits", Message::ToggleBinary),
                ],
            ),
        ]
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        let _scope = theme::enter(self.active_palette());
        let palette = theme::tokens();

        let body = match self.mode {
            ViewMode::Log => self.log_view(&palette),
            ViewMode::Grid => self.grid_view(&palette),
        };

        // Edge-to-edge, no card — the view fills the window; the log's own input
        // bar sits at the bottom (REPL layout).
        let main = container(body)
            .padding([10, 16])
            .width(Length::Fill)
            .height(Length::Fill);

        // The main area plus any right-side sidebars (inspector / reference).
        let horizontal: Element<'_, Message> = if self.inspector_visible || self.reference_visible {
            let mut panels = row![main.width(Length::Fill)].height(Length::Fill);
            if self.inspector_visible {
                panels = panels.push(self.inspector_panel(&palette));
            }
            if self.reference_visible {
                panels = panels.push(self.reference_panel(&palette));
            }
            panels.into()
        } else {
            main.into()
        };

        // The binary bit-editor rides underneath as a full-width strip.
        let content: Element<'_, Message> = if self.binary_visible {
            column![
                container(horizontal).height(Length::Fill),
                self.binary_panel(&palette)
            ]
            .into()
        } else {
            horizontal
        };

        // The menu bar. In GRID mode it's a PERSISTENT toolbar (rime's intended
        // layout): always shown, with the content reserved below it so it never
        // covers the grid's top row (the address/formula bar and column headers).
        // In LOG mode it AUTO-HIDES and overlays the top edge — the REPL layout,
        // where content fills the window and nothing jumps as the bar appears.
        //
        // CRUCIAL: in log mode `content` stays at a FIXED tree position (stack
        // layer 0) whether or not the bar shows — the bar is a second layer that's
        // either the real menu or a zero-size placeholder. Re-parenting `content`
        // *on hover* (wrapping it only when revealed) reset the focused text
        // field's widget state, so the log input lost focus the instant the
        // pointer neared the top edge. The grid-mode wrapper below is keyed on the
        // MODE (stable across hovers), so it doesn't trip that.
        let grid_mode = self.mode == ViewMode::Grid;
        let show_bar = grid_mode || self.menu_revealed || self.menu_open.is_some();
        let bar_layer: Element<'_, Message> = if show_bar {
            let inspector_icon = button::icon(glyph::NAMES, Message::ToggleInspector);
            menu_bar_with_trailing(
                self.menus(),
                self.menu_open,
                Message::ToggleMenu,
                Some(inspector_icon.into()),
            )
        } else {
            iced::widget::Space::new()
                .width(Length::Fixed(0.0))
                .height(Length::Fixed(0.0))
                .into()
        };
        // Grid mode reserves the bar's height at the top so the persistent
        // toolbar sits above the content, not over it.
        let stacked: Element<'_, Message> = if grid_mode {
            column![
                iced::widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fixed(menu::BAR_HEIGHT)),
                content,
            ]
            .into()
        } else {
            content
        };
        let base: Element<'_, Message> = iced::widget::stack![stacked, bar_layer].into();

        // The right-click cell context menu overlays the grid at the cursor.
        let base: Element<'_, Message> = if let Some(at) = self.cell_menu {
            let items = self.cell_menu_items();
            context_menu(base, &items, at, Message::CloseCellMenu)
        } else {
            base
        };

        // The Settings window, when open, frames itself over everything.
        if self.settings_open {
            self.settings_view(base, &palette)
        } else {
            base
        }
    }
}
