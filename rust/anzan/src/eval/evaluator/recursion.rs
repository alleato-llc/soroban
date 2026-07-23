//! Recursion (the port of Evaluator+Recursion.swift). TAIL calls loop at
//! constant stack; non-tail recursion grows onto fresh 16 MB segments when
//! the current thread runs low.

use super::helpers::{home_namespace, sibling_candidates};
use super::{Evaluator, Locals};
use crate::ast::{Expression, Parameter};
use crate::eval::environment::{EvaluationEnvironment, UserFunction};
use crate::eval::value::{FunctionValueKind, Value};
use crate::EngineError;

const STACK_HEADROOM: usize = 128 * 1024;
const SEGMENT_STACK_SIZE: usize = 16 << 20; // 16 MB per segment
/// Sanity cap: a missing base case errors (with a hint) instead of chewing
/// memory forever. ~10k frames is far beyond honest recursion.
#[cfg(not(target_arch = "wasm32"))]
const MAX_CALL_DEPTH: usize = 10_000;
/// On wasm the stack CANNOT grow (`stacker::maybe_grow` is a no-op there), so
/// deep recursion would TRAP — aborting the whole instance, which is fatal for
/// a browser REPL. Node/browser default stacks trap around
/// ~500 minimal language frames (measured); 200 leaves >2x margin for
/// complex expressions. Proper TAIL recursion is unaffected (constant stack).
#[cfg(target_arch = "wasm32")]
const MAX_CALL_DEPTH: usize = 200;
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
