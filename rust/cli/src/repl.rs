//! The interactive REPL — rustyline in place of the Swift CLI's LineNoise:
//! ↑/↓ history (persisted to ~/.soroban_history, the same file the Swift
//! CLI uses), tab completion and gray signature hints — both fed by the
//! engine's own autocomplete/docs.

use crate::{evaluate, handle_mode_command};
use anzan::Calculator;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use std::borrow::Cow;
use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

struct AnzanHelper {
    calculator: Rc<RefCell<Calculator>>,
}

impl Completer for AnzanHelper {
    type Candidate = String;

    /// Tab: complete the identifier being typed, keep the rest of the line.
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        if pos != line.len() {
            return Ok((pos, Vec::new()));
        }
        let prefix = Calculator::trailing_identifier(line);
        if prefix.is_empty() {
            return Ok((pos, Vec::new()));
        }
        let start = line.len() - prefix.len();
        let candidates = self
            .calculator
            .borrow()
            .completions(&prefix)
            .into_iter()
            .map(|c| c.name)
            .collect();
        Ok((start, candidates))
    }
}

impl Hinter for AnzanHelper {
    type Hint = String;

    /// After `name(`, ghost the rest of the signature from the docs.
    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if pos != line.len() || !line.ends_with('(') {
            return None;
        }
        let name = Calculator::trailing_identifier(&line[..line.len() - 1]);
        if name.is_empty() {
            return None;
        }
        let doc = self.calculator.borrow().documentation_for(&name)?;
        let open = doc.signature.find('(')?;
        Some(doc.signature[open + 1..].to_string())
    }
}

impl Highlighter for AnzanHelper {
    /// The hint renders gray, like LineNoise's (127, 127, 127).
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("\x1b[90m{hint}\x1b[0m"))
    }
}

impl Validator for AnzanHelper {}
impl Helper for AnzanHelper {}

pub(crate) fn run(calculator: Calculator, version: &str) -> ExitCode {
    let calculator = Rc::new(RefCell::new(calculator));
    let Ok(mut editor) = Editor::<AnzanHelper, DefaultHistory>::new() else {
        eprintln!("error: can't open the terminal for interactive input");
        return ExitCode::FAILURE;
    };
    editor.set_helper(Some(AnzanHelper {
        calculator: Rc::clone(&calculator),
    }));

    let history_file = std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join(".soroban_history"));
    if let Some(path) = &history_file {
        let _ = editor.load_history(path);
    }

    println!(
        "Anzan・暗算 {version} — Soroban's exact calculation language. man name (or manual/help) for docs, tab completes, :mode switches dialect; exit to leave."
    );
    let prompt = "> ";
    loop {
        let line = match editor.readline(prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            // EOF (⌃D) or a non-recoverable terminal problem.
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }
        if trimmed == ":mode" || trimmed.starts_with(":mode ") {
            handle_mode_command(trimmed, &mut calculator.borrow_mut(), false);
            continue;
        }
        let _ = editor.add_history_entry(&line);
        if let Some(path) = &history_file {
            let _ = editor.save_history(path);
        }
        evaluate(
            &line,
            &mut calculator.borrow_mut(),
            true,
            false,
            prompt.len(),
        );
    }
    ExitCode::SUCCESS
}
