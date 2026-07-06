//! Building block evaluations: array/map literals, call-argument collection
//! (where ranges expand in place), and the ∑/∏ indexed reductions.

use super::{require_int, Evaluator, Locals};
use crate::ast::{Expression, MapLiteralEntry, ReductionOperation};
use crate::eval::environment::EvaluationEnvironment;
use crate::eval::value::{MapEntry, Value};
use crate::{BigDecimal, EngineError};

impl Evaluator<'_> {
    pub(super) fn array_value(
        &self,
        items: &[Expression],
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<Value, EngineError> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.evaluate(item, environment, locals, depth)?);
        }
        Ok(Value::Array(values))
    }

    pub(super) fn map_value(
        &self,
        entries: &[MapLiteralEntry],
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<Value, EngineError> {
        let mut values = Vec::with_capacity(entries.len());
        for entry in entries {
            values.push(MapEntry::new(
                entry.key.clone(),
                self.evaluate(&entry.value, environment, locals, depth)?,
            ));
        }
        Ok(Value::Map(values))
    }

    /// Evaluates a call's arguments; ranges expand in place, so
    /// `sum(A:1..A:9, 10)` sees ≤10 numbers.
    pub(super) fn arguments(
        &self,
        expressions: &[Expression],
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<Vec<Value>, EngineError> {
        let mut arguments = Vec::with_capacity(expressions.len());
        for expr in expressions {
            if let Expression::CellRange {
                sheet,
                from_column,
                from_row,
                to_column,
                to_row,
            } = expr
            {
                let Some(resolve_range) = &self.resolvers.range else {
                    return Err(EngineError::domain("no sheet available for ranges"));
                };
                arguments.extend(
                    resolve_range(
                        (self, environment),
                        sheet.as_deref(),
                        from_column,
                        *from_row,
                        to_column,
                        *to_row,
                    )?
                    .into_iter()
                    .map(Value::Number),
                );
            } else {
                arguments.push(self.evaluate(expr, environment, locals, depth)?);
            }
        }
        Ok(arguments)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reduce(
        &self,
        operation: ReductionOperation,
        index: &str,
        lower_expr: &Expression,
        upper_expr: &Expression,
        body_expr: &Expression,
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<Value, EngineError> {
        let symbol = operation.symbol();
        let lower = require_int(
            &self
                .evaluate(lower_expr, environment, locals, depth)?
                .as_number(&format!("the {symbol} lower bound"))?,
            &format!("{symbol} lower bound"),
        )?;
        let upper = require_int(
            &self
                .evaluate(upper_expr, environment, locals, depth)?
                .as_number(&format!("the {symbol} upper bound"))?,
            &format!("{symbol} upper bound"),
        )?;
        // Empty range, by convention: ∑ → 0 (additive identity), ∏ → 1
        // (multiplicative identity).
        let identity = match operation {
            ReductionOperation::Sum => BigDecimal::zero(),
            ReductionOperation::Product => BigDecimal::one(),
        };
        if lower > upper {
            return Ok(Value::Number(identity));
        }

        let (span, overflow) = upper.overflowing_sub(lower);
        if overflow || span >= 100_000 {
            return Err(EngineError::domain(format!(
                "{symbol} spans more than 100,000 terms"
            )));
        }

        let mut total = identity;
        let mut iteration_locals = locals.clone();
        for i in lower..=upper {
            // Shadows globals, like a parameter.
            iteration_locals.insert(index.to_string(), Value::Number(BigDecimal::from_int(i)));
            let term = self
                .evaluate(body_expr, environment, &iteration_locals, depth)?
                .as_number(&format!("the {symbol} term"))?;
            total = match operation {
                ReductionOperation::Sum => &total + &term,
                ReductionOperation::Product => &total * &term,
            };
        }
        Ok(Value::Number(total))
    }
}
