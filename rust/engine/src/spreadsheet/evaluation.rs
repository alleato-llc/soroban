//! Cell evaluation: the dynamic half of classification, cycle-safe memoized
//! `display_value`, control rendering, and the numeric read paths that
//! referencing formulas and ranges use.

use super::{CellDisplay, Host, Spreadsheet};
use crate::cell::{Cell, Content, DefinitionKind};
use crate::cell_address::CellAddress;
use crate::controls::{Control, SliderInfo};
use anzan::ast::Expression;
use anzan::eval::registry::FunctionRegistry;
use anzan::{BigDecimal, EngineError, Evaluator, Locals, Value};
use std::rc::Rc;

impl Spreadsheet {
    // MARK: Evaluation

    /// A cell formula against the live environment — mutation always
    /// disabled (recalc must stay reproducible), `ans` untouched.
    pub(super) fn evaluate_formula(
        (evaluator, environment): Host<'_, '_>,
        expression: &Expression,
    ) -> Result<Value, EngineError> {
        // Definitions and session mutations belong to the log — the same
        // rejections the Calculator's formula path applies.
        if let Some(rejection) = anzan::Calculator::formula_rejection(expression) {
            return Err(rejection);
        }
        let formula_evaluator = Evaluator {
            registry: evaluator.registry,
            resolvers: evaluator.resolvers,
            allow_mutation: false,
        };
        formula_evaluator.evaluate(expression, environment, &Locals::new(), 0)
    }

    pub fn display_value(self: &Rc<Self>, host: Host<'_, '_>, address: CellAddress) -> CellDisplay {
        if let Some(cached) = self.cache.borrow().get(&address) {
            return cached.clone();
        }

        let key = self.key(address);
        if self.context.resolving.borrow().contains(&key) {
            // Don't cache: the "circular reference" report belongs to the
            // cell that closed the loop, not everything on the path.
            let qualified = match self.display_name() {
                Some(name) => format!("{name}!{address}"),
                None => format!("{address}"),
            };
            return CellDisplay::Error(format!("circular reference involving {qualified}"));
        }
        self.context.resolving.borrow_mut().insert(key);

        // While this cell evaluates, unqualified references belong to THIS
        // sheet (not whichever tab the user is looking at), and reads are
        // recorded as dependency edges pointing at this cell.
        self.context.push(self, key);
        let cell = self.cells.borrow().get(&address).cloned();
        let display = self.evaluate_cell(host, cell.as_ref(), address);
        self.context.pop();
        self.context.resolving.borrow_mut().remove(&key);

        self.cache.borrow_mut().insert(address, display.clone());
        display
    }

