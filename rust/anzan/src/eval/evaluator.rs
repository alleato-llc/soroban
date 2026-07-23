//! Walks the AST against an environment. Mutates the environment only via
//! assignment/definition expressions; `ans` updating is the Calculator
//! facade's job.
//!
//! Recursion is bounded by MEMORY and a sanity cap, never by whichever
//! thread happened to call evaluate(). TAIL calls loop at constant stack
//! (`apply_user` + `tail_step`); non-tail recursion grows the stack and,
//! when the current thread runs low, CONTINUES on a fresh 16 MB segment
//! (`stacker::maybe_grow` — the Rust analogue of the Swift side's
//! continueOnFreshStack thread hop; single-threaded discipline preserved).
//!
//! The evaluator's methods are split across sibling submodules by concern:
//! `values` (literals/arguments/reductions), `resolution` (name lookup +
//! namespaces), `calls` (function calls + construction), `operators`
//! (indexing + operator application), and `recursion` (tail-call loop).

use super::data_type::DataType;
use super::environment::{EvaluationEnvironment, UserFunction};
use super::money::Money;
use super::registry::FunctionRegistry;
use super::value::{FunctionValue, Value};
use crate::ast::{Expression, TypeAnnotation};
use crate::{BigDecimal, EngineError};
use helpers::binary_operator_named;
use num_bigint::BigInt;
use std::collections::HashMap;
use std::rc::Rc;

mod calls;
mod helpers;
mod operators;
mod recursion;
mod resolution;
mod values;

pub type Locals = HashMap<String, Value>;

/// The re-entry context evaluation-capable resolvers receive: cell reads
/// evaluate other formulas against the SAME environment, so the evaluator
/// and environment thread through the resolver as plain recursion (the Rust
/// answer to Swift's shared-class re-entrancy).
pub type Reentry<'a, 'b> = (&'a Evaluator<'a>, &'b mut EvaluationEnvironment);

/// Resolves `A:1`-style references (optionally sheet-qualified). Cells are
/// scalar: resolvers speak BigDecimal, the evaluator wraps.
pub type CellResolver =
    Box<dyn Fn(Reentry<'_, '_>, Option<&str>, &str, i64) -> Result<BigDecimal, EngineError>>;
/// Expands `A:1..B:9` rectangles to their numeric values (empty/text cells
/// skipped, Excel-style).
pub type RangeResolver = Box<
    dyn Fn(
        Reentry<'_, '_>,
        Option<&str>,
        &str,
        i64,
        &str,
        i64,
    ) -> Result<Vec<BigDecimal>, EngineError>,
>;
/// Sheet-scoped λ definitions (cells like `tax(x) = …`), resolved against
/// the owning sheet. Scoped names shadow log globals; locals shadow all.
pub type ScopedFunctionResolver = Box<dyn Fn(&str) -> Option<UserFunction>>;
/// Sheet-scoped 𝑖 definitions (`rate = 0.08` cells) — evaluated lazily, so
/// resolution re-enters evaluation.
pub type ScopedVariableResolver =
    Box<dyn Fn(Reentry<'_, '_>, &str) -> Result<Option<Value>, EngineError>>;
/// `'Projected Rate'` named-cell references (optionally sheet-qualified).
pub type NameResolver =
    Box<dyn Fn(Reentry<'_, '_>, Option<&str>, &str) -> Result<BigDecimal, EngineError>>;
/// Sheet-scoped `data` declarations (𝑫 cells).
pub type ScopedDataTypeResolver = Box<dyn Fn(&str) -> Option<DataType>>;
/// A bare name → a HOST value (`Workbook`, `History`). The bool is
/// `allow_mutation` (the log path) — lets the host scope a name to the log.
pub type HostValueResolver = Box<dyn Fn(&str, bool) -> Option<Value>>;
/// A free call → a HOST function (`cell(col, row)`), `None` when the name
/// isn't a reflection function.
pub type HostFunctionResolver =
    Box<dyn Fn(Reentry<'_, '_>, &str, &[Value]) -> Result<Option<Value>, EngineError>>;
/// A free call → a HOST mutation (`updateCell`, …); the bool is
/// `allow_mutation` — the host rejects mutations during cell recalc.
pub type HostMutationResolver =
    Box<dyn Fn(Reentry<'_, '_>, &str, &[Value], bool) -> Result<Option<Value>, EngineError>>;

#[derive(Default)]
pub struct Resolvers {
    pub cell: Option<CellResolver>,
    pub range: Option<RangeResolver>,
    pub scoped_function: Option<ScopedFunctionResolver>,
    pub scoped_variable: Option<ScopedVariableResolver>,
    pub name: Option<NameResolver>,
    pub scoped_data_type: Option<ScopedDataTypeResolver>,
    pub host_value: Option<HostValueResolver>,
    pub host_function: Option<HostFunctionResolver>,
    pub host_mutation: Option<HostMutationResolver>,
}

pub struct Evaluator<'a> {
    pub registry: &'static FunctionRegistry,
    pub resolvers: &'a Resolvers,
    /// True only on the log path — workbook mutations are allowed here and
    /// rejected (by the host resolver) during cell recalc.
    pub allow_mutation: bool,
}

