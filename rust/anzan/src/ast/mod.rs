//! Expression tree produced by the parser.

mod source;

pub use source::{key_literal, quoted};

use crate::eval::currency::Currency;
use crate::eval::data_type::DataField;
use crate::BigDecimal;

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(BigDecimal),
    /// A finance-mode currency literal — `$10`, `€10`, `$10,000`. The currency
    /// propagates through arithmetic (see `Money`), so it is part of the value.
    Money {
        value: BigDecimal,
        currency: Currency,
    },
    /// A finance-mode grouped plain number — `138,561`. Presentation only; the
    /// grouping echoes through a calculation (see `Grouped`).
    Grouped(BigDecimal),
    Variable(String),
    /// `A:1` or `Budget!A:1` / `'Q1 Budget'!A:1` — `None` sheet means the
    /// sheet that owns the formula (or the active sheet, from the log).
    CellReference {
        sheet: Option<String>,
        column: String,
        row: i64,
    },
    /// A:1..B:9 — expands to the rectangle's numeric values; valid only as a
    /// function argument (sum(A:1..A:9)). May be sheet-qualified.
    CellRange {
        sheet: Option<String>,
        from_column: String,
        from_row: i64,
        to_column: String,
        to_row: i64,
    },
    UnaryMinus(Box<Expression>),
    /// `3%` — postfix percent: the operand divided by 100 (3% → 0.03), exact.
    /// Binds tighter than `^` (like indexing); modulo is the `mod(x, y)`
    /// function.
    Percent(Box<Expression>),
    Binary(BinaryOperator, Box<Expression>, Box<Expression>),
    Call {
        name: String,
        arguments: Vec<Expression>,
    },
    Assignment {
        name: String,
        value: Box<Expression>,
    },
    /// `f(x) = …` / `dist(p: Point) = …` / `+(a: Point, b: Point) = …`.
    /// Parameters may carry a type annotation; the same `name` can have
    /// several definitions distinguished by their annotations (typed
    /// dispatch). The `name` may be an operator symbol (`+`), which overloads
    /// that operator.
    FunctionDefinition {
        name: String,
        parameters: Vec<Parameter>,
        body: Box<Expression>,
    },
    /// ∑_i=1^10(term) / ∏_i=1^5(term) — binding forms: `term` is re-evaluated
    /// with `index` bound to each integer in lower...upper, accumulated by
    /// the operation.
    Reduction {
        operation: ReductionOperation,
        index: String,
        lower: Box<Expression>,
        upper: Box<Expression>,
        body: Box<Expression>,
    },
    /// `a < b` etc. — evaluates to 1 (true) or 0 (false).
    Comparison(ComparisonOperator, Box<Expression>, Box<Expression>),
    /// `if(cond, then, else)` — a special form: only the taken branch is
    /// evaluated, so the other may divide by zero or recurse.
    Conditional {
        condition: Box<Expression>,
        then: Box<Expression>,
        otherwise: Box<Expression>,
    },
    /// `man pmt` / `manual pmt` / `help pmt` — prints documentation; the
    /// argument is a NAME, never evaluated, space-separated (no parentheses).
    HelpRequest {
        name: String,
    },
    /// `"…"` — a string value.
    StringLiteral(String),
    /// `[1, 2, 3]` — elements are full expressions; nests freely.
    ArrayLiteral(Vec<Expression>),
    /// `{name: "Ada", age: 36}` — keys unique and case-sensitive.
    MapLiteral(Vec<MapLiteralEntry>),
    /// `arr[0]` / `m["key"]` — 0-based for arrays and strings; string keys
    /// for maps.
    Index {
        base: Box<Expression>,
        index: Box<Expression>,
    },
    /// `m.name` — map member access with a literal key.
    Member {
        base: Box<Expression>,
        name: String,
    },
    /// `worksheet.cell("A", 2)` — a method call on a host value. Distinct
    /// from member access (no parens) and from a free `name(args)` call.
    MethodCall {
        base: Box<Expression>,
        name: String,
        arguments: Vec<Expression>,
    },
    /// `x -> x * 2` / `(a, b) -> a + b` — an anonymous function value.
    /// Locals in scope at evaluation are captured by value (closure).
    Lambda {
        parameters: Vec<String>,
        body: Box<Expression>,
    },
    /// `'Projected Rate'` / `Budget!'Projected Rate'` — a NAMED CELL
    /// reference. Single quotes are Soroban's name-of-a-thing syntax (sheets
    /// already use them); `None` sheet = the owning sheet (active, from the
    /// log).
    NameReference {
        sheet: Option<String>,
        name: String,
    },
    /// `data Person { name: String, age: Number, active: Boolean }` —
    /// declares a record type whose name becomes its constructor.
    DataDefinition {
        name: String,
        fields: Vec<DataField>,
    },
    /// `namespace Bits { data BitField { … }  data BitFormat { … } }` —
    /// groups declarations under a name; members are reached as
    /// `Bits::BitField` (docs/MODULES.md).
    NamespaceDefinition {
        name: String,
        members: Vec<Expression>,
    },
    /// `import Bits` — brings a namespace's members into scope unqualified
    /// (docs/MODULES.md 2b).
    ImportDirective {
        name: String,
    },
}