    /// The dynamic half of classification: static facts (markers, parse
    /// outcome) were settled in `Cell::new`; here the stored AST is
    /// evaluated against the current sheet + variables.
    fn evaluate_cell(
        self: &Rc<Self>,
        host: Host<'_, '_>,
        cell: Option<&Cell>,
        address: CellAddress,
    ) -> CellDisplay {
        let Some(cell) = cell else {
            return CellDisplay::Empty;
        };

        match &cell.content {
            Content::ExplicitText(text) | Content::PlainText(text) => {
                CellDisplay::Text(text.clone())
            }

            Content::Note(comment) => CellDisplay::Note(comment.clone()),

            Content::Definition(definition) => {
                // Built-in names stay protected, mirroring the log.
                // Functions and data type constructors share the call
                // namespace; variable definitions don't (a 𝑖 named `abs`
                // shadows nothing).
                match definition.kind {
                    DefinitionKind::Function { .. } | DefinitionKind::DataType { .. } => {
                        if FunctionRegistry::standard().contains(&definition.name) {
                            return CellDisplay::Error(format!(
                                "'{}' is a built-in function and can't be redefined",
                                definition.name
                            ));
                        }
                    }
                    DefinitionKind::Variable(_) => {}
                }
                // Only the canonical cell (first claim) renders the glyph.
                let canonical = {
                    let definitions = self.definitions.borrow();
                    definitions
                        .get(&definition.name.to_lowercase())
                        .map(|d| d.address)
                };
                if canonical != Some(address) {
                    let owner = canonical
                        .map(|a| format!("{a}"))
                        .unwrap_or_else(|| "another cell".to_string());
                    return CellDisplay::Error(format!(
                        "'{}' is already defined in {owner}",
                        definition.name
                    ));
                }
                match &definition.kind {
                    DefinitionKind::Function { parameters, .. } => CellDisplay::Definition(
                        format!("λ {}({})", definition.name, parameters.join(", ")),
                    ),
                    DefinitionKind::DataType { .. } => {
                        CellDisplay::Definition(format!("𝑫 {}", definition.name))
                    }
                    DefinitionKind::Variable(_) => {
                        // A 𝑖 whose body is a control expression draws the
                        // control (only after the duplicate/builtin checks —
                        // a shadowed slider must show its error, not a
                        // working knob).
                        if let Some(control) = Control::display(cell) {
                            return self.applying_override(control, address);
                        }
                        CellDisplay::Definition(format!("𝑖 {}", definition.name))
                    }
                }
            }

            Content::ExplicitFormula(Err(error)) => CellDisplay::Error(error.to_string()),

            Content::ExplicitFormula(Ok(expression)) => {
                // Anonymous =slider(…) etc.
                if let Some(control) = Control::display(cell) {
                    return self.applying_override(control, address);
                }
                match Self::evaluate_formula(host, expression) {
                    Ok(value) => Self::display_of(&value),
                    Err(error) => CellDisplay::Error(error.to_string()),
                }
            }

            Content::Candidate(expression) => {
                // Anonymous slider(…) etc.
                if let Some(control) = Control::display(cell) {
                    return self.applying_override(control, address);
                }
                match Self::evaluate_formula(host, expression) {
                    Ok(value) => Self::display_of(&value),
                    // Cell refs are always formulas.
                    Err(error) if expression.contains_cell_reference() => {
                        CellDisplay::Error(error.to_string())
                    }
                    // Unresolved names mean this is a label ("Q1 revenue"
                    // parses as Q1 * revenue).
                    Err(EngineError::UnknownVariable { .. })
                    | Err(EngineError::UnknownFunction { .. }) => {
                        CellDisplay::Text(cell.raw.clone())
                    }
                    // Anything else (division by zero, sqrt(-1), wrong
                    // arity, …) only happens to genuine formulas.
                    Err(error) => CellDisplay::Error(error.to_string()),
                }
            }
        }
    }

    /// Mid-drag, a slider's preview value replaces the stored literal
    /// (clamped). Other controls commit immediately — no preview state.
    fn applying_override(&self, control: CellDisplay, address: CellAddress) -> CellDisplay {
        let CellDisplay::Slider(info) = &control else {
            return control;
        };
        let Some(override_value) = self.slider_overrides.borrow().get(&address).cloned() else {
            return control;
        };
        CellDisplay::Slider(SliderInfo {
            name: info.name.clone(),
            value: override_value
                .max(info.minimum.clone())
                .min(info.maximum.clone()),
            minimum: info.minimum.clone(),
            maximum: info.maximum.clone(),
            step: info.step.clone(),
        })
    }

    /// Cells hold scalars: numbers display as values, string results render
    /// as text (so `="Q" + quarter` labels work — and behave like text when
    /// referenced: skipped in ranges, error on direct numeric use). Arrays
    /// and maps don't fit in a cell — aggregate them.
    fn display_of(value: &Value) -> CellDisplay {
        match value {
            Value::Number(number) => CellDisplay::Value(number.clone()),
            // Shows its numeric value.
            Value::FixedInt(f) => CellDisplay::Value(f.decimal()),
            // Value; CellFormat handles currency padding.
            Value::FixedDecimal(d) => CellDisplay::Value(d.value.clone()),
            Value::String(text) => CellDisplay::Text(text.clone()),
            Value::Array(_) | Value::Map(_) | Value::Record(_) => CellDisplay::Error(format!(
                "a cell can't hold {} — aggregate it (e.g. sum(…)) or reference a field",
                value.kind_name()
            )),
            Value::Function(_) => CellDisplay::Error(
                "a cell can't hold a function — call it (e.g. =f(A:1))".to_string(),
            ),
            Value::Host(_) => CellDisplay::Error(format!(
                "a cell can't hold {} — read a field from it (e.g. .value)",
                value.kind_name()
            )),
        }
    }

