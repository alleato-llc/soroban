//! Facade for the Soroban expression engine. Owns the environment
//! (variables, `ans`) and runs the lex → parse → eval pipeline.

use crate::ast::Expression;
use crate::eval::environment::EvaluationEnvironment;
use crate::eval::evaluator::{Evaluator, Locals, Resolvers};
use crate::eval::registry::FunctionRegistry;
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError, LanguageMode, Parser};
use std::collections::HashMap;
use std::fmt;

mod documentation;
mod host_seams;

/// Where a name is cell-defined ("Budget!A:3"), for immutability errors.
pub type ScopedDefinitionOwnerResolver = Box<dyn Fn(&str) -> Option<String>>;

#[derive(Default)]
pub struct Calculator {
    environment: EvaluationEnvironment,
    /// Cell/range/name/scoped/host resolvers — wired by a hosting layer
    /// (SheetStore); all `None` in the CLI and plain tests.
    pub resolvers: Resolvers,
    pub scoped_definition_owner: Option<ScopedDefinitionOwnerResolver>,
    /// The active input/display dialect for the LOG path (`evaluate`). The
    /// host sets this (CLI `:mode`, the app's toggle). Cells are unaffected —
    /// the cell path always parses `Normal` (log-only scope). See
    /// `docs/MODES.md`.
    pub mode: LanguageMode,
}

impl fmt::Debug for Calculator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Calculator(mode: {:?})", self.mode)
    }
}

