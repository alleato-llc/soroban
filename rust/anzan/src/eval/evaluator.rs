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

use super::data_type::{DataField, DataFieldType, DataType};
use super::environment::{EvaluationEnvironment, UserFunction};
use super::fixed_decimal::FixedDecimal;
use super::fixed_int::FixedInt;
use super::registry::FunctionRegistry;
use super::value::{FunctionValue, FunctionValueKind, MapEntry, RecordValue, Value};
use crate::ast::{
    BinaryOperator, ComparisonOperator, Expression, MapLiteralEntry, Parameter, ReductionOperation,
    TypeAnnotation,
};
use crate::{BigDecimal, EngineError};
use num_bigint::BigInt;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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

const STACK_HEADROOM: usize = 128 * 1024;
const SEGMENT_STACK_SIZE: usize = 16 << 20; // 16 MB per segment
/// Sanity cap: a missing base case errors (with a hint) instead of chewing
/// memory forever. ~10k frames is far beyond honest recursion.
const MAX_CALL_DEPTH: usize = 10_000;
/// Tail-call iteration cap: a tail loop uses CONSTANT stack, so without
/// this a base-case-less TAIL recursion would spin forever.
const MAX_TAIL_ITERATIONS: usize = 1_000_000;

/// One step of tail-aware body evaluation: either a finished value, or "now
/// call THIS function with THESE arguments" — which `apply_user` turns into
/// a loop iteration instead of a stack frame.
enum TailStep {
    Value(Value),
    Call(UserFunction, Vec<Value>, Locals),
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
                Ok(Value::Number(-&value.as_number("-")?))
            }

            Expression::Percent(inner) => {
                // `3%` → 3 × 0.01, exact (× never rounds). Numeric only,
                // like unary −.
                let value = self.evaluate(inner, environment, locals, depth)?;
                Ok(Value::Number(
                    &value.as_number("%")? * &BigDecimal::new(BigInt::from(1), -2),
                ))
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

    /// The `.variable` case — extracted (the Swift side keeps the big switch
    /// slim for frame size; here it keeps the match readable).
    fn variable(
        &self,
        name: &str,
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
    ) -> Result<Value, EngineError> {
        // Parameters shadow sheet definitions, which shadow log globals.
        if let Some(value) = locals.get(name) {
            return Ok(value.clone());
        }
        // Inside a namespaced member, an unqualified name resolves a sibling
        // first (a sibling constant, function, or type as a value), walking
        // up the nesting chain, before sheet scope and globals.
        let current_namespace = environment.current_namespace().map(str::to_string);
        for qualified in sibling_candidates(name, current_namespace.as_deref()) {
            if let Some(value) = environment.get(&qualified) {
                return Ok(value);
            }
            if environment.function(&qualified).is_some()
                || environment.data_type(&qualified).is_some()
            {
                return Ok(Value::Function(FunctionValue::user(qualified)));
            }
        }
        if let Some(resolve) = &self.resolvers.scoped_variable {
            if let Some(scoped) = resolve((self, environment), name)? {
                return Ok(scoped);
            }
        }
        if let Some(value) = environment.get(name) {
            return Ok(value);
        }
        // The host reflection handle (`Workbook`, `History`) — after user
        // variables, so a user can still bind the name, before the function
        // fallbacks. `History` resolves only on the log path; in a cell it
        // returns None here and falls through to the text-label rule.
        if let Some(resolve) = &self.resolvers.host_value {
            if let Some(host) = resolve(name, self.allow_mutation) {
                return Ok(host);
            }
        }
        // A bare function name is a function VALUE — `map(double, arr)`.
        if let Some(resolve) = &self.resolvers.scoped_function {
            if let Some(scoped) = resolve(name) {
                // Cell-defined: carried structurally (it lives in a cell,
                // not the environment, so a name can't re-resolve it).
                return Ok(Value::Function(FunctionValue::lambda(
                    scoped.parameters.iter().map(|p| p.name.clone()).collect(),
                    scoped.body,
                )));
            }
        }
        if environment.function(name).is_some() {
            return Ok(Value::Function(FunctionValue::user(name.to_string())));
        }
        // A bare data type name is its constructor as a value —
        // `map(Person, listOfMaps)`. Carried by name; apply re-resolves.
        let scoped_type = self
            .resolvers
            .scoped_data_type
            .as_ref()
            .and_then(|resolve| resolve(name));
        if scoped_type.is_some() || environment.data_type(name).is_some() {
            return Ok(Value::Function(FunctionValue::user(name.to_string())));
        }
        if self.registry.contains(name) {
            return Ok(Value::Function(FunctionValue::builtin(name.to_string())));
        }
        // An imported namespace's member as a bare name. A constant resolves
        // to its value; a function/type to a function value. Last fallback;
        // re-resolves by qualified name.
        if let Some(qualified) = environment.imported_name(name) {
            if let Some(value) = environment.get(&qualified) {
                return Ok(value);
            }
            return Ok(Value::Function(FunctionValue::user(qualified)));
        }
        // A qualified builtin as a value — `map(Finance::pmt, …)`.
        if let Some(bare) = self.registry.resolve_qualified(name) {
            return Ok(Value::Function(FunctionValue::builtin(bare)));
        }
        Err(EngineError::UnknownVariable {
            name: name.to_string(),
        })
    }

    /// Registers a namespace's members under `prefix::`, recursing into
    /// nested namespaces (`A::B::member`). A data field or function
    /// parameter type that references a sibling TYPE is qualified to the
    /// prefix; a function body resolves its siblings unqualified at call
    /// time via the home-namespace context; a constant evaluates EAGERLY
    /// under that context. (docs/MODULES.md)
    fn register_namespace(
        &self,
        prefix: &str,
        members: &[Expression],
        environment: &mut EvaluationEnvironment,
        depth: usize,
        enclosing: &HashMap<String, String>,
    ) -> Result<(), EngineError> {
        // This level's type names, mapped to their qualified form, ON TOP of
        // the enclosing scope — so a member may name a sibling OR a parent's
        // type unqualified; nesting shadows the parent.
        let mut scope = enclosing.clone();
        for member in members {
            match member {
                Expression::DataDefinition { name, .. } => {
                    scope.insert(name.to_lowercase(), format!("{prefix}::{name}"));
                }
                Expression::FunctionDefinition { .. }
                | Expression::Assignment { .. }
                | Expression::NamespaceDefinition { .. } => {}
                _ => {
                    return Err(EngineError::domain(format!(
                        "namespace {prefix} holds data, function, constant, and nested namespace declarations"
                    )));
                }
            }
        }
        for member in members {
            match member {
                Expression::DataDefinition { name, fields } => {
                    let qualified = format!("{prefix}::{name}");
                    if environment.function(&qualified).is_some() {
                        return Err(EngineError::domain(format!(
                            "'{qualified}' is already a function"
                        )));
                    }
                    let qualified_fields: Vec<DataField> = fields
                        .iter()
                        .map(|f| DataField::new(f.name.clone(), f.field_type.qualified(&scope)))
                        .collect();
                    environment.define_data_type(DataType::new(
                        qualified,
                        qualified_fields,
                        String::new(),
                    ));
                }
                Expression::FunctionDefinition {
                    name,
                    parameters,
                    body,
                } => {
                    let qualified = format!("{prefix}::{name}");
                    if environment.data_type(&qualified).is_some() {
                        return Err(EngineError::domain(format!(
                            "'{qualified}' is already a data type"
                        )));
                    }
                    let qualified_params: Vec<Parameter> = parameters
                        .iter()
                        .map(|p| Parameter {
                            name: p.name.clone(),
                            annotation: p.annotation.as_ref().map(|t| t.qualified(&scope)),
                        })
                        .collect();
                    environment.define_function(UserFunction::new(
                        qualified,
                        qualified_params,
                        body.as_ref().clone(),
                        String::new(),
                    ));
                }
                Expression::Assignment { name, value } => {
                    let qualified = format!("{prefix}::{name}");
                    if environment.function(&qualified).is_some()
                        || environment.data_type(&qualified).is_some()
                    {
                        return Err(EngineError::domain(format!(
                            "'{qualified}' is already defined"
                        )));
                    }
                    environment.enter_namespace(Some(prefix.to_string()));
                    let result = self.evaluate(value, environment, &Locals::new(), depth);
                    environment.leave_namespace();
                    environment.set(&qualified, result?);
                }
                Expression::NamespaceDefinition {
                    name,
                    members: inner,
                } => {
                    self.register_namespace(
                        &format!("{prefix}::{name}"),
                        inner,
                        environment,
                        depth,
                        &scope,
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn array_value(
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

    fn map_value(
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
    fn arguments(
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
    fn reduce(
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

    /// Built-ins win (collisions are impossible — definitions are blocked
    /// above); then sheet-scoped λ cells (specific scope over general); then
    /// log functions; then data type constructors; then variables/locals
    /// holding a function value (`f = x -> x * 2` then `f(3)`); then error.
    fn call(
        &self,
        name: &str,
        arguments: Vec<Value>,
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<Value, EngineError> {
        if self.registry.contains(name) {
            return self.registry_call(name, &arguments, environment, depth);
        }
        // Inside a namespaced member, an unqualified call resolves a sibling
        // (function or type constructor) first — `Bits::area` calling
        // `width` — walking up the nesting chain.
        let current_namespace = environment.current_namespace().map(str::to_string);
        for qualified in sibling_candidates(name, current_namespace.as_deref()) {
            if let Some(function) = environment.function(&qualified).cloned() {
                return self.apply_user(function, arguments, Locals::new(), environment, depth);
            }
            if let Some(data_type) = environment.data_type(&qualified).cloned() {
                return Self::construct(&data_type, &arguments);
            }
        }
        if let Some(resolve) = &self.resolvers.scoped_function {
            if let Some(scoped) = resolve(name) {
                return self.apply_user(scoped, arguments, Locals::new(), environment, depth);
            }
        }
        let overloads = environment.overloads(name).to_vec();
        if !overloads.is_empty() {
            let chosen = Self::select_overload(name, &arguments, &overloads)?.clone();
            return self.apply_user(chosen, arguments, Locals::new(), environment, depth);
        }
        let scoped_type = self
            .resolvers
            .scoped_data_type
            .as_ref()
            .and_then(|resolve| resolve(name));
        if let Some(data_type) = scoped_type.or_else(|| environment.data_type(name).cloned()) {
            return Self::construct(&data_type, &arguments);
        }
        let bound = locals.get(name).cloned().or_else(|| environment.get(name));
        if let Some(Value::Function(function)) = bound {
            return self.apply_function(&Value::Function(function), &arguments, environment, depth);
        }
        // Host reflection functions (`cell`, `sheetNames`, …) resolve LAST —
        // a user's own `cell(x) = …` shadows them, like any builtin would.
        if let Some(resolve) = &self.resolvers.host_function {
            if let Some(value) = resolve((self, environment), name, &arguments)? {
                return Ok(value);
            }
        }
        // Host mutations (`updateCell`, `addWorksheet`, …) resolve last of
        // all, and the resolver rejects them outside the log.
        if let Some(resolve) = &self.resolvers.host_mutation {
            if let Some(value) =
                resolve((self, environment), name, &arguments, self.allow_mutation)?
            {
                return Ok(value);
            }
        }
        // Imported namespaces are the final fallback (a user/host/builtin
        // name always wins); the import conflict check keeps this
        // unambiguous.
        if let Some(qualified) = environment.imported_name(name) {
            if let Some(function) = environment.function(&qualified).cloned() {
                return self.apply_user(function, arguments, Locals::new(), environment, depth);
            }
            if let Some(data_type) = environment.data_type(&qualified).cloned() {
                return Self::construct(&data_type, &arguments);
            }
        }
        // A qualified builtin (`Finance::pmt`) — the bare name is also
        // global.
        if let Some(bare) = self.registry.resolve_qualified(name) {
            return self.registry_call(&bare, &arguments, environment, depth);
        }
        Err(EngineError::UnknownFunction {
            name: name.to_string(),
        })
    }

    /// A registry call with the applier wired to this evaluator (how
    /// higher-order builtins call back in).
    fn registry_call(
        &self,
        name: &str,
        arguments: &[Value],
        environment: &mut EvaluationEnvironment,
        depth: usize,
    ) -> Result<Value, EngineError> {
        self.registry.call(name, arguments, &mut |function, args| {
            self.apply_function(function, args, environment, depth)
        })
    }

    /// Picks the user-function overload that matches the argument types.
    /// A typed parameter matches only an argument of that type; an untyped
    /// parameter matches anything. Among matching overloads the most
    /// specific (most typed params) wins; a tie is ambiguous. With no typed
    /// match, the untyped catch-all (if any) is used.
    fn select_overload<'f>(
        name: &str,
        arguments: &[Value],
        overloads: &'f [UserFunction],
    ) -> Result<&'f UserFunction, EngineError> {
        let arity_match: Vec<&UserFunction> = overloads
            .iter()
            .filter(|f| f.parameters.len() == arguments.len())
            .collect();
        if arity_match.is_empty() {
            // No overload of this arity — surface the standard arity error.
            return Ok(&overloads[0]);
        }
        let fits = |f: &UserFunction| {
            f.parameters.iter().zip(arguments).all(|(param, arg)| {
                param
                    .annotation
                    .as_ref()
                    .is_none_or(|t| type_matches(arg, t))
            })
        };
        let fitting: Vec<&UserFunction> = arity_match.into_iter().filter(|f| fits(f)).collect();
        let typed: Vec<&UserFunction> = fitting.iter().copied().filter(|f| f.is_typed()).collect();
        if !typed.is_empty() {
            let typed_count = |f: &UserFunction| {
                f.parameters
                    .iter()
                    .filter(|p| p.annotation.is_some())
                    .count()
            };
            let most_specific = typed
                .iter()
                .map(|f| typed_count(f))
                .max()
                .expect("non-empty");
            let best: Vec<&UserFunction> = typed
                .into_iter()
                .filter(|f| typed_count(f) == most_specific)
                .collect();
            if best.len() != 1 {
                return Err(EngineError::domain(format!(
                    "ambiguous call to '{name}' — more than one overload matches"
                )));
            }
            return Ok(best[0]);
        }
        if let Some(untyped) = fitting.iter().find(|f| !f.is_typed()) {
            return Ok(untyped);
        }
        // Right arity, but the argument types match no overload.
        let got: Vec<String> = arguments.iter().map(Value::kind_name).collect();
        Err(EngineError::domain(format!(
            "no overload of '{name}' accepts ({})",
            got.join(", ")
        )))
    }

    /// The user operator overload (`+(a: Point, b: Point) = …`) matching
    /// these operand types, or `None` to fall through to the built-in
    /// operator. Errors only when several overloads are equally specific.
    fn operator_overload(
        op: BinaryOperator,
        lhs: &Value,
        rhs: &Value,
        environment: &EvaluationEnvironment,
    ) -> Result<Option<UserFunction>, EngineError> {
        let overloads = environment.overloads(op.symbol());
        if overloads.is_empty() {
            return Ok(None);
        }
        let args = [lhs, rhs];
        let fitting: Vec<&UserFunction> = overloads
            .iter()
            .filter(|f| {
                f.parameters.len() == 2
                    && f.parameters.iter().zip(args.iter()).all(|(param, arg)| {
                        param
                            .annotation
                            .as_ref()
                            .is_none_or(|t| type_matches(arg, t))
                    })
            })
            .collect();
        if fitting.is_empty() {
            return Ok(None); // none match → built-in
        }
        let typed_count = |f: &UserFunction| {
            f.parameters
                .iter()
                .filter(|p| p.annotation.is_some())
                .count()
        };
        let most_specific = fitting
            .iter()
            .map(|f| typed_count(f))
            .max()
            .expect("non-empty");
        let best: Vec<&UserFunction> = fitting
            .into_iter()
            .filter(|f| typed_count(f) == most_specific)
            .collect();
        if best.len() != 1 {
            return Err(EngineError::domain(format!(
                "ambiguous '{}' for {} and {}",
                op.symbol(),
                lhs.kind_name(),
                rhs.kind_name()
            )));
        }
        Ok(Some(best[0].clone()))
    }

    /// Instantiates a data type. Exactly one map argument — what the named-
    /// argument syntax desugars to, and literally the from-map form. Every
    /// declared field must be present and type-correct, nothing extra;
    /// fields canonicalize to declaration order.
    fn construct(data_type: &DataType, arguments: &[Value]) -> Result<Value, EngineError> {
        let provided = match arguments {
            [Value::Map(provided)] => provided,
            _ => {
                let example: Vec<String> = data_type
                    .fields
                    .iter()
                    .map(|f| format!("{}: …", f.name))
                    .collect();
                return Err(EngineError::domain(format!(
                    "{}(…) takes named fields — {}({}) — or one map",
                    data_type.name,
                    data_type.name,
                    example.join(", ")
                )));
            }
        };
        let mut entries = Vec::with_capacity(data_type.fields.len());
        for field in &data_type.fields {
            let Some(value) = provided
                .iter()
                .find(|e| e.key == field.name)
                .map(|e| &e.value)
            else {
                return Err(EngineError::domain(format!(
                    "{} is missing '{}' — it needs {}",
                    data_type.name,
                    field.name,
                    data_type.field_list()
                )));
            };
            entries.push(MapEntry::new(
                field.name.clone(),
                field
                    .field_type
                    .validate(value, &field.name, &data_type.name)?,
            ));
        }
        let declared: HashSet<&str> = data_type.fields.iter().map(|f| f.name.as_str()).collect();
        if let Some(extra) = provided.iter().find(|e| !declared.contains(e.key.as_str())) {
            return Err(EngineError::domain(format!(
                "{} has no field '{}' — it has {}",
                data_type.name,
                extra.key,
                data_type.field_list()
            )));
        }
        Ok(Value::Record(RecordValue {
            type_name: data_type.name.clone(),
            entries,
            boolean_fields: data_type
                .fields
                .iter()
                .filter(|f| f.field_type == DataFieldType::Boolean)
                .map(|f| f.name.clone())
                .collect(),
        }))
    }

    /// Applies a function VALUE — what the higher-order builtins call back
    /// into. Named references re-resolve (so they follow redefinitions).
    pub(crate) fn apply_function(
        &self,
        value: &Value,
        arguments: &[Value],
        environment: &mut EvaluationEnvironment,
        depth: usize,
    ) -> Result<Value, EngineError> {
        let Value::Function(function) = value else {
            return Err(EngineError::domain(format!(
                "expected a function (a name or x -> …), got {}",
                value.kind_name()
            )));
        };
        match &function.kind {
            FunctionValueKind::Builtin(name) => {
                self.registry_call(name, arguments, environment, depth)
            }
            FunctionValueKind::User(name) => {
                if let Some(user) = environment.function(name).cloned() {
                    return self.apply_user(
                        user,
                        arguments.to_vec(),
                        Locals::new(),
                        environment,
                        depth,
                    );
                }
                // Constructors travel by name too (`map(Person, maps)`).
                let scoped_type = self
                    .resolvers
                    .scoped_data_type
                    .as_ref()
                    .and_then(|resolve| resolve(name));
                if let Some(data_type) =
                    scoped_type.or_else(|| environment.data_type(name).cloned())
                {
                    return Self::construct(&data_type, arguments);
                }
                Err(EngineError::UnknownFunction { name: name.clone() })
            }
            FunctionValueKind::Lambda { parameters, body } => self.apply_user(
                UserFunction::new(
                    "lambda".to_string(),
                    parameters
                        .iter()
                        .map(|p| Parameter::new(p.clone()))
                        .collect(),
                    body.clone(),
                    String::new(),
                ),
                arguments.to_vec(),
                function.captures.iter().cloned().collect(),
                environment,
                depth,
            ),
        }
    }

    /// `arr[0]` (0-based), `"abc"[0]`, `m["key"]`.
    fn subscript_value(
        &self,
        environment: &mut EvaluationEnvironment,
        base: &Value,
        index: &Value,
    ) -> Result<Value, EngineError> {
        match base {
            Value::Array(items) => {
                let position = require_int(&index.as_number("an array index")?, "array index")?;
                let count = items.len();
                if position < 0 || position as usize >= count {
                    return Err(EngineError::domain(format!(
                        "index {position} is out of range (array has {count} element{})",
                        if count == 1 { "" } else { "s" }
                    )));
                }
                Ok(items[position as usize].clone())
            }

            Value::String(text) => {
                let position = require_int(&index.as_number("a string index")?, "string index")?;
                let count = text.chars().count();
                if position < 0 || position as usize >= count {
                    return Err(EngineError::domain(format!(
                        "index {position} is out of range (string has {count} character{})",
                        if count == 1 { "" } else { "s" }
                    )));
                }
                let ch = text.chars().nth(position as usize).expect("bounds checked");
                Ok(Value::String(ch.to_string()))
            }

            Value::Map(_) | Value::Record(_) => {
                let Value::String(key) = index else {
                    return Err(EngineError::domain(format!(
                        "map keys are strings — e.g. m[\"name\"], got {}",
                        index.kind_name()
                    )));
                };
                if let Some(value) = base.map_value(key) {
                    return Ok(value.clone());
                }
                if let Value::Record(record) = base {
                    return Err(EngineError::domain(format!(
                        "{} has no field '{key}' — it has {}",
                        record.type_name,
                        record
                            .entries
                            .iter()
                            .map(|e| e.key.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
                Err(EngineError::domain(format!("no key '{key}' in map")))
            }

            // Host handles define their own indexing (Worksheets[0] /
            // ["Budget"]).
            Value::Host(object) => {
                let object = Rc::clone(object);
                object.index((self, environment), index).ok_or_else(|| {
                    EngineError::domain(format!(
                        "{} can't be indexed by {}",
                        object.type_name(),
                        index.kind_name()
                    ))
                })
            }

            Value::Number(_) | Value::FixedInt(_) | Value::FixedDecimal(_) | Value::Function(_) => {
                Err(EngineError::domain(format!(
                    "{} can't be indexed",
                    base.kind_name()
                )))
            }
        }
    }

    fn apply_op(op: BinaryOperator, lhs: &Value, rhs: &Value) -> Result<Value, EngineError> {
        // `+` concatenates as soon as either side is a string — "Q" + 1 is
        // "Q1".
        if op == BinaryOperator::Add
            && (matches!(lhs, Value::String(_)) || matches!(rhs, Value::String(_)))
        {
            return Ok(Value::String(format!(
                "{}{}",
                lhs.display_text(),
                rhs.display_text()
            )));
        }
        // Fixed-width integer arithmetic: the mixing matrix + checked
        // overflow (docs/FIXED-WIDTH.md). Numeric (non-FixedInt) operands
        // skip this and take the exact-decimal path below, unchanged.
        if FixedInt::is_involved(lhs, rhs) {
            return FixedInt::apply_binary(op, lhs, rhs);
        }
        // Fixed-precision decimal arithmetic — the money-type mixing matrix.
        if FixedDecimal::is_involved(lhs, rhs) {
            return FixedDecimal::apply_binary(op, lhs, rhs);
        }
        let a = lhs.as_number(op.symbol())?;
        let b = rhs.as_number(op.symbol())?;
        Ok(Value::Number(match op {
            BinaryOperator::Add => &a + &b,
            BinaryOperator::Subtract => &a - &b,
            BinaryOperator::Multiply => &a * &b,
            BinaryOperator::Divide => a.div(&b)?,
            BinaryOperator::Modulo => a.rem(&b)?,
            BinaryOperator::Power => super::numeric::pow(&a, &b)?,
        }))
    }

    /// `==`/`!=` are deep equality on any values; ordering needs numbers.
    fn compare(op: ComparisonOperator, lhs: &Value, rhs: &Value) -> Result<Value, EngineError> {
        match op {
            ComparisonOperator::Equal => Ok(Value::bool(lhs == rhs)),
            ComparisonOperator::NotEqual => Ok(Value::bool(lhs != rhs)),
            _ => {
                let a = lhs.as_number(op.symbol())?;
                let b = rhs.as_number(op.symbol())?;
                Ok(Value::bool(match op {
                    ComparisonOperator::Less => a < b,
                    ComparisonOperator::Greater => a > b,
                    ComparisonOperator::LessOrEqual => a <= b,
                    ComparisonOperator::GreaterOrEqual => a >= b,
                    ComparisonOperator::Equal | ComparisonOperator::NotEqual => {
                        unreachable!("handled above")
                    }
                }))
            }
        }
    }

    // MARK: Recursion (the port of Evaluator+Recursion.swift)

    /// User-function application with TAIL-CALL OPTIMIZATION: a recursive
    /// call in tail position (the whole result of the taken if() branch)
    /// loops here at CONSTANT stack. Non-tail recursion still stacks,
    /// hopping to fresh 16 MB segments when the thread runs low
    /// (`stacker::maybe_grow`).
    pub(crate) fn apply_user(
        &self,
        function: UserFunction,
        arguments: Vec<Value>,
        captures: Locals,
        environment: &mut EvaluationEnvironment,
        depth: usize,
    ) -> Result<Value, EngineError> {
        let mut function = function;
        let mut arguments = arguments;
        let mut captures = captures;
        let mut iterations: usize = 0;

        loop {
            if function.parameters.len() != arguments.len() {
                return Err(EngineError::ArityMismatch {
                    function: function.name.clone(),
                    expected: function.parameters.len().to_string(),
                    got: arguments.len(),
                });
            }
            if depth >= MAX_CALL_DEPTH || iterations >= MAX_TAIL_ITERATIONS {
                return Err(EngineError::domain(format!(
                    "function calls nested too deeply — if {}() is recursive, check its base case — e.g. factorial is fact2(n) = if(n <= 1, 1, n * fact2(n - 1)), fibonacci is fib(n) = if(n <= 2, 1, fib(n - 1) + fib(n - 2))",
                    function.name
                )));
            }
            // Parameters shadow captures, which shadow globals.
            let mut locals = captures.clone();
            for (parameter, argument) in function.parameters.iter().zip(arguments.iter()) {
                locals.insert(parameter.name.clone(), argument.clone());
            }
            // A namespaced member resolves siblings unqualified while its
            // body runs (home-context); a plain function pushes None.
            // Per-iteration so a tail call into another namespace sees the
            // right home; balanced on error.
            environment.enter_namespace(home_namespace(&function.name).map(str::to_string));
            // Out of stack ≠ out of budget: grow onto a fresh segment when
            // the red zone is near (checked by stacker per call).
            let step = stacker::maybe_grow(STACK_HEADROOM, SEGMENT_STACK_SIZE, || {
                self.tail_step(&function.body, environment, &locals, depth + 1)
            });
            environment.leave_namespace();
            match step? {
                TailStep::Value(value) => return Ok(value),
                TailStep::Call(next, next_arguments, next_captures) => {
                    function = next;
                    arguments = next_arguments;
                    captures = next_captures;
                    iterations += 1;
                }
            }
        }
    }

    /// Walks tail positions: through the taken branch of if(), down to a
    /// call. A call resolving to a USER function (scoped λ cell, log
    /// function, or a function-valued variable/lambda) becomes a
    /// `TailStep::Call`; registry builtins and every other shape evaluate
    /// normally. Resolution order mirrors `call` exactly — keep them in
    /// sync.
    fn tail_step(
        &self,
        expression: &Expression,
        environment: &mut EvaluationEnvironment,
        locals: &Locals,
        depth: usize,
    ) -> Result<TailStep, EngineError> {
        match expression {
            Expression::Conditional {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.evaluate(condition, environment, locals, depth)?;
                let branch = if condition.as_number("the if() condition")?.is_zero() {
                    otherwise
                } else {
                    then
                };
                self.tail_step(branch, environment, locals, depth)
            }

            Expression::Call {
                name,
                arguments: argument_exprs,
            } if !self.registry.contains(name) => {
                let arguments = self.arguments(argument_exprs, environment, locals, depth)?;
                // Mirror call's namespace-sibling resolution (home-context),
                // walking up the nesting chain.
                let current_namespace = environment.current_namespace().map(str::to_string);
                for qualified in sibling_candidates(name, current_namespace.as_deref()) {
                    if let Some(function) = environment.function(&qualified).cloned() {
                        return Ok(TailStep::Call(function, arguments, Locals::new()));
                    }
                    if let Some(data_type) = environment.data_type(&qualified).cloned() {
                        return Ok(TailStep::Value(Self::construct(&data_type, &arguments)?));
                    }
                }
                if let Some(resolve) = &self.resolvers.scoped_function {
                    if let Some(scoped) = resolve(name) {
                        return Ok(TailStep::Call(scoped, arguments, Locals::new()));
                    }
                }
                let overloads = environment.overloads(name).to_vec();
                if !overloads.is_empty() {
                    let chosen = Self::select_overload(name, &arguments, &overloads)?.clone();
                    return Ok(TailStep::Call(chosen, arguments, Locals::new()));
                }
                let scoped_type = self
                    .resolvers
                    .scoped_data_type
                    .as_ref()
                    .and_then(|resolve| resolve(name));
                if let Some(data_type) =
                    scoped_type.or_else(|| environment.data_type(name).cloned())
                {
                    return Ok(TailStep::Value(Self::construct(&data_type, &arguments)?));
                }
                let bound = locals.get(name).cloned().or_else(|| environment.get(name));
                if let Some(Value::Function(function)) = bound {
                    match &function.kind {
                        FunctionValueKind::Lambda { parameters, body } => {
                            return Ok(TailStep::Call(
                                UserFunction::new(
                                    name.to_string(),
                                    parameters
                                        .iter()
                                        .map(|p| Parameter::new(p.clone()))
                                        .collect(),
                                    body.clone(),
                                    String::new(),
                                ),
                                arguments,
                                function.captures.iter().cloned().collect(),
                            ));
                        }
                        FunctionValueKind::User(user_name) => {
                            if let Some(user) = environment.function(user_name).cloned() {
                                return Ok(TailStep::Call(user, arguments, Locals::new()));
                            }
                            return Err(EngineError::UnknownFunction {
                                name: user_name.clone(),
                            });
                        }
                        FunctionValueKind::Builtin(builtin_name) => {
                            return Ok(TailStep::Value(self.registry_call(
                                builtin_name,
                                &arguments,
                                environment,
                                depth,
                            )?));
                        }
                    }
                }
                // Imported namespaces — the final fallback (mirrors call).
                if let Some(qualified) = environment.imported_name(name) {
                    if let Some(function) = environment.function(&qualified).cloned() {
                        return Ok(TailStep::Call(function, arguments, Locals::new()));
                    }
                    if let Some(data_type) = environment.data_type(&qualified).cloned() {
                        return Ok(TailStep::Value(Self::construct(&data_type, &arguments)?));
                    }
                }
                // A qualified builtin (`Finance::pmt`).
                if let Some(bare) = self.registry.resolve_qualified(name) {
                    return Ok(TailStep::Value(self.registry_call(
                        &bare,
                        &arguments,
                        environment,
                        depth,
                    )?));
                }
                Err(EngineError::UnknownFunction {
                    name: name.to_string(),
                })
            }

            _ => Ok(TailStep::Value(self.evaluate(
                expression,
                environment,
                locals,
                depth,
            )?)),
        }
    }
}

/// The namespace a qualified name lives in — `Bits::area` → `Bits`,
/// `A::B::area` → `A::B`, a plain name → `None`. The prefix before the LAST
/// `::`, so a nested member's home is its immediate (innermost) namespace.
fn home_namespace(name: &str) -> Option<&str> {
    name.rfind("::").map(|i| &name[..i])
}

/// Qualified candidates for an unqualified `name` seen inside `namespace`,
/// walking UP the nesting chain: in `A::B`, `c` is tried as `A::B::c` then
/// `A::c` (then the caller falls through to global). Empty when there's no
/// home context or the name is already qualified. The single source of truth
/// for sibling resolution — `.variable`, `call`, and `tail_step` all iterate
/// it, so they stay in sync.
fn sibling_candidates(name: &str, namespace: Option<&str>) -> Vec<String> {
    let Some(namespace) = namespace else {
        return Vec::new();
    };
    if name.contains("::") {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let mut prefix = namespace;
    loop {
        candidates.push(format!("{prefix}::{name}"));
        let Some(separator) = prefix.rfind("::") else {
            break;
        };
        prefix = &prefix[..separator];
    }
    candidates
}

/// Does a runtime value satisfy a parameter's type annotation? Booleans are
/// numbers in Anzan, so `Boolean` matches a number; a named type matches a
/// record of that type (case-insensitive, like the call namespace).
fn type_matches(value: &Value, annotation: &TypeAnnotation) -> bool {
    match (annotation, value) {
        (TypeAnnotation::Number, Value::Number(_))
        | (TypeAnnotation::Boolean, Value::Number(_)) => true,
        (TypeAnnotation::String, Value::String(_)) => true,
        (TypeAnnotation::Named(type_name), Value::Record(record)) => {
            record.type_name.eq_ignore_ascii_case(type_name)
        }
        _ => false,
    }
}

/// A binary operator by its symbol — the operator-overload definition check
/// (the Swift `BinaryOperator(rawValue:)`).
fn binary_operator_named(name: &str) -> Option<BinaryOperator> {
    match name {
        "+" => Some(BinaryOperator::Add),
        "-" => Some(BinaryOperator::Subtract),
        "*" => Some(BinaryOperator::Multiply),
        "/" => Some(BinaryOperator::Divide),
        "%" => Some(BinaryOperator::Modulo),
        "^" => Some(BinaryOperator::Power),
        _ => None,
    }
}

/// An exact integer (for indexes and bounds), or a domain error naming the
/// context.
pub(crate) fn require_int(value: &BigDecimal, what: &str) -> Result<i64, EngineError> {
    value
        .int_value()
        .ok_or_else(|| EngineError::domain(format!("{what} must be an integer")))
}
