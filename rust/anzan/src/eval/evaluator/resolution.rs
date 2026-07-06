//! Name resolution: a bare `.variable` reference and namespace registration.
//! Both walk the same scoping ladder (locals → siblings → sheet scope →
//! globals → host → function/type values → builtins → imports).

use super::helpers::sibling_candidates;
use super::{Evaluator, Locals};
use crate::ast::{Expression, Parameter};
use crate::eval::data_type::{DataField, DataType};
use crate::eval::environment::{EvaluationEnvironment, UserFunction};
use crate::eval::value::{FunctionValue, Value};
use crate::EngineError;
use std::collections::HashMap;

impl Evaluator<'_> {
    /// The `.variable` case — extracted (the Swift side keeps the big switch
    /// slim for frame size; here it keeps the match readable).
    pub(super) fn variable(
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
    pub(super) fn register_namespace(
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
}