    /// Numeric value of a cell as seen from a referencing formula.
    /// Empty cells are 0 (spreadsheet convention); text and errors propagate.
    pub fn numeric_value(
        self: &Rc<Self>,
        host: Host<'_, '_>,
        column: &str,
        row: i64,
    ) -> Result<BigDecimal, EngineError> {
        let Some(address) = CellAddress::from_column_name(column, row) else {
            return Err(EngineError::domain(format!(
                "cell {column}:{row} is out of range"
            )));
        };
        self.context.record_cell_read(self.key(address));

        match self.display_value(host, address) {
            CellDisplay::Empty => Ok(BigDecimal::zero()),
            CellDisplay::Value(value) => Ok(value),
            // Controls read as their current value.
            CellDisplay::Slider(info) | CellDisplay::Stepper(info) => Ok(info.value),
            CellDisplay::Checkbox(info) => Ok(if info.is_on {
                BigDecimal::one()
            } else {
                BigDecimal::zero()
            }),
            CellDisplay::Dropdown(info) => match info.value {
                Value::Number(value) => Ok(value),
                // String options act like text.
                _ => Err(EngineError::domain(format!(
                    "cell {address} is not a number"
                ))),
            },
            CellDisplay::Text(_) | CellDisplay::Note(_) => Err(EngineError::domain(format!(
                "cell {address} is not a number"
            ))),
            CellDisplay::Definition(glyph) => Err(EngineError::domain(format!(
                "cell {address} is a definition ({glyph}) — use the name directly"
            ))),
            CellDisplay::Error(message) => Err(EngineError::domain(message)),
        }
    }

    /// Values in the rectangle spanned by two corners (any orientation),
    /// row-major. Excel semantics: empty and text cells are skipped — so
    /// avg/count over a sparse column do what you expect — while error cells
    /// propagate as errors.
    pub fn numeric_values(
        self: &Rc<Self>,
        host: Host<'_, '_>,
        from_column: &str,
        from_row: i64,
        to_column: &str,
        to_row: i64,
    ) -> Result<Vec<BigDecimal>, EngineError> {
        let (Some(from), Some(to)) = (
            CellAddress::from_column_name(from_column, from_row),
            CellAddress::from_column_name(to_column, to_row),
        ) else {
            return Err(EngineError::domain(format!(
                "range {from_column}:{from_row}..{to_column}:{to_row} is out of bounds"
            )));
        };

        let rows = from.row.min(to.row)..=from.row.max(to.row);
        let columns = from.column.min(to.column)..=from.column.max(to.column);
        self.context
            .record_range_read(self.id, rows.clone(), columns.clone());

        let mut values: Vec<BigDecimal> = Vec::new();
        let (evaluator, environment) = host;
        for row in rows {
            for column in columns.clone() {
                let address = CellAddress::new(column, row);
                match self.display_value((evaluator, &mut *environment), address) {
                    CellDisplay::Value(value) => values.push(value),
                    CellDisplay::Slider(info) | CellDisplay::Stepper(info) => {
                        values.push(info.value)
                    }
                    CellDisplay::Checkbox(info) => values.push(if info.is_on {
                        BigDecimal::one()
                    } else {
                        BigDecimal::zero()
                    }),
                    CellDisplay::Dropdown(info) => {
                        // String selections skip like text.
                        if let Value::Number(value) = info.value {
                            values.push(value);
                        }
                    }
                    // Notes skip like text.
                    CellDisplay::Empty
                    | CellDisplay::Text(_)
                    | CellDisplay::Definition(_)
                    | CellDisplay::Note(_) => continue,
                    CellDisplay::Error(message) => {
                        return Err(EngineError::domain(message));
                    }
                }
            }
        }
        Ok(values)
    }
}
