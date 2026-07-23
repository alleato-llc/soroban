//! A re-parseable rendering of an expression. Used by lambda values for
//! display AND workbook persistence (`Value` description → parse), so the
//! contract is round-tripping, not prettiness: compound subexpressions are
//! parenthesized conservatively rather than by precedence analysis.

use super::{BinaryOperator, Expression, ReductionOperation};
use crate::LanguageMode;

/// Quotes a string with the language's `\" \\ \n \t` escapes — the exact
/// inverse of the lexer's string scanner. (`Value`'s string rendering — the
/// Swift `Value.quoted` — lives here so the renderer has no Value
/// dependency.)
pub fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A map key as source: bare when it re-lexes as an identifier, else a quoted
/// string literal (the Swift `Value.keyLiteral`).
pub fn key_literal(key: &str) -> String {
    let mut chars = key.chars();
    let plain = match chars.next() {
        Some(first) => {
            (first.is_alphabetic() || first == '_')
                && chars.all(|c| c.is_alphabetic() || c.is_numeric() || c == '_')
        }
        None => false,
    };
    if plain {
        key.to_string()
    } else {
        quoted(key)
    }
}

impl Expression {
    /// Canonical (Normal-mode) rendering. External callers — value
    /// descriptions, workbook persistence — get this, and it must round-trip
    /// via `Parser::parse`.
    pub fn source_text(&self) -> String {
        self.source_text_in(LanguageMode::Normal)
    }

    /// Renders under a display dialect. Only the overloaded glyphs differ
    /// between modes (`^ % & | << >>`); everything else is identical. In
    /// `Programmer` the canonical bitwise/mod calls and the power/percent
    /// nodes render with their infix glyphs; in `Normal`/`Scientific` they
    /// render as the canonical function / `^` / `%`. The result re-parses
    /// *under the same mode*. See `docs/MODES.md`.
    pub fn source_text_in(&self, mode: LanguageMode) -> String {
        let sub = |e: &Expression| e.source_text_in(mode);
        match self {
            Self::Number(value) => value.to_string(),
            // The currency literal is core grammar — it re-parses in any mode;
            // the symbol is part of the value, so it always renders.
            Self::Money { value, currency } => {
                let magnitude = if value.is_negative() {
                    -value
                } else {
                    value.clone()
                };
                let sign = if value.is_negative() { "-" } else { "" };
                format!("{sign}{}{magnitude}", currency.symbol())
            }
            // Grouping is presentation; source renders the plain number.
            Self::Grouped(value) => value.to_string(),
            Self::StringLiteral(text) => quoted(text),
            Self::Variable(name) => name.clone(),
            Self::CellReference { sheet, column, row } => {
                format!("{}{column}:{row}", qualified(sheet.as_deref()))
            }
            Self::CellRange {
                sheet,
                from_column,
                from_row,
                to_column,
                to_row,
            } => format!(
                "{}{from_column}:{from_row}..{to_column}:{to_row}",
                qualified(sheet.as_deref())
            ),
            Self::UnaryMinus(inner) => format!("(-{})", sub(inner)),
            Self::Percent(inner) => {
                // Programmer mode has no `%`-percent (it's modulo) → ×0.01.
                if mode == LanguageMode::Programmer {
                    format!("({} * 0.01)", sub(inner))
                } else {
                    // Postfix; compound inner expressions self-parenthesize,
                    // so `3%`, `x%`, `(a + b)%` all re-parse to the same
                    // percent.
                    format!("{}%", sub(inner))
                }
            }
            Self::Degrees(inner) => {
                // `°` is mode-agnostic (no dialect owns another meaning), so
                // it renders — and re-parses — identically everywhere.
                format!("{}°", sub(inner))
            }
            Self::Binary(op, lhs, rhs) => {
                // Power has no `^` glyph in Programmer mode (`^` is XOR).
                if mode == LanguageMode::Programmer && *op == BinaryOperator::Power {
                    format!("pow({}, {})", sub(lhs), sub(rhs))
                } else {
                    format!("({} {} {})", sub(lhs), op.symbol(), sub(rhs))
                }
            }
            Self::Comparison(op, lhs, rhs) => {
                format!("({} {} {})", sub(lhs), op.symbol(), sub(rhs))
            }
            Self::Call { name, arguments } => {
                // Programmer mode re-spells the canonical bitwise/mod calls
                // as infix.
                if mode == LanguageMode::Programmer {
                    if name.eq_ignore_ascii_case("bitnot") && arguments.len() == 1 {
                        return format!("~{}", sub(&arguments[0]));
                    }
                    if let Some(infix) = programmer_infix(name, arguments, &sub) {
                        return infix;
                    }
                }
                let args: Vec<String> = arguments.iter().map(sub).collect();
                format!("{name}({})", args.join(", "))
            }
            Self::Conditional {
                condition,
                then,
                otherwise,
            } => {
                format!("if({}, {}, {})", sub(condition), sub(then), sub(otherwise))
            }
            Self::Assignment { name, value } => format!("{name} = {}", sub(value)),
            Self::FunctionDefinition {
                name,
                parameters,
                body,
            } => {
                let params: Vec<String> = parameters.iter().map(|p| p.rendered()).collect();
                format!("{name}({}) = {}", params.join(", "), sub(body))
            }
            Self::Reduction {
                operation,
                index,
                lower,
                upper,
                body,
            } => {
                // The typed spelling; bounds parenthesized (boundPrimary
                // takes those).
                let keyword = match operation {
                    ReductionOperation::Sum => "sigma",
                    ReductionOperation::Product => "product",
                };
                format!(
                    "{keyword}_{index}=({})^({})({})",
                    sub(lower),
                    sub(upper),
                    sub(body)
                )
            }
            Self::HelpRequest { name } => format!("man {name}"),
            Self::ArrayLiteral(items) => {
                let rendered: Vec<String> = items.iter().map(sub).collect();
                format!("[{}]", rendered.join(", "))
            }
            Self::MapLiteral(entries) => {
                let body: Vec<String> = entries
                    .iter()
                    .map(|e| format!("{}: {}", key_literal(&e.key), sub(&e.value)))
                    .collect();
                format!("{{{}}}", body.join(", "))
            }
            Self::Index { base, index } => format!("{}[{}]", sub(base), sub(index)),
            Self::Member { base, name } => format!("{}.{name}", sub(base)),
            Self::MethodCall {
                base,
                name,
                arguments,
            } => {
                let args: Vec<String> = arguments.iter().map(sub).collect();
                format!("{}.{name}({})", sub(base), args.join(", "))
            }
            Self::Lambda { parameters, body } => {
                format!("({}) -> {}", parameters.join(", "), sub(body))
            }
            Self::NameReference { sheet, name } => {
                format!("{}'{name}'", qualified(sheet.as_deref()))
            }
            Self::DataDefinition { name, fields } => {
                let rendered: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.field_type.label()))
                    .collect();
                format!("data {name} {{ {} }}", rendered.join(", "))
            }
            Self::NamespaceDefinition { name, members } => {
                let rendered: Vec<String> = members.iter().map(sub).collect();
                format!("namespace {name} {{ {} }}", rendered.join("; "))
            }
            Self::ImportDirective { name } => format!("import {name}"),
        }
    }
}

