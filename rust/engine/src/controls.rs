//! Control expressions — sliders, steppers, checkboxes, dropdowns: a cell
//! whose expression is a literal-argument control call renders as an
//! interactive control, and interaction rewrites the storage literal in
//! place. Combined with 𝑖 definitions (`rate = slider(0.08, 0, 0.2)`) the
//! control is named, sheet-scoped, and immutable from the log — all
//! inherited.

use crate::cell::{Cell, Content, DefinitionKind};
use crate::spreadsheet::CellDisplay;
use anzan::ast::Expression;
use anzan::lexer::{Lexer, TokenKind};
use anzan::{BigDecimal, Value};

/// Everything the grid needs to draw and drag one slider (or stepper).
#[derive(Debug, Clone, PartialEq)]
pub struct SliderInfo {
    /// The 𝑖 name when the slider is a definition; `None` for an anonymous
    /// `=slider(…)` cell (read it by address instead).
    pub name: Option<String>,
    /// Clamped into minimum...maximum.
    pub value: BigDecimal,
    pub minimum: BigDecimal,
    pub maximum: BigDecimal,
    /// Explicit 4th argument, or (max−min)/100 — an exact exponent shift.
    pub step: BigDecimal,
}

impl SliderInfo {
    /// Builds from a `slider(…)`/`stepper(…)` call whose arguments are all
    /// numeric LITERALS (the value argument IS the storage — it can't be an
    /// expression). `None` for any other shape; invalid ranges fall through
    /// to normal evaluation, which reports the error. Default step:
    /// (max−min)/100 for sliders, 1 for steppers.
    pub(crate) fn extract(
        expression: &Expression,
        name: Option<&str>,
        function: &str,
    ) -> Option<SliderInfo> {
        let Expression::Call {
            name: call_name,
            arguments,
        } = expression
        else {
            return None;
        };
        if !call_name.eq_ignore_ascii_case(function) || !(3..=4).contains(&arguments.len()) {
            return None;
        }

        let mut literals: Vec<BigDecimal> = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let Some(Value::Number(literal)) = Control::literal_value(argument) else {
                return None;
            };
            literals.push(literal);
        }
        let minimum = literals[1].clone();
        let maximum = literals[2].clone();
        if minimum >= maximum {
            return None; // evaluation reports this
        }
        if literals.len() == 4 && literals[3] <= BigDecimal::zero() {
            return None;
        }

        let span = &maximum - &minimum;
        let step = if literals.len() == 4 {
            literals[3].clone()
        } else if function == "stepper" {
            BigDecimal::one()
        } else {
            BigDecimal::new(span.significand().clone(), span.exponent() - 2)
        };
        let value = literals[0]
            .clone()
            .max(minimum.clone())
            .min(maximum.clone());
        Some(SliderInfo {
            name: name.map(str::to_string),
            value,
            minimum,
            maximum,
            step,
        })
    }
}

/// A checkbox cell: `flag = checkbox(true)`. Clicking flips the literal.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckboxInfo {
    pub name: Option<String>,
    pub is_on: bool,
}

impl CheckboxInfo {
    fn extract(expression: &Expression, name: Option<&str>) -> Option<CheckboxInfo> {
        let Expression::Call {
            name: call_name,
            arguments,
        } = expression
        else {
            return None;
        };
        if !call_name.eq_ignore_ascii_case("checkbox") || arguments.len() != 1 {
            return None;
        }
        let Some(Value::Number(state)) = Control::literal_value(&arguments[0]) else {
            return None;
        };
        Some(CheckboxInfo {
            name: name.map(str::to_string),
            is_on: !state.is_zero(),
        })
    }
}

/// A dropdown cell: `region = dropdown("EU", ["EU", "US", "APAC"])`. The
/// cell's value IS the selected option; choosing rewrites the literal.
/// Options are literals too — strings or numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct DropdownInfo {
    pub name: Option<String>,
    pub value: Value,
    pub options: Vec<Value>,
}

impl DropdownInfo {
    fn extract(expression: &Expression, name: Option<&str>) -> Option<DropdownInfo> {
        let Expression::Call {
            name: call_name,
            arguments,
        } = expression
        else {
            return None;
        };
        if !call_name.eq_ignore_ascii_case("dropdown") || arguments.len() != 2 {
            return None;
        }
        let value = Control::literal_value(&arguments[0])?;
        let Expression::ArrayLiteral(items) = &arguments[1] else {
            return None;
        };
        if items.is_empty() {
            return None;
        }
        let mut options = Vec::with_capacity(items.len());
        for item in items {
            options.push(Control::literal_value(item)?);
        }
        Some(DropdownInfo {
            name: name.map(str::to_string),
            value,
            options,
        })
    }
}