impl Calculator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn environment(&self) -> &EvaluationEnvironment {
        &self.environment
    }

    pub fn environment_mut(&mut self) -> &mut EvaluationEnvironment {
        &mut self.environment
    }

    /// Evaluates one line from the log. On success a value becomes `ans`
    /// (definitions don't). A single leading `=` is tolerated (spreadsheet
    /// muscle memory — pasted cell formulas like `=B:1 * 2` should just
    /// work).
    pub fn evaluate(&mut self, input: &str) -> Result<EvalOutcome, EngineError> {
        let mut line = input.trim();
        if let Some(rest) = line.strip_prefix('=') {
            line = rest.trim();
        }

        // A line that is ONLY a comment is a first-class note — recorded,
        // not a parse error, and it never touches `ans`.
        if let Some(comment) = Self::standalone_comment(line) {
            return Ok(EvalOutcome::Comment(comment));
        }

        let expression = Parser::parse(line, self.mode)?;

        // Cell-defined names are owned by their cells — the log can't
        // reassign them (single source of truth; edit the cell instead).
        match &expression {
            Expression::Assignment { name, .. }
            | Expression::FunctionDefinition { name, .. }
            | Expression::DataDefinition { name, .. } => {
                if let Some(resolve) = &self.scoped_definition_owner {
                    if let Some(owner) = resolve(name) {
                        return Err(EngineError::domain(format!(
                            "'{name}' is defined in cell {owner} — edit that cell to change it"
                        )));
                    }
                }
            }
            _ => {}
        }

        if let Expression::FunctionDefinition { name, .. } = &expression {
            let name = name.clone();
            self.run(&expression, false)?;
            // Keep the original line for workbook serialization — with its
            // trailing # comment, which doubles as documentation.
            self.environment.set_function_source(line, &name);
            // The just-defined overload is the last appended — report ITS
            // signature (typed dispatch can leave several per name).
            let signature = self
                .environment
                .overloads(&name)
                .last()
                .map(|f| f.signature())
                .unwrap_or_else(|| name.clone());
            return Ok(EvalOutcome::FunctionDefined { signature });
        }

        if let Expression::DataDefinition { name, .. } = &expression {
            let name = name.clone();
            self.run(&expression, false)?;
            // Same source-line persistence contract as functions.
            self.environment.set_data_type_source(line, &name);
            let declaration = self
                .environment
                .data_type(&name)
                .map(|t| t.declaration())
                .unwrap_or_else(|| name.clone());
            return Ok(EvalOutcome::DataDefined { declaration });
        }

        if let Expression::NamespaceDefinition { name, .. } = &expression {
            let name = name.clone();
            self.run(&expression, false)?;
            // Persist the declaration line — its members live under
            // qualified names and are restored by replaying this on open.
            self.environment.record_namespace_source(line);
            return Ok(EvalOutcome::DataDefined {
                declaration: format!("namespace {name} {{ … }}"),
            });
        }

        if let Expression::ImportDirective { name } = &expression {
            // 2b: brings the namespace's members into scope unqualified.
            let name = name.clone();
            self.run(&expression, false)?;
            return Ok(EvalOutcome::DataDefined {
                declaration: format!("import {name}"),
            });
        }

        if let Expression::HelpRequest { name } = &expression {
            let Some(doc) = self.documentation_for(name) else {
                return Err(EngineError::domain(format!(
                    "no documentation for '{name}' — see the Function Reference (⌘/) for everything available"
                )));
            };
            return Ok(EvalOutcome::Documentation(doc));
        }

        let value = self.run(&expression, true)?;
        self.environment.set_ans(value.clone());
        Ok(EvalOutcome::Value(value))
    }

    /// Set a user variable directly, off the log — for host-managed edits
    /// like renaming a saved bit-format. Persists with the workbook (bumps
    /// the environment's change_count) without adding a history line.
    pub fn set_user_variable(&mut self, name: &str, value: Value) {
        self.environment.set(name, value);
    }

    /// Remove a user variable directly, off the log (the counterpart to
    /// `set_user_variable`).
    pub fn remove_user_variable(&mut self, name: &str) {
        self.environment.remove_variable(name);
    }

    /// Evaluates a spreadsheet cell formula: identical semantics except
    /// `ans` is left untouched, so grid recalculation never disturbs the log
    /// session.
    pub fn evaluate_formula(&mut self, input: &str) -> Result<Value, EngineError> {
        // Cells are always canonical — log-only mode scope (docs/MODES.md).
        let expression = Parser::parse(input, LanguageMode::Normal)?;
        self.evaluate_formula_expression(&expression)
    }

    /// Same, for an already-parsed expression — the sheet parses each cell
    /// once at commit time and re-evaluates the stored AST per recalc.
    /// Function definitions belong to the log, not cells.
    pub fn evaluate_formula_expression(
        &mut self,
        expression: &Expression,
    ) -> Result<Value, EngineError> {
        if let Some(rejection) = Self::formula_rejection(expression) {
            return Err(rejection);
        }
        self.run(expression, false)
    }

    /// Why a cell can't hold this expression, or `None` when it can —
    /// definitions and session mutations belong to the log. Shared with the
    /// hosting layer's recalc path, which evaluates stored ASTs without
    /// re-entering the facade.
    pub fn formula_rejection(expression: &Expression) -> Option<EngineError> {
        match expression {
            Expression::FunctionDefinition { .. } => {
                Some(EngineError::domain("define functions in the calculation log"))
            }
            // Only reachable via `=data …` — the PLAIN form classifies as a
            // sheet definition (a 𝑫 cell) before evaluation ever sees it.
            Expression::DataDefinition { name, .. } => Some(EngineError::domain(format!(
                "drop the leading '=' — a plain 'data {name} {{ … }}' cell declares a sheet data type"
            ))),
            Expression::NamespaceDefinition { name, .. } => Some(EngineError::domain(format!(
                "define namespace {name} in the calculation log, not a cell"
            ))),
            Expression::ImportDirective { .. } => {
                Some(EngineError::domain("import in the calculation log, not a cell"))
            }
            // Only reachable via `=name = value` — the PLAIN form classifies
            // as a sheet definition before evaluation ever sees it.
            Expression::Assignment { name, .. } => Some(EngineError::domain(format!(
                "drop the leading '=' — a plain '{name} = …' cell defines a sheet variable"
            ))),
            Expression::HelpRequest { .. } => {
                Some(EngineError::domain("man works in the calculation log, not a cell"))
            }
            _ => None,
        }
    }

    /// Runs `body` with a formula-context evaluator (mutation disabled) and
    /// the live environment — the seam hosts use to drive evaluation from
    /// OUTSIDE a log line (grid recalc, display, inspector reads). Splits the
    /// borrow: the evaluator borrows the resolvers, the body gets the
    /// environment mutably.
    pub fn host_eval<R>(
        &mut self,
        body: impl FnOnce(&Evaluator<'_>, &mut EvaluationEnvironment) -> R,
    ) -> R {
        let evaluator = Evaluator {
            registry: FunctionRegistry::standard(),
            resolvers: &self.resolvers,
            allow_mutation: false,
        };
        body(&evaluator, &mut self.environment)
    }

    /// `allow_mutation` is true only on the log path — workbook mutations
    /// (`updateCell`, `addWorksheet`, …) are rejected during cell recalc so
    /// recalculation stays reproducible.
    fn run(&mut self, expression: &Expression, allow_mutation: bool) -> Result<Value, EngineError> {
        let evaluator = Evaluator {
            registry: FunctionRegistry::standard(),
            resolvers: &self.resolvers,
            allow_mutation,
        };
        evaluator.evaluate(expression, &mut self.environment, &Locals::new(), 0)
    }

    /// All built-in function names (for help/autocomplete).
    pub fn function_names() -> Vec<&'static str> {
        FunctionRegistry::standard().names()
    }

    /// Rebinds persisted variables: pure literals fold directly (the fast
    /// path every pre-`data` workbook takes); anything else — record
    /// constructor calls — evaluates against the current session, which is
    /// why types/functions must already be restored. Unparseable entries
    /// (hand-edited files) are dropped.
    pub fn restore_variables(&mut self, variables: &HashMap<String, String>) {
        let mut folded: HashMap<String, Value> = HashMap::new();
        let mut deferred: Vec<(&String, &String)> = Vec::new();
        for (name, text) in variables {
            if let Some(value) = Value::parsing(text) {
                folded.insert(name.clone(), value);
            } else {
                deferred.push((name, text));
            }
        }
        self.environment.replace_user_variables(folded);
        deferred.sort_by_key(|(name, _)| name.as_str());
        for (name, text) in deferred {
            if let Ok(value) = self.evaluate_formula(text) {
                self.environment.set(name, value);
            }
        }
    }
}