/// In Programmer mode the canonical 2-arg bitwise/mod calls render with their
/// infix glyphs (parenthesized, like `Binary`, for safe re-parsing). `None`
/// when the call isn't one of these (render as an ordinary call). `>>` is
/// recovered from a negated shift count (`bitShift(a, -n)` ≡ `a >> n`).
fn programmer_infix(
    name: &str,
    args: &[Expression],
    sub: &dyn Fn(&Expression) -> String,
) -> Option<String> {
    if args.len() != 2 {
        return None;
    }
    let lhs = sub(&args[0]);
    match name.to_lowercase().as_str() {
        "bitxor" => Some(format!("({lhs} ^ {})", sub(&args[1]))),
        "bitand" => Some(format!("({lhs} & {})", sub(&args[1]))),
        "bitor" => Some(format!("({lhs} | {})", sub(&args[1]))),
        "mod" => Some(format!("({lhs} % {})", sub(&args[1]))),
        "bitshift" => {
            if let Expression::UnaryMinus(inner) = &args[1] {
                Some(format!("({lhs} >> {})", sub(inner)))
            } else {
                Some(format!("({lhs} << {})", sub(&args[1])))
            }
        }
        _ => None,
    }
}

fn qualified(sheet: Option<&str>) -> String {
    let Some(sheet) = sheet else {
        return String::new();
    };
    // Quote unless the name is a plain identifier.
    let plain = !sheet.is_empty()
        && sheet.chars().next().is_some_and(char::is_alphabetic)
        && sheet
            .chars()
            .all(|c| c.is_alphabetic() || c.is_numeric() || c == '_');
    if plain {
        format!("{sheet}!")
    } else {
        format!("'{sheet}'!")
    }
}