/// A function parameter: a name and an optional type annotation. An
/// un-annotated parameter (`type == None`) matches an argument of any type;
/// an annotated one participates in typed dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub annotation: Option<TypeAnnotation>,
}

impl Parameter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            annotation: None,
        }
    }

    pub fn typed(name: impl Into<String>, annotation: TypeAnnotation) -> Self {
        Self {
            name: name.into(),
            annotation: Some(annotation),
        }
    }

    /// Source/display form: `p: Point` when typed, else `p`.
    pub fn rendered(&self) -> String {
        match &self.annotation {
            Some(t) => format!("{}: {}", self.name, t.label()),
            None => self.name.clone(),
        }
    }
}

/// A parameter's declared type: a built-in scalar or a named `data` type.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Number,
    String,
    Boolean,
    /// A declared data type, e.g. Point.
    Named(String),
}

impl TypeAnnotation {
    /// Maps a written type name to an annotation. The three scalars match
    /// case-insensitively (like `data` field types); anything else is a named
    /// data type, spelling preserved (existence checked at dispatch time).
    pub fn parsing(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "number" => Self::Number,
            "string" => Self::String,
            "boolean" => Self::Boolean,
            _ => Self::Named(name.to_string()),
        }
    }

    /// As written in source — `Number` / `Point`.
    pub fn label(&self) -> String {
        match self {
            Self::Number => "Number".to_string(),
            Self::String => "String".to_string(),
            Self::Boolean => "Boolean".to_string(),
            Self::Named(name) => name.clone(),
        }
    }

    /// Within a namespace, qualify a type annotation (`p: Point` →
    /// `p: Bits::Point`) so typed dispatch matches the qualified instances.
    /// `scope` maps a simple type name (lowercased) to its qualified form,
    /// accumulated from the enclosing namespaces. An already-qualified or
    /// out-of-scope name is left alone.
    pub(crate) fn qualified(
        &self,
        scope: &std::collections::HashMap<String, String>,
    ) -> TypeAnnotation {
        if let Self::Named(name) = self {
            if !name.contains("::") {
                if let Some(qualified) = scope.get(&name.to_lowercase()) {
                    return Self::Named(qualified.clone());
                }
            }
        }
        self.clone()
    }
}

/// One `key: value` pair of a map literal.
#[derive(Debug, Clone, PartialEq)]
pub struct MapLiteralEntry {
    pub key: String,
    pub value: Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
    Equal,
    NotEqual,
}

impl ComparisonOperator {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Less => "<",
            Self::Greater => ">",
            Self::LessOrEqual => "<=",
            Self::GreaterOrEqual => ">=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReductionOperation {
    /// ∑ — starts at 0, accumulates with +.
    Sum,
    /// ∏ — starts at 1, accumulates with ×.
    Product,
}

impl ReductionOperation {
    pub(crate) fn symbol(&self) -> &'static str {
        match self {
            Self::Sum => "∑",
            Self::Product => "∏",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

impl BinaryOperator {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Modulo => "%",
            Self::Power => "^",
        }
    }
}

impl Expression {
    /// True if any node references a spreadsheet cell — used by the grid's
    /// formula auto-detection.
    pub fn contains_cell_reference(&self) -> bool {
        match self {
            Self::CellReference { .. } | Self::CellRange { .. } => true,
            Self::Number(_) | Self::Money { .. } | Self::Grouped(_) | Self::Variable(_) => false,
            Self::UnaryMinus(inner) | Self::Percent(inner) => inner.contains_cell_reference(),
            Self::Binary(_, lhs, rhs) => {
                lhs.contains_cell_reference() || rhs.contains_cell_reference()
            }
            Self::Call { arguments, .. } => {
                arguments.iter().any(Expression::contains_cell_reference)
            }
            Self::Assignment { value, .. } => value.contains_cell_reference(),
            Self::FunctionDefinition { body, .. } => body.contains_cell_reference(),
            Self::Reduction {
                lower, upper, body, ..
            } => {
                lower.contains_cell_reference()
                    || upper.contains_cell_reference()
                    || body.contains_cell_reference()
            }
            Self::Comparison(_, lhs, rhs) => {
                lhs.contains_cell_reference() || rhs.contains_cell_reference()
            }
            Self::Conditional {
                condition,
                then,
                otherwise,
            } => {
                condition.contains_cell_reference()
                    || then.contains_cell_reference()
                    || otherwise.contains_cell_reference()
            }
            Self::HelpRequest { .. } | Self::StringLiteral(_) => false,
            Self::ArrayLiteral(items) => items.iter().any(Expression::contains_cell_reference),
            Self::MapLiteral(entries) => entries.iter().any(|e| e.value.contains_cell_reference()),
            Self::Index { base, index } => {
                base.contains_cell_reference() || index.contains_cell_reference()
            }
            Self::Member { base, .. } => base.contains_cell_reference(),
            Self::MethodCall {
                base, arguments, ..
            } => {
                base.contains_cell_reference()
                    || arguments.iter().any(Expression::contains_cell_reference)
            }
            Self::Lambda { body, .. } => body.contains_cell_reference(),
            // A named cell IS a cell reference.
            Self::NameReference { .. } => true,
            Self::DataDefinition { .. }
            | Self::NamespaceDefinition { .. }
            | Self::ImportDirective { .. } => false,
        }
    }
}