pub struct Control;

impl Control {
    const NAMES: [&'static str; 4] = ["slider", "stepper", "checkbox", "dropdown"];

    /// The literal forms a control's storage argument may take: numbers
    /// (optionally signed), true/false, and "strings".
    fn literal_value(expression: &Expression) -> Option<Value> {
        match expression {
            Expression::Number(value) => Some(Value::Number(value.clone())),
            Expression::UnaryMinus(inner) => {
                if let Expression::Number(value) = inner.as_ref() {
                    Some(Value::Number(-value))
                } else {
                    None
                }
            }
            Expression::StringLiteral(text) => Some(Value::String(text.clone())),
            Expression::Variable(name) if name.eq_ignore_ascii_case("true") => {
                Some(Value::Number(BigDecimal::one()))
            }
            Expression::Variable(name) if name.eq_ignore_ascii_case("false") => {
                Some(Value::Number(BigDecimal::zero()))
            }
            _ => None,
        }
    }

    /// The cell's control, if its content is a control expression: either a
    /// 𝑖 definition whose body is a control call (named) or a plain/`=`
    /// formula that IS one (anonymous).
    pub(crate) fn display(cell: &Cell) -> Option<CellDisplay> {
        let (expression, name): (&Expression, Option<&str>) = match &cell.content {
            Content::Definition(definition) => {
                let DefinitionKind::Variable(body) = &definition.kind else {
                    return None;
                };
                (body, Some(definition.name.as_str()))
            }
            Content::ExplicitFormula(Ok(body)) | Content::Candidate(body) => (body, None),
            _ => return None,
        };

        let Expression::Call {
            name: call_name, ..
        } = expression
        else {
            return None;
        };
        match call_name.to_lowercase().as_str() {
            "slider" => SliderInfo::extract(expression, name, "slider").map(CellDisplay::Slider),
            "stepper" => SliderInfo::extract(expression, name, "stepper").map(CellDisplay::Stepper),
            "checkbox" => CheckboxInfo::extract(expression, name).map(CellDisplay::Checkbox),
            "dropdown" => DropdownInfo::extract(expression, name).map(CellDisplay::Dropdown),
            _ => None,
        }
    }

    /// Rewrites a control's STORAGE argument literal inside the raw cell
    /// text, leaving everything else — spacing, the 𝑖 name, trailing
    /// `# comments` — intact. `literal` is the replacement source text
    /// (`0.11`, `true`, `"US"`). Token-precise via the lexer's ranges.
    pub fn rewriting(raw: &str, literal: &str) -> Option<String> {
        let tokens = Lexer::tokenize(raw).ok()?;
        for (index, token) in tokens.iter().enumerate() {
            let TokenKind::Identifier(name) = &token.kind else {
                continue;
            };
            if !Self::NAMES.contains(&name.to_lowercase().as_str()) {
                continue;
            }
            if index + 2 >= tokens.len() || tokens[index + 1].kind != TokenKind::LeftParen {
                continue;
            }

            let mut start = index + 2;
            let range = match &tokens[start].kind {
                TokenKind::Number(_) | TokenKind::String(_) => tokens[start].range.clone(),
                TokenKind::Identifier(word)
                    if word.eq_ignore_ascii_case("true") || word.eq_ignore_ascii_case("false") =>
                {
                    tokens[start].range.clone()
                }
                TokenKind::Minus => {
                    start += 1;
                    if start >= tokens.len() || !matches!(tokens[start].kind, TokenKind::Number(_))
                    {
                        return None;
                    }
                    tokens[start - 1].range.start..tokens[start].range.end
                }
                _ => return None,
            };

            let mut characters: Vec<char> = raw.chars().collect();
            characters.splice(range, literal.chars());
            return Some(characters.into_iter().collect());
        }
        None
    }

    /// Numeric convenience kept for slider drags (the Swift `Slider` enum).
    pub fn rewriting_value(raw: &str, new_value: &BigDecimal) -> Option<String> {
        Self::rewriting(raw, &new_value.to_string())
    }
}
