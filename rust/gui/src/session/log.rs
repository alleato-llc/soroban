//! The calculation log: input handling, autocomplete, language mode, submit /
//! evaluate, and the ↑/↓ recall tape.

use super::*;

impl Session {
    // MARK: Log

    pub fn entries(&self) -> std::cell::Ref<'_, Vec<LogEntry>> {
        self.entries.borrow()
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    /// Live typing — breaks any in-progress history recall.
    pub fn set_input(&mut self, text: String) {
        self.input = text;
        self.history_cursor = None;
    }

    /// Autocomplete candidates for the trailing identifier of `draft` — the
    /// engine's completion pass over the live environment (user variables and
    /// functions, constants, every built-in). Empty when the trailing word is
    /// blank or already a unique full match, so an empty result closes the
    /// popup for free.
    pub fn suggestions(&self, draft: &str) -> Vec<Completion> {
        let prefix = Calculator::trailing_identifier(draft);
        if prefix.is_empty() {
            return Vec::new();
        }
        self.calculator.borrow().completions(&prefix)
    }

    /// Splice `completion` over the trailing identifier of `draft`, returning
    /// the new text (cursor implicitly at the end). A function or type
    /// constructor also gets its opening `(` — you complete `fac` to `fact(`,
    /// ready for arguments, matching the CLI and the AppKit original.
    pub fn apply_completion(draft: &str, completion: &Completion) -> String {
        let trailing = Calculator::trailing_identifier(draft);
        // `trailing` is a suffix of `draft`, so the byte split is exact.
        let head = &draft[..draft.len() - trailing.len()];
        let mut out = String::from(head);
        out.push_str(&completion.name);
        if completion.kind == CompletionKind::Function {
            out.push('(');
        }
        out
    }

    /// Evaluate the current line, append it to the log, and record it in the
    /// history tape. A blank line is ignored.
    /// The active calculator dialect (drives how the LOG parses/renders; cells
    /// are always Normal). Programmer reads `^ & | << >> ~ %` as bitwise/modulo.
    pub fn language_mode(&self) -> LanguageMode {
        self.calculator.borrow().mode
    }

    /// Switch the log's dialect. Canonical storage is unchanged — only which
    /// glyphs you type and read differ.
    pub fn set_language_mode(&mut self, mode: LanguageMode) {
        self.calculator.borrow_mut().mode = mode;
        self.revision += 1;
    }

    /// The dialect + style, for the `:mode` status line — "scientific eng"
    /// when the ENG variant is on, otherwise just the mode name.
    fn mode_text(&self) -> String {
        let calculator = self.calculator.borrow();
        if calculator.mode == LanguageMode::Scientific
            && calculator.sci_style == ScientificStyle::Eng
        {
            "scientific eng".to_string()
        } else {
            calculator.mode.name().to_string()
        }
    }

    /// Intercept the host-level `:mode [name]` command (like the CLI), through
    /// the engine's one shared parse seam (`set_mode_parsing` — the same
    /// errors as the CLI's). Returns the log outcome to record, or `None` if
    /// the line isn't a mode command.
    fn mode_command(&mut self, line: &str) -> Option<Outcome> {
        let rest = line.strip_prefix(":mode")?;
        let arg = rest.trim();
        if arg.is_empty() {
            return Some(Outcome::Info(format!("mode: {}", self.mode_text())));
        }
        let result = self.calculator.borrow_mut().set_mode_parsing(arg);
        match result {
            Ok(()) => {
                self.revision += 1;
                Some(Outcome::Info(format!("mode: {}", self.mode_text())))
            }
            Err(error) => Some(Outcome::Error {
                message: error.to_string(),
                position: None,
            }),
        }
    }

    pub fn submit(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }
        let outcome = self
            .mode_command(&line)
            .unwrap_or_else(|| self.evaluate(&line));
        self.entries.borrow_mut().push(LogEntry {
            input: line.clone(),
            outcome,
        });
        // Don't stack consecutive duplicates in the recall tape.
        if self.history.last() != Some(&line) {
            self.history.push(line);
        }
        self.input.clear();
        self.history_cursor = None;
        // A log line may define a variable/function/type — mark the doc dirty.
        self.revision += 1;
        // The tape + recall history survive a relaunch (mirrors LogStore).
        self.save_persisted();
    }

    fn evaluate(&self, line: &str) -> Outcome {
        let result = self.calculator.borrow_mut().evaluate(line);
        match result {
            Ok(outcome) => {
                // Multi-line results (pretty JSON, man pages) render raw.
                if let Some(block) = outcome.raw_block() {
                    return Outcome::Info(block.to_string());
                }
                match &outcome {
                    // Mode-aware echo (the engine's one display seam):
                    // scientific mode shows a plain numeric result as
                    // 2.46912e5 (or eng); recall still gives the canonical
                    // form.
                    EvalOutcome::Value(_) => {
                        let calculator = self.calculator.borrow();
                        Outcome::Value(
                            outcome.display_description_in(calculator.mode, calculator.sci_style),
                        )
                    }
                    EvalOutcome::FunctionDefined { signature } => {
                        Outcome::Function(signature.clone())
                    }
                    EvalOutcome::DataDefined { declaration } => Outcome::Data(declaration.clone()),
                    EvalOutcome::Comment(text) => Outcome::Comment(text.clone()),
                    EvalOutcome::Documentation(_) => Outcome::Info(format!("{outcome}")),
                }
            }
            Err(error) => Outcome::Error {
                message: error.to_string(),
                position: error.position(),
            },
        }
    }

    /// ↑ — recall an older line (or the newest, on first press).
    pub fn recall_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = match self.history_cursor {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(current) => current - 1,
        };
        self.history_cursor = Some(index);
        self.input = self.history[index].clone();
    }

    /// ↓ — walk back toward the newest line, then to an empty field.
    pub fn recall_next(&mut self) {
        match self.history_cursor {
            Some(current) if current + 1 < self.history.len() => {
                self.history_cursor = Some(current + 1);
                self.input = self.history[current + 1].clone();
            }
            Some(_) => {
                // Past the newest recalled line — return to an empty field.
                self.history_cursor = None;
                self.input.clear();
            }
            None => {}
        }
    }
}
