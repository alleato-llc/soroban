//! Function calls: resolving a name to a builtin/user/scoped function or a
//! data-type constructor, overload selection, operator overloading, record
//! construction, and applying a function VALUE.

use super::helpers::{sibling_candidates, type_matches};
use super::{Evaluator, Locals};
use crate::ast::{BinaryOperator, Parameter};
use crate::eval::data_type::{DataFieldType, DataType};
use crate::eval::environment::{EvaluationEnvironment, UserFunction};
use crate::eval::value::{FunctionValueKind, MapEntry, RecordValue, Value};
use crate::EngineError;
use std::collections::HashSet;

impl Evaluator<'_> {
    /// Built-ins win (collisions are impossible — definitions are blocked
    /// above); then sheet-scoped λ cells (specific scope over general); then
    /// log functions; then data type constructors; then variables/locals
    /// holding a function value (`f = x -> x * 2` then `f(3)`); then error.
    pub(super) fn call(
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
    pub(super) fn registry_call(
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
    pub(super) fn select_overload<'f>(
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
    pub(super) fn operator_overload(
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
    pub(super) fn construct(
        data_type: &DataType,
        arguments: &[Value],
    ) -> Result<Value, EngineError> {
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
}
