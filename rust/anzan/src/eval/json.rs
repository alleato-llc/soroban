//! JSON encode/decode — port of Eval/JSON.swift (hand-rolled deliberately:
//! JSON number literals must go straight to BigDecimal, never through f64).
//!
//! Both directions of JSON live here: `json_text` (the toJson() builtin) and
//! `JsonParser` (fromJson()) — exact inverses for everything Anzan can
//! represent.

use super::value::{MapEntry, Value};
use crate::{BigDecimal, EngineError};

// MARK: - Serializing (toJson)

/// JSON text: numbers bare (BigDecimal's canonical text is valid JSON),
/// strings escaped, arrays/maps/records as arrays/objects. Boolean-declared
/// record fields come out as true/false — the type declaration is what makes
/// that possible. Functions refuse.
pub(crate) fn json_text(value: &Value, pretty: bool) -> Result<String, EngineError> {
    json_text_at(value, pretty, 0)
}

fn json_text_at(value: &Value, pretty: bool, depth: usize) -> Result<String, EngineError> {
    match value {
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) => Ok(json_quoted(text)),
        Value::Array(items) => {
            if items.is_empty() {
                return Ok("[]".to_string());
            }
            let mut rendered = Vec::with_capacity(items.len());
            for item in items {
                rendered.push(json_text_at(item, pretty, depth + 1)?);
            }
            Ok(joined(&rendered, ("[", "]"), pretty, depth))
        }
        Value::Map(entries) => json_object(entries, &|_| false, pretty, depth),
        Value::Record(record) => json_object(
            &record.entries,
            &|key| record.boolean_fields.contains(key),
            pretty,
            depth,
        ),
        // A bounded integer is a JSON number (its exact value).
        Value::FixedInt(fixed) => Ok(fixed.value.to_string()),
        // A JSON number, kept at the declared scale (e.g. 10.50).
        Value::FixedDecimal(decimal) => Ok(decimal.text()),
        // Money is a JSON number — the symbol is presentation, and JSON has no
        // currency notion to carry it.
        Value::Money(m) => Ok(m.value.to_string()),
        // Grouping is presentation; JSON gets the plain number.
        Value::Grouped(n) => Ok(n.to_string()),
        Value::Function(_) => Err(EngineError::domain("toJson() can't serialize a function")),
        Value::Host(object) => Err(EngineError::domain(format!(
            "toJson() can't serialize a {}",
            object.type_name()
        ))),
    }
}

fn json_object(
    entries: &[MapEntry],
    is_boolean_field: &dyn Fn(&str) -> bool,
    pretty: bool,
    depth: usize,
) -> Result<String, EngineError> {
    if entries.is_empty() {
        return Ok("{}".to_string());
    }
    let mut rendered = Vec::with_capacity(entries.len());
    for entry in entries {
        let value = match &entry.value {
            Value::Number(flag) if is_boolean_field(&entry.key) => {
                if flag.is_zero() { "false" } else { "true" }.to_string()
            }
            other => json_text_at(other, pretty, depth + 1)?,
        };
        let separator = if pretty { ": " } else { ":" };
        rendered.push(format!("{}{separator}{value}", json_quoted(&entry.key)));
    }
    Ok(joined(&rendered, ("{", "}"), pretty, depth))
}

/// Compact packs everything; pretty is the conventional 2-space layout.
fn joined(parts: &[String], brackets: (&str, &str), pretty: bool, depth: usize) -> String {
    if !pretty {
        return format!("{}{}{}", brackets.0, parts.join(","), brackets.1);
    }
    let pad = "  ".repeat(depth + 1);
    let body: Vec<String> = parts.iter().map(|part| format!("{pad}{part}")).collect();
    format!(
        "{}\n{}\n{}{}",
        brackets.0,
        body.join(",\n"),
        "  ".repeat(depth),
        brackets.1
    )
}

/// JSON string escaping — the JSON set, not the lexer's (adds \r and \u00XX
/// for remaining control characters).
fn json_quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for scalar in text.chars() {
        match scalar {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => {
                if (scalar as u32) < 0x20 {
                    out.push_str(&format!("\\u{:04X}", scalar as u32));
                } else {
                    out.push(scalar);
                }
            }
        }
    }
    out.push('"');
    out
}

