//! The Settings window's sections (appearance, live preview, calculator).

use iced::widget::{column, scrollable, text};
use iced::{Element, Length};
use rime::theme;
use rime::widgets::{caption, card, color_field, select, settings, slider};
use soroban_engine::LanguageMode;

use crate::themes;
use crate::{font_choices, title_case, App, Message};

impl App {
    /// The Settings window: rime's `settings` shell (a dimmed backdrop, a section
    /// rail, and the active section's body). Appearance = theme + custom colors +
    /// font size + a live preview; Calculator = the language mode.
    pub(crate) fn settings_view<'a>(
        &'a self,
        base: Element<'a, Message>,
        palette: &theme::Palette,
    ) -> Element<'a, Message> {
        let content = match self.settings_section {
            1 => self.settings_calculator(palette),
            _ => self.settings_appearance(palette),
        };
        settings(
            base,
            &["Appearance", "Calculator"],
            self.settings_section,
            Message::SelectSettingsSection,
            content,
            None,
            Message::CloseSettings,
        )
    }

    /// The Appearance section: a theme picker (the ten named palettes plus a
    /// hand-editable "Custom"), the custom color rows when it's active, a font-
    /// size slider, and a live preview swatch.
    pub(crate) fn settings_appearance(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let mut names = themes::names();
        names.push("Custom".to_string());
        let current = self.theme_display_name().to_string();
        let theme_picker = select(names, Some(current.clone()), Message::SelectTheme);

        let mut body = column![caption("Theme"), theme_picker,].spacing(10);

        // The custom color editor — one color_field per token — shows only for
        // the "Custom" theme, seeded from the live palette.
        if current == "Custom" {
            let editable = self.active_palette();
            let mut rows = column![].spacing(8);
            for &key in theme::PALETTE_KEYS {
                if let Some(color) = editable.color(key) {
                    let owned_key = key.to_string();
                    rows = rows.push(color_field(key, color, move |c| {
                        Message::SetCustomColor(owned_key.clone(), c)
                    }));
                }
            }
            body = body.push(rows);
        }

        // Font family (the bundled monospace choices).
        let font_names: Vec<String> = font_choices().iter().map(|(n, _)| n.to_string()).collect();
        let current_font = if self.font_name.is_empty() {
            font_choices()[0].0.to_string()
        } else {
            self.font_name.clone()
        };
        body = body.push(caption("Font"));
        body = body.push(select(font_names, Some(current_font), Message::SelectFont));

        let size = self.base_font_size();
        body = body.push(caption("Font size"));
        body = body.push(slider(
            "",
            9.0..=28.0,
            size,
            format!("{} pt", size.round() as i32),
            Message::SetFontSize,
        ));

        body = body.push(caption("Preview"));
        body = body.push(self.settings_preview(palette));
        scrollable(body).height(Length::Fill).into()
    }

    /// A small live preview of the log: an expression echo, a result, an error,
    /// and a muted note — all in the pending palette and font size, so theme and
    /// size changes are visible without leaving Settings.
    pub(crate) fn settings_preview(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let font = self.mono();
        let size = self.base_font_size();
        let sample = column![
            text("1024 / 8").font(font).size(size).color(palette.accent),
            text("= 128").font(font).size(size).color(palette.ink),
            text("sqrt(-1)").font(font).size(size).color(palette.accent),
            text("domain error")
                .font(font)
                .size(size)
                .color(palette.danger),
            text("# a note").font(font).size(size).color(palette.muted),
        ]
        .spacing(6);
        card(sample)
    }

    /// The Calculator section: the language mode (input/display dialect).
    pub(crate) fn settings_calculator(&self, palette: &theme::Palette) -> Element<'_, Message> {
        let modes = [
            LanguageMode::Normal,
            LanguageMode::Programmer,
            LanguageMode::Finance,
        ];
        let labels: Vec<String> = modes.iter().map(|m| title_case(m.name())).collect();
        let current = title_case(self.session.language_mode().name());
        let picker = select(labels, Some(current), move |chosen: String| {
            let mode = modes
                .iter()
                .copied()
                .find(|m| title_case(m.name()) == chosen)
                .unwrap_or(LanguageMode::Normal);
            Message::SelectMode(mode)
        });
        column![
            caption("Mode"),
            picker,
            text(
                "Normal is the everyday dialect. Programmer reads ^ & | << >> as \
                 bitwise operators; Finance tunes the display for money."
            )
            .size(12)
            .color(palette.muted),
        ]
        .spacing(10)
        .into()
    }
}