impl Evaluator<'_> {
    pub fn evaluate(
        &self,
        expression: &Expression,
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<Value, EngineError> {
        match expression {
            Expression::Number(value) => Ok(Value::Number(value.clone())),

            Expression::Money { value, currency } => {
                Ok(Value::Money(Money::new(value.clone(), *currency)))
            }

            Expression::Grouped(value) => Ok(Value::Grouped(value.clone())),

            Expression::StringLiteral(text) => Ok(Value::String(text.clone())),

            Expression::ArrayLiteral(items) => self.array_value(items, environment, locals, depth),

            Expression::MapLiteral(entries) => self.map_value(entries, environment, locals, depth),

            Expression::Index { base, index } => {
                let base = self.evaluate(base, environment, locals, depth)?;
                let index = self.evaluate(index, environment, locals, depth)?;
                self.subscript_value(environment, &base, &index)
            }

            Expression::Member { base, name } => {
                let base = self.evaluate(base, environment, locals, depth)?;
                match &base {
                    Value::Map(_) => base
                        .map_value(name)
                        .cloned()
                        .ok_or_else(|| EngineError::domain(format!("no key '{name}' in map"))),
                    Value::Record(record) => base.map_value(name).cloned().ok_or_else(|| {
                        EngineError::domain(format!(
                            "{} has no field '{name}' — it has {}",
                            record.type_name,
                            record
                                .entries
                                .iter()
                                .map(|e| e.key.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    }),
                    // Host handles expose their own members (.name, .worksheets).
                    Value::Host(object) => {
                        let object = Rc::clone(object);
                        object.member((self, environment), name).ok_or_else(|| {
                            EngineError::domain(format!(
                                "{} has no member '.{name}'",
                                object.type_name()
                            ))
                        })
                    }
                    _ => Err(EngineError::domain(format!(
                        ".{name} needs a map or data value, got {}",
                        base.kind_name()
                    ))),
                }
            }

            Expression::MethodCall {
                base,
                name,
                arguments,
            } => {
                let base = self.evaluate(base, environment, locals, depth)?;
                let Value::Host(object) = &base else {
                    return Err(EngineError::domain(format!(
                        ".{name}(…) needs a host value, got {}",
                        base.kind_name()
                    )));
                };
                let object = object.clone();
                let arguments = self.arguments(arguments, environment, locals, depth)?;
                object.call((self, environment), name, &arguments)
            }

            Expression::Variable(name) => self.variable(name, environment, locals),

            Expression::Lambda { parameters, body } => {
                // Closure-by-value: whatever locals are visible now ride along.
                Ok(Value::Function(FunctionValue::lambda_with_captures(
                    parameters.clone(),
                    body.as_ref().clone(),
                    locals.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                )))
            }

            Expression::CellReference { sheet, column, row } => {
                let Some(resolve_cell) = &self.resolvers.cell else {
                    return Err(EngineError::domain(format!(
                        "no sheet available for {column}:{row}"
                    )));
                };
                Ok(Value::Number(resolve_cell(
                    (self, environment),
                    sheet.as_deref(),
                    column,
                    *row,
                )?))
            }

            Expression::NameReference { sheet, name } => {
                let Some(resolve_name) = &self.resolvers.name else {
                    return Err(EngineError::domain(format!(
                        "no sheet available for '{name}'"
                    )));
                };
                Ok(Value::Number(resolve_name(
                    (self, environment),
                    sheet.as_deref(),
                    name,
                )?))
            }

            Expression::CellRange {
                from_column,
                from_row,
                ..
            } => {
                // Ranges only mean something as a list of arguments.
                Err(EngineError::domain(format!(
                    "ranges like {from_column}:{from_row}..… can only be used inside functions, e.g. sum(A:1..A:9)"
                )))
            }

            Expression::UnaryMinus(inner) => {
                let value = self.evaluate(inner, environment, locals, depth)?;
                negate(value)
            }

            Expression::Percent(inner) => {
                let value = self.evaluate(inner, environment, locals, depth)?;
                percent(value)
            }

            Expression::Degrees(inner) => {
                let value = self.evaluate(inner, environment, locals, depth)?;
                degrees(value)
            }

            Expression::Binary(op, lhs_expr, rhs_expr) => {
                let lhs = self.evaluate(lhs_expr, environment, locals, depth)?;
                let rhs = self.evaluate(rhs_expr, environment, locals, depth)?;
                // Operator overloading: when a record is involved, a
                // user-defined operator whose typed operands match wins;
                // otherwise the built-in (so plain numeric/string math is
                // untouched and pays no lookup).
                if lhs.is_record() || rhs.is_record() {
                    if let Some(overload) = Self::operator_overload(*op, &lhs, &rhs, environment)? {
                        return self.apply_user(
                            overload,
                            vec![lhs, rhs],
                            Locals::new(),
                            environment,
                            depth,
                        );
                    }
                }
                Self::apply_op(*op, &lhs, &rhs)
            }

            Expression::Call { name, arguments } => {
                let arguments = self.arguments(arguments, environment, locals, depth)?;
                self.call(name, arguments, environment, locals, depth)
            }

            Expression::Assignment { name, value } => {
                let value = self.evaluate(value, environment, locals, depth)?;
                environment.set(name, value.clone());
                Ok(value)
            }

            Expression::Comparison(op, lhs_expr, rhs_expr) => {
                let lhs = self.evaluate(lhs_expr, environment, locals, depth)?;
                let rhs = self.evaluate(rhs_expr, environment, locals, depth)?;
                Self::compare(*op, &lhs, &rhs)
            }

            Expression::Conditional {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.evaluate(condition, environment, locals, depth)?;
                // Truthiness: nonzero is true. Only the taken branch evaluates.
                let branch = if condition.as_number("the if() condition")?.is_zero() {
                    otherwise
                } else {
                    then
                };
                self.evaluate(branch, environment, locals, depth)
            }

            Expression::Reduction {
                operation,
                index,
                lower,
                upper,
                body,
            } => self.reduce(
                *operation,
                index,
                lower,
                upper,
                body,
                environment,
                locals,
                depth,
            ),

            Expression::HelpRequest { .. } => {
                // Calculator intercepts this in the log; reaching the
                // evaluator means a context with no documentation surface.
                Err(EngineError::domain(
                    "man works in the calculation log, not a cell",
                ))
            }

            Expression::FunctionDefinition {
                name,
                parameters,
                body,
            } => {
                if self.registry.contains(name) {
                    return Err(EngineError::domain(format!(
                        "'{name}' is a built-in function and can't be redefined"
                    )));
                }
                if environment.data_type(name).is_some() {
                    return Err(EngineError::domain(format!(
                        "'{name}' is a data type — its constructor can't be redefined"
                    )));
                }
                // Operator overloads (`+(a: Point, b: Point) = …`): exactly
                // two operands and at least one declared data type, so
                // built-in arithmetic on numbers/strings can never be
                // clobbered.
                if binary_operator_named(name).is_some() {
                    if parameters.len() != 2 {
                        return Err(EngineError::domain(
                            "an operator overload takes two operands — e.g. +(a: Point, b: Point) = …",
                        ));
                    }
                    let involves_data_type = parameters
                        .iter()
                        .any(|p| matches!(p.annotation, Some(TypeAnnotation::Named(_))));
                    if !involves_data_type {
                        return Err(EngineError::domain(format!(
                            "an operator overload must involve a data type — the built-in '{name}' on numbers/strings can't be redefined"
                        )));
                    }
                }
                environment.define_function(UserFunction::new(
                    name.clone(),
                    parameters.clone(),
                    body.as_ref().clone(),
                    String::new(),
                ));
                // The facade reports definitions via EvalOutcome; this value
                // is never displayed.
                Ok(Value::Number(BigDecimal::zero()))
            }

            Expression::DataDefinition { name, fields } => {
                // Constructors live in the call namespace — built-in and
                // function collisions are rejected both ways (redeclaring
                // your OWN type is allowed, like redefining your own
                // function).
                if self.registry.contains(name) {
                    return Err(EngineError::domain(format!(
                        "'{name}' is a built-in function and can't be a data type"
                    )));
                }
                if environment.function(name).is_some() {
                    return Err(EngineError::domain(format!(
                        "'{name}' is already a function — pick a different name"
                    )));
                }
                environment.define_data_type(DataType::new(
                    name.clone(),
                    fields.clone(),
                    String::new(),
                ));
                Ok(Value::Number(BigDecimal::zero()))
            }

            Expression::NamespaceDefinition { name, members } => {
                self.register_namespace(name, members, environment, depth, &HashMap::new())?;
                Ok(Value::Number(BigDecimal::zero()))
            }

            Expression::ImportDirective { name } => {
                // Already imported → idempotent no-op (before the conflict
                // check, which would otherwise flag the namespace's own
                // members).
                if environment
                    .imported_namespaces()
                    .iter()
                    .any(|i| i.eq_ignore_ascii_case(name))
                {
                    return Ok(Value::Number(BigDecimal::zero()));
                }
                // A builtin module (Finance, Stats, …) is already in the
                // global prelude — importing it is a harmless no-op.
                if self.registry.is_module(name) {
                    return Ok(Value::Number(BigDecimal::zero()));
                }
                let members = environment.member_names(name);
                if members.is_empty() {
                    return Err(EngineError::domain(format!(
                        "no namespace '{name}' to import"
                    )));
                }
                // Loud conflicts (docs/MODULES.md): an imported member must
                // not collide with a builtin, a global function/type/
                // variable, or another import. Qualify it instead.
                for member in &members {
                    if self.registry.contains(member)
                        || environment.function(member).is_some()
                        || environment.data_type(member).is_some()
                        || environment.get(member).is_some()
                        || environment.imported_name(member).is_some()
                    {
                        return Err(EngineError::domain(format!(
                            "importing {name} would shadow '{member}' — use {name}::{member} instead"
                        )));
                    }
                }
                environment.add_import(name);
                Ok(Value::Number(BigDecimal::zero()))
            }
        }
    }
}

/// Unary minus with the money/grouped tag preserved — `-$1,234.50` stays dollars and
/// `-138,561` stays grouped. Kept OUT of `evaluate` (`#[inline(never)]`) so its
/// locals don't inflate the recursive evaluator's stack frame (deep recursion
/// hops fixed 16 MB segments; a fatter frame overflows the red zone sooner).
#[inline(never)]
fn negate(value: Value) -> Result<Value, EngineError> {
    let negated = -&value.as_number("-")?;
    match value {
        Value::Money(m) => Ok(Value::Money(Money::new(negated, m.currency))),
        Value::Grouped(_) => Ok(Value::Grouped(negated)),
        _ => Ok(Value::Number(negated)),
    }
}

/// `x%` → `x × 0.01`, exact. A currency amount is refused (a percent scales a
/// plain number, so "$9 as a percent" is a category error — and since the
/// symbol never changes the number, `$9%` would otherwise be indistinguishable
/// from `9%`). Grouping is presentation and echoes through the scale. Kept out
/// of `evaluate` for the same stack-frame reason as `negate`.
#[inline(never)]
fn percent(value: Value) -> Result<Value, EngineError> {
    if let Value::Money(_) = &value {
        return Err(EngineError::domain(
            "can't apply % to a currency amount — % scales a plain number (e.g. $10 * 5%)",
        ));
    }
    let scaled = &value.as_number("%")? * &BigDecimal::new(BigInt::from(1), -2);
    match value {
        Value::Grouped(_) => Ok(Value::Grouped(scaled)),
        _ => Ok(Value::Number(scaled)),
    }
}

/// `90°` → 90 × π/180 — the multiply is exact against the 60-digit π
/// constant; the divide rounds to working precision (50 digits), so
/// 90° == pi / 2 holds exactly. Mode-agnostic, like the AST node. Kept out
/// of `evaluate` for the same stack-frame reason as `negate`.
#[inline(never)]
fn degrees(value: Value) -> Result<Value, EngineError> {
    let radians = (&value.as_number("°")? * &super::environment::constants::pi())
        .div(&BigDecimal::from_int(180))?;
    Ok(Value::Number(radians))
}

/// An exact integer (for indexes and bounds), or a domain error naming the
/// context.
pub(crate) fn require_int(value: &BigDecimal, what: &str) -> Result<i64, EngineError> {
    value
        .int_value()
        .ok_or_else(|| EngineError::domain(format!("{what} must be an integer")))
}