// MARK: - Parsing (fromJson)

/// Parses JSON text into a `Value` — `json_text`'s inverse for everything
/// Anzan can represent: objects → maps, arrays → arrays, strings → strings,
/// numbers → exact decimals, true/false → 1/0.
///
/// Hand-rolled on purpose: a stock JSON library round-trips numbers through
/// f64, which is precisely the float drift this engine exists to refuse.
/// Number literals here go straight to `BigDecimal::parse` at full precision.
///
/// JSON `null` is refused — Anzan deliberately has no null (see
/// docs/ANZAN.md, Influences) and won't invent a coercion for it.
pub(crate) struct JsonParser {
    chars: Vec<char>,
    index: usize,
    depth: usize,
}

/// Nesting cap: honest data never comes close; a pathological input errors
/// instead of chewing the parser's stack. Matches the Swift engine's cap.
const MAX_DEPTH: usize = 128;

impl JsonParser {
    pub(crate) fn parse(text: &str) -> Result<Value, EngineError> {
        let mut parser = JsonParser {
            chars: text.chars().collect(),
            index: 0,
            depth: 0,
        };
        parser.skip_whitespace();
        let value = parser.value()?;
        parser.skip_whitespace();
        if parser.index != parser.chars.len() {
            return Err(parser.error("unexpected trailing content"));
        }
        Ok(value)
    }

    // MARK: Scanning

