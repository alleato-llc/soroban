//! One cell's content, parsed and statically classified exactly once at
//! commit time. The *dynamic* half of classification — what a formula
//! evaluates to, and whether an ambiguous candidate is a formula or a label
//! — happens per recalculation in `Spreadsheet`, because it depends on the
//! current sheet and variable environment (`12 * rte` is a label until the
//! log defines `rte`).

use anzan::ast::Expression;
use anzan::eval::data_type::DataField;
use anzan::{Calculator, EngineError, LanguageMode, Parser};

#[derive(Debug, Clone)]
pub struct Cell {
    /// Exactly what the user typed (markers included) — what editing shows
    /// and what persistence stores.
    pub raw: String,
    pub(crate) content: Content,
}

#[derive(Debug, Clone)]
pub(crate) enum Content {
    /// `=…` — always a formula; carries the parse outcome so even a
    /// malformed explicit formula renders as an error, not text.
    ExplicitFormula(Result<Expression, EngineError>),
    /// `"…"` — always text, quotes stripped.
    ExplicitText(String),
    /// Doesn't parse — always text.
    PlainText(String),
    /// Parses without an explicit marker; formula vs label is decided at
    /// evaluation time by the auto-detect rules.
    Candidate(Expression),
    /// `tax(x) = x * 2` / `rate = 0.0825` / `data Pt { x: Number, … }`,
    /// typed plain — a SHEET-SCOPED definition. The cell renders λ/𝑖/𝑫; the
    /// name resolves from formulas on the owning sheet and is immutable from
    /// the log.
    Definition(Definition),
    /// `# a note` — a comment-only cell: a free-floating annotation that
    /// holds no value (skipped in ranges, errors on direct reference, like
    /// text). The string is the comment without its `#`.
    Note(String),
}

/// What a definition cell defines.
#[derive(Debug, Clone)]
pub struct Definition {
    /// As typed; lookup is case-insensitive (one namespace per sheet).
    pub name: String,
    pub kind: DefinitionKind,
    /// The original line — keeps a function's trailing `# doc comment`.
    pub source: String,
}

#[derive(Debug, Clone)]
pub enum DefinitionKind {
    Function {
        parameters: Vec<String>,
        body: Expression,
    },
    Variable(Expression),
    DataType {
        fields: Vec<DataField>,
    },
}

impl Cell {
    /// `None` for blank input — the cell should be removed, not stored.
    pub fn new(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let raw = trimmed.to_string();

        if let Some(rest) = trimmed.strip_prefix('=') {
            let formula = rest.trim();
            let content = if formula.is_empty() {
                Content::ExplicitFormula(Err(EngineError::domain("empty formula")))
            } else {
                Content::ExplicitFormula(Parser::parse(formula, LanguageMode::Normal))
            };
            return Some(Self { raw, content });
        }

        if let Some(rest) = trimmed.strip_prefix('"') {
            let text = rest.strip_suffix('"').unwrap_or(rest);
            return Some(Self {
                raw,
                content: Content::ExplicitText(text.to_string()),
            });
        }

        // A comment-only cell (`# a note`) is an annotation, not a value.
        if let Some(comment) = Calculator::standalone_comment(trimmed) {
            return Some(Self {
                raw,
                content: Content::Note(comment),
            });
        }

        let content = match Parser::parse(trimmed, LanguageMode::Normal) {
            Ok(Expression::FunctionDefinition {
                name,
                parameters,
                body,
            }) => {
                // λ cells store untyped parameter names for now; typed
                // dispatch applies to log functions.
                Content::Definition(Definition {
                    name,
                    kind: DefinitionKind::Function {
                        parameters: parameters.into_iter().map(|p| p.name).collect(),
                        body: *body,
                    },
                    source: trimmed.to_string(),
                })
            }
            Ok(Expression::Assignment { name, value }) => Content::Definition(Definition {
                name,
                kind: DefinitionKind::Variable(*value),
                source: trimmed.to_string(),
            }),
            Ok(Expression::DataDefinition { name, fields }) => Content::Definition(Definition {
                name,
                kind: DefinitionKind::DataType { fields },
                source: trimmed.to_string(),
            }),
            Ok(expression) => Content::Candidate(expression),
            Err(_) => Content::PlainText(trimmed.to_string()),
        };
        Some(Self { raw, content })
    }

    pub(crate) fn is_definition(&self) -> bool {
        matches!(self.content, Content::Definition(_))
    }
}
