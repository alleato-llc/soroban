//! Soroban — the Rust/iced desktop app (docs/MIGRATION.md Phase 3b).
//!
//! Slice ①: a working log-view calculator. Type an expression, press Enter,
//! and the Anzan engine evaluates it into the log; ↑/↓ recall the input
//! history. The engine and history live in [`session::Session`]; this file is
//! the iced shell (state → message → update → view) and the rime-styled
//! rendering. Later slices add the grid, controls, the binary editor, and
//! workbook save/open.

mod session;

use iced::widget::{column, container, row, scrollable, text};
use iced::{event, keyboard, Element, Event, Font, Length, Subscription, Theme};
use rime::theme::{self, ThemeChoice};
use rime::widgets::{button, card, header_row, section, text_field};
use session::{Outcome, Session};

const MONO: Font = Font::MONOSPACE;

#[derive(Default)]
struct App {
    session: Session,
    choice: ThemeChoice,
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Submit,
    HistoryPrevious,
    HistoryNext,
    ToggleTheme,
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::InputChanged(text) => self.session.set_input(text),
            Message::Submit => self.session.submit(),
            Message::HistoryPrevious => self.session.recall_previous(),
            Message::HistoryNext => self.session.recall_next(),
            Message::ToggleTheme => self.choice = self.choice.toggled(),
        }
    }

    fn theme(&self) -> Theme {
        self.choice.theme()
    }

    /// ↑/↓ drive input-history recall. A single-line input ignores them, so
    /// capturing them globally is safe (the input is the only field).
    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                ..
            }) => Some(Message::HistoryPrevious),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                ..
            }) => Some(Message::HistoryNext),
            _ => None,
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let _scope = theme::enter(self.choice.palette());
        let palette = theme::tokens();

        let theme_label = if matches!(self.choice, ThemeChoice::Dark) {
            "☀ Light"
        } else {
            "☾ Dark"
        };

        let input_bar = row![
            text_field(
                "Type an expression — try 0.1 + 0.2",
                self.session.input(),
                Message::InputChanged
            )
            .on_submit(Message::Submit)
            .font(MONO),
            button::primary("=", Message::Submit),
            button::ghost(theme_label, Message::ToggleTheme),
        ]
        .spacing(8);

        // The log, newest first so the latest result sits right under the input.
        let log: Element<'_, Message> = if self.session.entries().is_empty() {
            container(
                text("Results appear here. ↑/↓ recall what you typed.")
                    .size(13)
                    .color(palette.muted),
            )
            .padding(12)
            .into()
        } else {
            let mut items = column![].spacing(12);
            for entry in self.session.entries().iter().rev() {
                items = items.push(entry_view(&entry.input, &entry.outcome, &palette));
            }
            scrollable(items.padding([4, 8]))
                .height(Length::Fill)
                .into()
        };

        let body = card(
            column![
                header_row(
                    "Soroban",
                    "Anzan — exact calculation (50 significant digits)"
                ),
                input_bar,
                section("Log"),
                log,
            ]
            .spacing(16),
        );

        container(body)
            .padding(20)
            .center_x(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// One log entry: the echoed input, then its outcome (a value, a definition, a
/// note, a raw block, or an error with an aligned caret).
fn entry_view<'a>(
    input: &str,
    outcome: &Outcome,
    palette: &theme::Palette,
) -> Element<'a, Message> {
    // Echoed input, monospace so an error caret lines up beneath it.
    let echo = text(format!("› {input}"))
        .font(MONO)
        .size(13)
        .color(palette.muted);

    let result: Element<'a, Message> = match outcome {
        Outcome::Value(value) => text(format!("= {value}"))
            .font(MONO)
            .size(14)
            .color(palette.accent)
            .into(),
        Outcome::Function(signature) => text(format!("λ {signature}"))
            .font(MONO)
            .size(13)
            .color(palette.ink)
            .into(),
        Outcome::Data(declaration) => text(format!("𝑫 {declaration}"))
            .font(MONO)
            .size(13)
            .color(palette.ink)
            .into(),
        Outcome::Comment(note) => text(format!("# {note}"))
            .font(MONO)
            .size(13)
            .color(palette.muted)
            .into(),
        Outcome::Info(block) => text(block.clone())
            .font(MONO)
            .size(13)
            .color(palette.ink)
            .into(),
        Outcome::Error { message, position } => {
            let mut lines = column![].spacing(2);
            if let Some(position) = position {
                // The echo prefix "› " is two columns wide; offset the caret.
                let caret = format!("{}^", " ".repeat(2 + position));
                lines = lines.push(text(caret).font(MONO).size(13).color(palette.danger));
            }
            lines
                .push(
                    text(format!("error: {message}"))
                        .size(13)
                        .color(palette.danger),
                )
                .into()
        }
    };

    column![echo, result].spacing(2).into()
}

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Soroban")
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size(iced::Size::new(720.0, 560.0))
        .run()
}