    fn current(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.index += 1;
            } else {
                break;
            }
        }
    }

    fn error(&self, message: &str) -> EngineError {
        EngineError::domain(format!(
            "fromJson: {message} at character {}",
            self.index + 1
        ))
    }

    // MARK: Values

    fn value(&mut self) -> Result<Value, EngineError> {
        self.depth += 1;
        let result = self.value_inner();
        self.depth -= 1;
        result
    }

    fn value_inner(&mut self) -> Result<Value, EngineError> {
        if self.depth > MAX_DEPTH {
            return Err(self.error(&format!("nesting deeper than {MAX_DEPTH} levels")));
        }
        match self.current() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => Ok(Value::String(self.string()?)),
            Some('t') | Some('f') | Some('n') => self.keyword(),
            Some(c) if c == '-' || c.is_numeric() => self.number(),
            Some(c) => Err(self.error(&format!("unexpected character '{c}'"))),
            None => Err(self.error("unexpected end of JSON")),
        }
    }

    fn object(&mut self) -> Result<Value, EngineError> {
        self.index += 1; // '{'
        let mut entries: Vec<MapEntry> = Vec::new();
        self.skip_whitespace();
        if self.current() == Some('}') {
            self.index += 1;
            return Ok(Value::Map(Vec::new()));
        }
        loop {
            self.skip_whitespace();
            if self.current() != Some('"') {
                return Err(self.error("expected a quoted object key"));
            }
            let key = self.string()?;
            if entries.iter().any(|entry| entry.key == key) {
                return Err(self.error(&format!("duplicate key \"{key}\"")));
            }
            self.skip_whitespace();
            if self.current() != Some(':') {
                return Err(self.error(&format!("expected ':' after key \"{key}\"")));
            }
            self.index += 1;
            self.skip_whitespace();
            let value = self.value()?;
            entries.push(MapEntry::new(key, value));
            self.skip_whitespace();
            match self.current() {
                Some(',') => self.index += 1,
                Some('}') => {
                    self.index += 1;
                    return Ok(Value::Map(entries));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<Value, EngineError> {
        self.index += 1; // '['
        let mut items: Vec<Value> = Vec::new();
        self.skip_whitespace();
        if self.current() == Some(']') {
            self.index += 1;
            return Ok(Value::Array(Vec::new()));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value()?);
            self.skip_whitespace();
            match self.current() {
                Some(',') => self.index += 1,
                Some(']') => {
                    self.index += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn keyword(&mut self) -> Result<Value, EngineError> {
        let rest: String = self.chars[self.index..].iter().collect();
        if rest.starts_with("true") {
            self.index += 4;
            return Ok(Value::Number(BigDecimal::one()));
        }
        if rest.starts_with("false") {
            self.index += 5;
            return Ok(Value::Number(BigDecimal::zero()));
        }
        if rest.starts_with("null") {
            return Err(
                self.error("JSON null has no Anzan value — remove it or default it before parsing")
            );
        }
        let c = self.current().unwrap();
        Err(self.error(&format!("unexpected character '{c}'")))
    }

    /// JSON's number grammar, handed to BigDecimal at full precision. The
    /// leading sign is split off (the engine's number parser, like its lexer,
    /// treats signs as separate).
    fn number(&mut self) -> Result<Value, EngineError> {
        let start = self.index;
        let mut negative = false;
        if self.current() == Some('-') {
            negative = true;
            self.index += 1;
        }
        let digits_start = self.index;
        while let Some(c) = self.current() {
            let sign_after_exponent = (c == '+' || c == '-')
                && self.index > 0
                && matches!(self.chars[self.index - 1], 'e' | 'E');
            if c.is_numeric() || c == '.' || c == 'e' || c == 'E' || sign_after_exponent {
                self.index += 1;
            } else {
                break;
            }
        }
        let text: String = self.chars[digits_start..self.index].iter().collect();
        let magnitude = if text.is_empty() {
            None
        } else {
            BigDecimal::parse(&text)
        };
        let Some(magnitude) = magnitude else {
            self.index = start;
            return Err(self.error("malformed number"));
        };
        Ok(Value::Number(if negative { -magnitude } else { magnitude }))
    }

    /// `"…"` with the full JSON escape set, including \uXXXX and surrogate
    /// pairs (which is why this scans rather than reusing the lexer).
    fn string(&mut self) -> Result<String, EngineError> {
        self.index += 1; // opening quote
        let mut text = String::new();
        while let Some(c) = self.current() {
            match c {
                '"' => {
                    self.index += 1;
                    return Ok(text);
                }
                '\\' => {
                    self.index += 1;
                    match self.current() {
                        Some('"') => {
                            text.push('"');
                            self.index += 1;
                        }
                        Some('\\') => {
                            text.push('\\');
                            self.index += 1;
                        }
                        Some('/') => {
                            text.push('/');
                            self.index += 1;
                        }
                        Some('n') => {
                            text.push('\n');
                            self.index += 1;
                        }
                        Some('t') => {
                            text.push('\t');
                            self.index += 1;
                        }
                        Some('r') => {
                            text.push('\r');
                            self.index += 1;
                        }
                        Some('b') => {
                            text.push('\u{8}');
                            self.index += 1;
                        }
                        Some('f') => {
                            text.push('\u{C}');
                            self.index += 1;
                        }
                        Some('u') => {
                            self.index += 1;
                            let c = self.unicode_escape()?;
                            text.push(c);
                        }
                        Some(escaped) => {
                            return Err(self.error(&format!("unknown escape '\\{escaped}'")));
                        }
                        None => return Err(self.error("unterminated string")),
                    }
                }
                _ => {
                    text.push(c);
                    self.index += 1;
                }
            }
        }
        Err(self.error("unterminated string"))
    }

    /// The 4 hex digits after `\u` — possibly the high half of a surrogate
    /// pair, in which case the matching `\uDC00–\uDFFF` must follow.
    fn unicode_escape(&mut self) -> Result<char, EngineError> {
        let high = self.hex4()?;
        if (0xD800..=0xDBFF).contains(&high) {
            let followed = self.current() == Some('\\')
                && self.index + 1 < self.chars.len()
                && self.chars[self.index + 1] == 'u';
            if !followed {
                return Err(self.error("missing low surrogate after \\u escape"));
            }
            self.index += 2;
            let low = self.hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.error("invalid low surrogate in \\u escape"));
            }
            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return Ok(char::from_u32(combined).expect("combined surrogate is a valid scalar"));
        }
        // A lone low surrogate has no scalar value.
        char::from_u32(high).ok_or_else(|| self.error("invalid \\u escape"))
    }

    fn hex4(&mut self) -> Result<u32, EngineError> {
        let code = if self.index + 4 <= self.chars.len() {
            let digits: String = self.chars[self.index..self.index + 4].iter().collect();
            u32::from_str_radix(&digits, 16).ok()
        } else {
            None
        };
        let Some(code) = code else {
            return Err(self.error("\\u needs 4 hex digits"));
        };
        self.index += 4;
        Ok(code)
    }
}
