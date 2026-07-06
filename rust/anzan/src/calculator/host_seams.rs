//! Pure host seams: comment splitting, point-mode operand detection,
//! programmer-notation sniffing, SpeedCrunch ans-prefixing, and the
//! autocomplete word/candidate helpers.

use super::{Calculator, Completion, CompletionKind};
use crate::eval::registry::FunctionRegistry;
use crate::lexer::Lexer;
use crate::LanguageMode;

impl Calculator {
    /// The comment text of a line that is ONLY a comment (`# note`), or
    /// `None` when the line has code. Used by hosts and the calculator to
    /// treat a comment-only line/cell as a first-class note instead of a
    /// parse error.
    pub fn standalone_comment(line: &str) -> Option<String> {
        let (code, comment) = Lexer::split_comment(line);
        if code.trim().is_empty() {
            comment
        } else {
            None
        }
    }

    /// The trailing comment on a line that ALSO has code (`5 + 3 # adds`),
    /// or `None`. Hosts show it dimmed beside the result and keep it on the
    /// raw.
    pub fn trailing_comment(line: &str) -> Option<String> {
        let (code, comment) = Lexer::split_comment(line);
        if code.trim().is_empty() {
            None
        } else {
            comment
        }
    }

    /// True when a formula draft ends "expecting an operand" — after an
    /// operator, open paren, comma, `=`, comparison, or range dots. This is
    /// Excel's point mode test: clicking a cell while it holds should insert
    /// the cell's reference rather than commit the edit.
    pub fn expects_operand(draft: &str) -> bool {
        let Some(last) = draft.trim().chars().last() else {
            return false;
        };
        // $ starts a pinned ref.
        "+-*/%^(,=<>≤≥≠·×÷−√.[{:$".contains(last)
    }

    /// Did this input line speak programmer? (0x/0b literals at a token
    /// boundary, or the base/bit functions.) Hosts use it to decide when an
    /// integer result deserves a hex echo — display only, never semantics.
    pub fn uses_programmer_notation(line: &str) -> bool {
        let lowered = line.to_lowercase();
        for name in [
            "bitand", "bitor", "bitxor", "bitshift", "frombase", "tobase",
        ] {
            if lowered.contains(name) {
                return true;
            }
        }
        let chars: Vec<char> = lowered.chars().collect();
        for i in 0..chars.len().saturating_sub(1) {
            if chars[i] == '0' && (chars[i + 1] == 'x' || chars[i + 1] == 'b') {
                // Token boundary: "10x" is implicit multiplication, "a0b" an
                // identifier — only a bare 0x/0b counts.
                if i == 0
                    || !(chars[i - 1].is_alphabetic()
                        || chars[i - 1].is_numeric()
                        || chars[i - 1] == '_')
                {
                    return true;
                }
            }
        }
        false
    }

    /// SpeedCrunch-style continuation: when the user starts a line with a
    /// binary operator (the previous result is the implied left operand),
    /// prefix `ans`, so `+5` reads as `ans+5`. `None` when no operator
    /// leads. Mode-aware: `%` is binary modulo and `& | << >>` are operators
    /// only in `Programmer`. (Hosts apply this only when the field was empty
    /// — a fresh continuation — never on a programmatic rewrite.)
    pub fn ans_prefixed(input: &str, mode: LanguageMode) -> Option<String> {
        let body = input.trim_start_matches(' ');
        if body.is_empty() {
            return None;
        }
        // Two-char operators first (so `<<` isn't read as a lone `<`).
        let leads: &[&str] = if mode == LanguageMode::Programmer {
            &[
                "<<", ">>", "+", "-", "*", "/", "^", "%", "&", "|", "×", "÷", "·",
            ]
        } else {
            &["+", "-", "*", "/", "^", "×", "÷", "·"]
        };
        if leads.iter().any(|lead| body.starts_with(lead)) {
            return Some(format!("ans{body}"));
        }
        None
    }

    /// The identifier fragment at the end of an input line — the thing
    /// autocomplete should complete and replace.
    pub fn trailing_identifier(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut start = chars.len();
        while start > 0
            && (chars[start - 1].is_alphabetic()
                || chars[start - 1].is_numeric()
                || chars[start - 1] == '_')
        {
            start -= 1;
        }
        // Identifiers can't start with a digit (that'd be a number literal).
        while start < chars.len() && chars[start].is_numeric() {
            start += 1;
        }
        chars[start..].iter().collect()
    }

    /// Candidates whose name starts with `prefix` (case-insensitive): the
    /// user's variables, the built-in constants, and every function. A
    /// single candidate that already equals the prefix is omitted — there's
    /// nothing left to complete.
    pub fn completions(&self, prefix: &str) -> Vec<Completion> {
        if prefix.is_empty() {
            return Vec::new();
        }
        let needle = prefix.to_lowercase();
        let mut matches: Vec<Completion> = Vec::new();
        for name in self.environment.user_variables().keys() {
            if name.to_lowercase().starts_with(&needle) {
                matches.push(Completion {
                    name: name.clone(),
                    kind: CompletionKind::Variable,
                });
            }
        }
        for function in self.environment.user_functions().values() {
            if function.name.to_lowercase().starts_with(&needle) {
                matches.push(Completion {
                    name: function.name.clone(),
                    kind: CompletionKind::Function,
                });
            }
        }
        // Data type constructors complete like functions (they take "(").
        for data_type in self.environment.user_data_types().values() {
            if data_type.name.to_lowercase().starts_with(&needle) {
                matches.push(Completion {
                    name: data_type.name.clone(),
                    kind: CompletionKind::Function,
                });
            }
        }
        for name in ["ans", "e", "pi", "tau", "true", "false", "Json"] {
            if name.to_lowercase().starts_with(&needle) {
                matches.push(Completion {
                    name: name.to_string(),
                    kind: CompletionKind::Constant,
                });
            }
        }
        for name in FunctionRegistry::standard().names() {
            if name.to_lowercase().starts_with(&needle) {
                matches.push(Completion {
                    name: name.to_string(),
                    kind: CompletionKind::Function,
                });
            }
        }
        // Special forms aren't in the registry.
        for special in ["sigma", "if", "man", "help"] {
            if special.starts_with(&needle) {
                matches.push(Completion {
                    name: special.to_string(),
                    kind: CompletionKind::Function,
                });
            }
        }

        matches.sort_by_key(|c| c.name.to_lowercase());
        if matches.len() == 1 && matches[0].name.to_lowercase() == needle {
            return Vec::new();
        }
        matches
    }
}
