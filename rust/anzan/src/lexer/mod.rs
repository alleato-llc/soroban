//! Converts a source line into tokens. Whitespace separates tokens but is
//! otherwise insignificant. Positions are character offsets (a `Vec<char>`
//! of Unicode scalars — the Swift side counts grapheme clusters, identical
//! for everything the language can express) so hosts can render a caret
//! under the offending column; preserve them when changing the lexer.

mod token;

pub use token::{Token, TokenKind};

use crate::{BigDecimal, EngineError};
use num_bigint::BigInt;

pub struct Lexer {
    chars: Vec<char>,
    index: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            index: 0,
        }
    }

    /// Splits a source line into its code and its trailing `#` comment,
    /// respecting string literals (a `#` inside `"…"` is not a comment). The
    /// comment is returned WITHOUT the leading `#`, trimmed; `None` when
    /// absent. Both hosts use this to display/store comments; the calculator
    /// uses it to recognize a comment-only line as a note rather than a parse
    /// error.
    pub fn split_comment(source: &str) -> (String, Option<String>) {
        let chars: Vec<char> = source.chars().collect();
        let mut index = 0;
        let mut in_string = false;
        while index < chars.len() {
            let c = chars[index];
            if in_string {
                if c == '\\' {
                    index += 2; // skip the escaped char
                    continue;
                }
                if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if c == '#' {
                let code: String = chars[..index].iter().collect();
                let comment: String = chars[index + 1..].iter().collect();
                return (code, Some(comment.trim().to_string()));
            }
            index += 1;
        }
        (source.to_string(), None)
    }

    /// Tokenizes the whole input, appending a final `End` token.
    pub fn tokenize(source: &str) -> Result<Vec<Token>, EngineError> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token()? {
            tokens.push(token);
        }
        tokens.push(Token {
            kind: TokenKind::End,
            range: lexer.index..lexer.index,
        });
        Ok(tokens)
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn char_at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    fn next_token(&mut self) -> Result<Option<Token>, EngineError> {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.index += 1;
            } else {
                break;
            }
        }
        let Some(c) = self.current() else {
            return Ok(None);
        };
        let start = self.index;

        // '#' starts a comment that runs to the end of the line. A trailing
        // comment on a function definition doubles as its documentation.
        if c == '#' {
            self.index = self.chars.len();
            return Ok(None);
        }

        // Two-character operators before the single-character table
        // (so `==` never lexes as two assigns, `<=` as less-then-assign).
        if let Some(next) = self.char_at(self.index + 1) {
            let kind = match (c, next) {
                ('=', '=') => Some(TokenKind::EqualEqual),
                ('!', '=') => Some(TokenKind::NotEqual),
                ('<', '=') => Some(TokenKind::LessOrEqual),
                ('>', '=') => Some(TokenKind::GreaterOrEqual),
                ('<', '<') => Some(TokenKind::ShiftLeft), // before the single-char `<`/`>`
                ('>', '>') => Some(TokenKind::ShiftRight),
                ('.', '.') => Some(TokenKind::DotDot), // before the number scanner grabs the first '.'
                ('-', '>') => Some(TokenKind::Arrow),  // before '-' lexes as minus
                (':', ':') => Some(TokenKind::ColonColon), // namespace qualifier, before the single ':'
                _ => None,
            };
            if let Some(kind) = kind {
                self.index += 2;
                return Ok(Some(Token {
                    kind,
                    range: start..self.index,
                }));
            }
        }

        // A lone '.' is member access (m.name); '.' before a digit starts a
        // number (.5). Must run after the two-char pass ('..' is a range) and
        // before the simple table would otherwise claim it.
        if c == '.' {
            if self.char_at(self.index + 1).is_some_and(|n| n.is_numeric()) {
                return self.number(start).map(Some);
            }
            self.index += 1;
            return Ok(Some(Token {
                kind: TokenKind::Dot,
                range: start..self.index,
            }));
        }

        // Single-character operators and punctuation. The typographic forms
        // (× ÷ − ·) arrive constantly via copy/paste from documents.
        let simple = match c {
            '+' => Some(TokenKind::Plus),
            '-' | '−' => Some(TokenKind::Minus),
            '*' | '×' | '·' => Some(TokenKind::Star),
            '/' | '÷' => Some(TokenKind::Slash),
            '%' => Some(TokenKind::Percent),
            '^' => Some(TokenKind::Caret),
            '=' => Some(TokenKind::Assign),
            '(' => Some(TokenKind::LeftParen),
            ')' => Some(TokenKind::RightParen),
            ',' => Some(TokenKind::Comma),
            '[' => Some(TokenKind::LeftBracket),
            ']' => Some(TokenKind::RightBracket),
            '{' => Some(TokenKind::LeftBrace),
            '}' => Some(TokenKind::RightBrace),
            ':' => Some(TokenKind::Colon), // a cell reference consumes its own ':' (A:1)
            ';' => Some(TokenKind::Semicolon), // namespace member separator
            '√' => Some(TokenKind::SqrtSign),
            '<' => Some(TokenKind::LessThan),
            '>' => Some(TokenKind::GreaterThan),
            '&' => Some(TokenKind::Ampersand), // Programmer-mode bitwise; parser errors elsewhere
            '|' => Some(TokenKind::Pipe),
            '~' => Some(TokenKind::Tilde), // Programmer-mode bitwise NOT
            '≤' => Some(TokenKind::LessOrEqual),
            '≥' => Some(TokenKind::GreaterOrEqual),
            '≠' => Some(TokenKind::NotEqual),
            '!' => Some(TokenKind::Bang), // sheet qualifier — "!=" was caught by the two-char pass
            // Math symbols aren't letters, so the identifier scanner can't
            // pick them up.
            '∑' => Some(TokenKind::Identifier("sigma".to_string())),
            '∏' => Some(TokenKind::Identifier("product".to_string())),
            _ => None,
        };
        if let Some(kind) = simple {
            self.index += 1;
            return Ok(Some(Token {
                kind,
                range: start..self.index,
            }));
        }

        // "…" string literal with \" \\ \n \t escapes.
        if c == '"' {
            return self.string_literal(start).map(Some);
        }

        // 'Quoted Sheet Name' — for names the identifier syntax can't carry.
        if c == '\'' {
            self.index += 1;
            let name_start = self.index;
            while self.current().is_some_and(|c| c != '\'') {
                self.index += 1;
            }
            if self.current() != Some('\'') {
                return Err(EngineError::LexError {
                    message: "unterminated sheet name quote".to_string(),
                    position: start,
                });
            }
            let name: String = self.chars[name_start..self.index].iter().collect();
            self.index += 1;
            return Ok(Some(Token {
                kind: TokenKind::QuotedName(name),
                range: start..self.index,
            }));
        }

        if c.is_numeric() {
            return self.number(start).map(Some);
        }

        // '$' pins a cell reference's column ($A:1); the row pin rides the
        // reference tail. Anything else after '$' is a loud lex error.
        if c == '$' {
            self.index += 1;
            let name_start = self.index;
            while self.current().is_some_and(char::is_alphabetic) {
                self.index += 1;
            }
            let name: String = self.chars[name_start..self.index].iter().collect();
            if !name.is_empty() && self.current() == Some(':') {
                if let Some(token) = self.cell_reference_tail(&name, start, true)? {
                    return Ok(Some(token));
                }
            }
            return Err(EngineError::LexError {
                message: "'$' pins a cell reference — write $A:1, A:$1, or $A:$1".to_string(),
                position: start,
            });
        }

        if c.is_alphabetic() || c == '_' {
            while self
                .current()
                .is_some_and(|c| c.is_alphabetic() || c.is_numeric() || c == '_')
            {
                self.index += 1;
            }
            let name: String = self.chars[start..self.index].iter().collect();

            // Letters-only identifier followed by ":<digits>" (or ":$digits"
            // — a pinned row) is a cell reference: A:1, b:12, A:$1.
            // Anything else keeps the ':' unconsumed.
            if name.chars().all(char::is_alphabetic) && self.current() == Some(':') {
                if let Some(token) = self.cell_reference_tail(&name, start, false)? {
                    return Ok(Some(token));
                }
            }
            return Ok(Some(Token {
                kind: TokenKind::Identifier(name),
                range: start..self.index,
            }));
        }

        Err(EngineError::LexError {
            message: format!("unexpected character '{c}'"),
            position: start,
        })
    }

    /// Consumes `:[$]digits` after a column name, or returns `None` leaving
    /// the ':' unconsumed (the caller's name is then a plain identifier).
    /// Expects `current == ':'`.
    fn cell_reference_tail(
        &mut self,
        column: &str,
        start: usize,
        pin_column: bool,
    ) -> Result<Option<Token>, EngineError> {
        let mut peek = self.index + 1; // past ':'
        let mut pin_row = false;
        if self.char_at(peek) == Some('$') {
            pin_row = true;
            peek += 1;
        }
        if !self.char_at(peek).is_some_and(|c| c.is_numeric()) {
            return Ok(None);
        }
        self.index = peek;
        let row_start = self.index;
        while self.current().is_some_and(|c| c.is_numeric()) {
            self.index += 1;
        }
        let row_text: String = self.chars[row_start..self.index].iter().collect();
        let Ok(row) = row_text.parse::<i64>() else {
            return Err(EngineError::LexError {
                message: "cell row out of range".to_string(),
                position: row_start,
            });
        };
        // Column case is preserved (resolution is case-insensitive anyway) —
        // map literals decompose compact `{b:1}` into key "b" + value 1 and
        // want the key exactly as typed.
        Ok(Some(Token {
            kind: TokenKind::CellReference {
                column: column.to_string(),
                row,
                pin_column,
                pin_row,
            },
            range: start..self.index,
        }))
    }

    /// Scans `"…"`, applying `\"` `\\` `\n` `\t` escapes. Unterminated
    /// strings and unknown escapes are lex errors.
    fn string_literal(&mut self, start: usize) -> Result<Token, EngineError> {
        self.index += 1; // opening quote
        let mut text = String::new();
        while let Some(c) = self.current() {
            if c == '"' {
                self.index += 1;
                return Ok(Token {
                    kind: TokenKind::String(text),
                    range: start..self.index,
                });
            }
            if c == '\\' {
                self.index += 1;
                match self.current() {
                    Some('\\') => text.push('\\'),
                    Some('"') => text.push('"'),
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some(escaped) => {
                        return Err(EngineError::LexError {
                            message: format!("unknown escape '\\{escaped}'"),
                            position: self.index - 1,
                        });
                    }
                    None => break, // falls out to the unterminated error below
                }
                self.index += 1;
                continue;
            }
            text.push(c);
            self.index += 1;
        }
        Err(EngineError::LexError {
            message: "unterminated string".to_string(),
            position: start,
        })
    }

    /// Scans `123`, `1.5`, `1_000`, `2.5e-3` — and the programmer literals
    /// `0xFF` / `0b1010` (any width; `_` separators welcome). The leading
    /// sign is not part of the literal — the parser handles unary minus.
    fn number(&mut self, start: usize) -> Result<Token, EngineError> {
        if self.chars[self.index] == '0' {
            let radix = match self.char_at(self.index + 1) {
                Some('x') | Some('X') => Some(16),
                Some('b') | Some('B') => Some(2),
                _ => None,
            };
            if let Some(radix) = radix {
                return self.radix_literal(start, radix);
            }
        }
        let mut saw_dot = false;
        while let Some(c) = self.current() {
            match c {
                '0'..='9' | '_' => self.index += 1,
                '.' if !saw_dot => {
                    saw_dot = true;
                    self.index += 1;
                }
                'e' | 'E' => {
                    // Exponent only counts if followed by digits (with
                    // optional sign); otherwise it's the start of an
                    // identifier (2e → number 2 then identifier e).
                    let mut peek = self.index + 1;
                    if matches!(self.char_at(peek), Some('+') | Some('-')) {
                        peek += 1;
                    }
                    if !self.char_at(peek).is_some_and(|c| c.is_numeric()) {
                        break;
                    }
                    self.index = peek + 1;
                    while self.current().is_some_and(|c| c.is_numeric()) {
                        self.index += 1;
                    }
                    break;
                }
                _ => break,
            }
        }

        // A second dot directly after the literal ("1.2.3") would otherwise
        // lex as two numbers and silently become implicit multiplication.
        if saw_dot && self.current() == Some('.') {
            let shown: String = self.chars[start..=self.index].iter().collect();
            return Err(EngineError::LexError {
                message: format!("malformed number '{shown}'"),
                position: start,
            });
        }

        let text: String = self.chars[start..self.index].iter().collect();
        let Some(value) = BigDecimal::parse(&text) else {
            return Err(EngineError::LexError {
                message: format!("malformed number '{text}'"),
                position: start,
            });
        };
        Ok(Token {
            kind: TokenKind::Number(value),
            range: start..self.index,
        })
    }

    /// `0xDEAD_BEEF` / `0b1010_1010` — exact integers at any width. A letter,
    /// digit, or '.' left dangling after the digits is a lex error (so `0xFG`
    /// and `0x1.5` fail loudly instead of becoming implicit multiplications).
    fn radix_literal(&mut self, start: usize, radix: u32) -> Result<Token, EngineError> {
        self.index += 2; // "0x" / "0b"
        let mut value = BigInt::from(0);
        let mut digits = 0;
        while let Some(c) = self.current() {
            if c == '_' {
                self.index += 1;
                continue;
            }
            let digit = match radix {
                16 => match c.to_digit(16) {
                    Some(hex) => hex,
                    None => break,
                },
                _ => match c {
                    '0' => 0,
                    '1' => 1,
                    _ => break,
                },
            };
            value = value * radix + digit;
            digits += 1;
            self.index += 1;
        }
        let name = if radix == 16 { "hex" } else { "binary" };
        if digits == 0 {
            return Err(EngineError::LexError {
                message: format!("{name} literal needs digits"),
                position: start,
            });
        }
        if self
            .current()
            .is_some_and(|c| c.is_alphabetic() || c.is_numeric() || c == '.')
        {
            let shown: String = self.chars[start..=self.index].iter().collect();
            return Err(EngineError::LexError {
                message: format!("malformed {name} literal '{shown}'"),
                position: start,
            });
        }
        Ok(Token {
            kind: TokenKind::Number(BigDecimal::new(value, 0)),
            range: start..self.index,
        })
    }
}

#[cfg(test)]
mod tests;