/// The reference window's data model — one documented name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionDoc {
    pub name: String,
    pub signature: String,
    pub summary: String,
    pub examples: Vec<String>,
}

/// One autocomplete candidate for the input bar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Completion {
    pub name: String,
    pub kind: CompletionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Function,
    Variable,
    Constant,
}

impl CompletionKind {
    /// Short tag shown beside a completion (`ƒ` for a function, etc.).
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Function => "ƒ",
            Self::Variable => "var",
            Self::Constant => "const",
        }
    }
}

/// What one log line produced.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalOutcome {
    Value(Value),
    FunctionDefined {
        signature: String,
    },
    DataDefined {
        declaration: String,
    },
    Documentation(FunctionDoc),
    /// A comment-only line (`# note`): a first-class note, recorded by the
    /// host but never affecting `ans`.
    Comment(String),
}

impl EvalOutcome {
    /// The numeric result, when the line was a calculation (`None` for
    /// definitions and non-numeric values).
    pub fn numeric_value(&self) -> Option<BigDecimal> {
        if let Self::Value(Value::Number(value)) = self {
            return Some(value.clone());
        }
        None
    }

    /// The clean, human-facing echo — `to_string` except a fixed-width int /
    /// fixed-precision decimal value renders as its plain number (`343353` /
    /// `10.50`) rather than its constructor. Hosts show this; `to_string`
    /// stays what they recall/copy/persist (the type survives).
    pub fn display_description(&self) -> String {
        if let Self::Value(value) = self {
            return value.display_description();
        }
        self.to_string()
    }

    /// A MULTI-line string result, raw — pretty JSON and friends. Hosts
    /// render this as a plain block (like man() output) instead of one
    /// canonical line of `\n` escapes; single-line strings keep their
    /// canonical quoting (the log stays re-parseable).
    pub fn raw_block(&self) -> Option<&str> {
        if let Self::Value(Value::String(text)) = self {
            if text.contains('\n') {
                return Some(text);
            }
        }
        None
    }
}

impl fmt::Display for EvalOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(value) => write!(f, "{value}"),
            Self::FunctionDefined { signature } => write!(f, "{signature}"),
            Self::DataDefined { declaration } => write!(f, "{declaration}"),
            Self::Documentation(doc) => {
                let mut lines = vec![doc.signature.clone(), doc.summary.clone()];
                lines.extend(doc.examples.iter().map(|e| format!("  e.g. {e}")));
                write!(f, "{}", lines.join("\n"))
            }
            Self::Comment(text) => write!(f, "# {text}"),
        }
    }
}
