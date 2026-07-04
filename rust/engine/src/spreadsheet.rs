//! The spreadsheet's calculation model: sparse raw contents plus memoized
//! evaluation with formula auto-detection and cycle detection.
//!
//! Explicit markers override auto-detection:
//!  - `=…` is always a formula; every failure (even unknown names) is an
//!    error
//!  - `"…"` is always text, shown without the quotes (`"123"` stays a label)
//!
//! Auto-detect rules for everything else:
//!  1. blank → empty
//!  2. doesn't parse → text
//!  3. parses and references a cell → always a formula (errors surface)
//!  4. parses without cell refs → formula if it evaluates; on failure the
//!     error kind decides: unknown variable/function means it's a label
//!     ("Q1 revenue" parses as `Q1 * revenue`), anything else (division by
//!     zero, domain error, arity) is a formula mistake and shows the error
//!
//! Interior mutability discipline: every RefCell borrow is SHORT — never
//! held across an inner evaluation, which can re-enter this sheet's maps.
//! Evaluation methods take `(&Evaluator, &mut EvaluationEnvironment)` — the
//! re-entry context the resolvers thread through (the Rust answer to the
//! Swift side's shared-class Calculator re-entrancy).

use crate::cell::{Cell, Content, Definition, DefinitionKind};
use crate::cell_address::CellAddress;
use crate::context::{CellKey, ResolutionContext, SheetId};
use crate::controls::{Control, SliderInfo};
use anzan::ast::{Expression, Parameter};
use anzan::eval::registry::FunctionRegistry;
use anzan::{
    BigDecimal, DataType, EngineError, EvaluationEnvironment, Evaluator, Locals, UserFunction,
    Value,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// What the grid shows for a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellDisplay {
    Empty,
    Text(String),
    Value(BigDecimal),
    Error(String),
    /// A sheet-scoped definition — "λ tax(x)" or "𝑖 rate" (user design:
    /// definitions show a glyph, not a value; the editor shows the source).
    Definition(String),
    /// A comment-only cell (`# a note`) — the host renders it dim; it holds
    /// no value (skipped in ranges, errors on direct reference).
    Note(String),
    /// Control expressions: `slider(…)` / `rate = slider(…)` etc. — the
    /// grid draws the control; interaction rewrites the storage literal in
    /// place.
    Slider(SliderInfo),
    Stepper(SliderInfo),
    Checkbox(crate::controls::CheckboxInfo),
    Dropdown(crate::controls::DropdownInfo),
}

/// One name claimed by a definition cell on this sheet.
#[derive(Debug, Clone)]
pub struct SheetDefinition {
    /// As typed.
    pub name: String,
    pub address: CellAddress,
    pub(crate) definition: Definition,
}

/// Which of the three a definition cell is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetDefinitionKind {
    Variable,
    Function,
    DataType,
}

impl SheetDefinition {
    pub fn kind(&self) -> SheetDefinitionKind {
        match self.definition.kind {
            DefinitionKind::Variable(_) => SheetDefinitionKind::Variable,
            DefinitionKind::Function { .. } => SheetDefinitionKind::Function,
            DefinitionKind::DataType { .. } => SheetDefinitionKind::DataType,
        }
    }

    /// "f(x, y)" for a λ cell, the bare name otherwise.
    pub fn signature(&self) -> String {
        if let DefinitionKind::Function { parameters, .. } = &self.definition.kind {
            return format!("{}({})", self.name, parameters.join(", "));
        }
        self.name.clone()
    }
}

/// The re-entry pair evaluation methods receive.
pub type Host<'a, 'b> = (&'a Evaluator<'a>, &'b mut EvaluationEnvironment);

pub struct Spreadsheet {
    pub(crate) id: SheetId,
    /// Cells, parsed and statically classified at commit time (see `Cell`).
    cells: RefCell<HashMap<CellAddress, Cell>>,
    /// Shared with every sheet of a SheetStore: tracks which sheet owns the
    /// formula being evaluated and detects cycles that span sheets.
    pub(crate) context: Rc<ResolutionContext>,
    /// For error messages ("circular reference involving Budget!A:1") — set
    /// by SheetStore; `None` for a standalone single sheet.
    pub(crate) display_name: RefCell<Option<String>>,
    /// Memo for the current generation; cleared by `recalculate()`.
    cache: RefCell<HashMap<CellAddress, CellDisplay>>,
    /// One name per cell, ≤64 chars, unique per sheet (case-insensitive,
    /// like sheet names). Distinct from 𝑖 definitions: a definition names a
    /// VALUE; a cell name names the cell itself, whatever it holds.
    cell_names: RefCell<HashMap<CellAddress, String>>,
    /// Name (lowercased — one case-insensitive namespace per sheet) → its
    /// canonical definition. Earliest address (row, then column) wins; the
    /// others display errors.
    definitions: RefCell<HashMap<String, SheetDefinition>>,
    /// Guards `rate = rate + 1`-style self-reference during lazy evaluation.
    resolving_definitions: RefCell<HashSet<String>>,
    /// Live drag values for slider cells: mid-drag the UI previews here and
    /// only rewrites the cell's raw on release.
    slider_overrides: RefCell<HashMap<CellAddress, BigDecimal>>,
}

impl Spreadsheet {
    pub const COLUMN_COUNT: usize = 26;
    pub const ROW_COUNT: usize = 1000;

    pub const MAX_NAME_LENGTH: usize = 64;

    pub fn new(context: Rc<ResolutionContext>) -> Rc<Self> {
        let id = context.allocate_id();
        let sheet = Rc::new(Spreadsheet {
            id,
            cells: RefCell::new(HashMap::new()),
            context: Rc::clone(&context),
            display_name: RefCell::new(None),
            cache: RefCell::new(HashMap::new()),
            cell_names: RefCell::new(HashMap::new()),
            definitions: RefCell::new(HashMap::new()),
            resolving_definitions: RefCell::new(HashSet::new()),
            slider_overrides: RefCell::new(HashMap::new()),
        });
        context.attach(id, &sheet);
        sheet
    }

    fn key(&self, address: CellAddress) -> CellKey {
        CellKey {
            sheet: self.id,
            address,
        }
    }

    pub fn display_name(&self) -> Option<String> {
        self.display_name.borrow().clone()
    }

    pub fn set_display_name(&self, name: Option<String>) {
        *self.display_name.borrow_mut() = name;
    }

    // MARK: Editing

    /// Sets (or clears, with `None`/blank) a cell's raw content. Only this
    /// cell and the formulas that (transitively) read it are recomputed —
    /// across sheets — via the dependency graph. Definition cells are MOSTLY
    /// the exception (λ/𝑫 calls leave no graph edges → invalidate
    /// everything, like a log variable change); the carve-out is a 𝑖 cell
    /// redefining the SAME variable (a slider drag commit): `defined_value`
    /// records a read edge per consumer, so its readers are exactly known —
    /// that's what keeps controls responsive on big workbooks.
    pub fn set_cell(&self, raw: Option<&str>, address: CellAddress) {
        let new = raw.and_then(Cell::new);
        let old = {
            let mut cells = self.cells.borrow_mut();
            match new {
                Some(cell) => cells.insert(address, cell),
                None => cells.remove(&address),
            }
        };
        let cells = self.cells.borrow();
        let new_ref = cells.get(&address);
        if Self::is_same_variable_redefinition(old.as_ref(), new_ref) {
            drop(cells);
            self.rebuild_definitions(); // refresh the indexed expression
            self.context.invalidate(self.key(address));
        } else if old.as_ref().is_some_and(Cell::is_definition)
            || new_ref.is_some_and(Cell::is_definition)
        {
            drop(cells);
            self.rebuild_definitions();
            self.context.invalidate_everything();
        } else {
            drop(cells);
            self.context.invalidate(self.key(address));
        }
    }

    /// Both sides define a VARIABLE with the same (case-insensitive) name.
    /// Only 𝑖 qualifies: function/data-type calls have no dependency edges,
    /// and a name change orphans readers the graph can't see.
    fn is_same_variable_redefinition(old: Option<&Cell>, new: Option<&Cell>) -> bool {
        let (Some(old), Some(new)) = (old, new) else {
            return false;
        };
        let (Content::Definition(before), Content::Definition(after)) =
            (&old.content, &new.content)
        else {
            return false;
        };
        matches!(before.kind, DefinitionKind::Variable(_))
            && matches!(after.kind, DefinitionKind::Variable(_))
            && before.name.to_lowercase() == after.name.to_lowercase()
    }

    /// Raw contents view — what persistence stores.
    pub fn raws(&self) -> HashMap<CellAddress, String> {
        self.cells
            .borrow()
            .iter()
            .map(|(a, c)| (*a, c.raw.clone()))
            .collect()
    }

    pub fn raw(&self, address: CellAddress) -> String {
        self.cells
            .borrow()
            .get(&address)
            .map(|c| c.raw.clone())
            .unwrap_or_default()
    }

    /// Replaces all contents (used when loading persisted state).
    pub fn load(&self, contents: &HashMap<CellAddress, String>) {
        *self.cells.borrow_mut() = contents
            .iter()
            .filter_map(|(a, raw)| Cell::new(raw).map(|c| (*a, c)))
            .collect();
        self.rebuild_definitions();
        self.recalculate();
    }

    /// Drops ALL memoized results, everywhere this sheet's context reaches —
    /// for changes the dependency graph can't see (variables, functions,
    /// sheet renames, workbook loads).
    pub fn recalculate(&self) {
        self.context.invalidate_everything();
    }

    pub(crate) fn clear_memo(&self, address: CellAddress) {
        self.cache.borrow_mut().remove(&address);
    }

    pub(crate) fn clear_all_memo(&self) {
        self.cache.borrow_mut().clear();
    }

    // MARK: Named cells ('Projected Rate' — a name for a LOCATION)

    /// Sets (or clears, with `None`) a cell's name. Validates; resolution is
    /// affected everywhere, so everything recalculates.
    pub fn set_cell_name(
        &self,
        name: Option<&str>,
        address: CellAddress,
    ) -> Result<(), EngineError> {
        let Some(name) = name else {
            self.cell_names.borrow_mut().remove(&address);
            self.context.invalidate_everything();
            return Ok(());
        };
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(EngineError::domain("cell names can't be empty"));
        }
        if trimmed.chars().count() > Self::MAX_NAME_LENGTH {
            return Err(EngineError::domain(format!(
                "cell names are limited to {} characters",
                Self::MAX_NAME_LENGTH
            )));
        }
        if trimmed.contains('\'') || trimmed.contains('!') {
            return Err(EngineError::domain("cell names can't contain ' or !"));
        }
        if let Some(existing) = self.address_for_name(trimmed) {
            if existing != address {
                return Err(EngineError::domain(format!(
                    "'{trimmed}' already names cell {existing}"
                )));
            }
        }
        self.cell_names
            .borrow_mut()
            .insert(address, trimmed.to_string());
        self.context.invalidate_everything();
        Ok(())
    }

    /// Case-insensitive lookup (matching sheet-name semantics).
    pub fn address_for_name(&self, name: &str) -> Option<CellAddress> {
        let needle = name.to_lowercase();
        self.cell_names
            .borrow()
            .iter()
            .find(|(_, n)| n.to_lowercase() == needle)
            .map(|(a, _)| *a)
    }

    /// Replaces all names wholesale (workbook load).
    pub fn load_cell_names(&self, names: HashMap<CellAddress, String>) {
        *self.cell_names.borrow_mut() = names;
    }

    pub fn cell_names(&self) -> HashMap<CellAddress, String> {
        self.cell_names.borrow().clone()
    }

    /// Resolves `'name'` to the cell's numeric value — dependency edges and
    /// cycle detection ride the ordinary cell-read path.
    pub(crate) fn numeric_value_for_name(
        self: &Rc<Self>,
        host: Host<'_, '_>,
        name: &str,
    ) -> Result<BigDecimal, EngineError> {
        let Some(target) = self.address_for_name(name) else {
            let qualified = self
                .display_name()
                .map(|n| format!(" on {n}"))
                .unwrap_or_default();
            return Err(EngineError::domain(format!(
                "no cell named '{name}'{qualified}"
            )));
        };
        self.numeric_value(host, &target.column_name(), target.row_number() as i64)
    }

    // MARK: Sheet-scoped definitions (λ / 𝑖 / 𝑫 cells)

    /// Re-derives the index from the cells. Definition cells are rare, so a
    /// full scan per definition edit is cheap.
    fn rebuild_definitions(&self) {
        let cells = self.cells.borrow();
        let mut defined: Vec<(CellAddress, &Cell, &Definition)> = cells
            .iter()
            .filter_map(|(address, cell)| {
                if let Content::Definition(definition) = &cell.content {
                    Some((*address, cell, definition))
                } else {
                    None
                }
            })
            .collect();
        defined.sort_by_key(|(address, _, _)| (address.row, address.column));
        let mut definitions = HashMap::new();
        for (address, _, definition) in defined {
            let key = definition.name.to_lowercase();
            // First claim wins.
            definitions.entry(key).or_insert_with(|| SheetDefinition {
                name: definition.name.clone(),
                address,
                definition: definition.clone(),
            });
        }
        drop(cells);
        *self.definitions.borrow_mut() = definitions;
    }

    pub fn definitions(&self) -> HashMap<String, SheetDefinition> {
        self.definitions.borrow().clone()
    }

    /// A cell-defined function, callable from this sheet's formulas.
    pub(crate) fn defined_function(&self, name: &str) -> Option<UserFunction> {
        let definitions = self.definitions.borrow();
        let entry = definitions.get(&name.to_lowercase())?;
        let DefinitionKind::Function { parameters, body } = &entry.definition.kind else {
            return None;
        };
        Some(UserFunction::new(
            entry.name.clone(),
            parameters
                .iter()
                .map(|p| Parameter::new(p.clone()))
                .collect(),
            body.clone(),
            entry.definition.source.clone(),
        ))
    }

    /// A cell-declared data type (𝑫 cell), constructible from this sheet's
    /// formulas and from the log while this sheet is active.
    pub(crate) fn defined_data_type(&self, name: &str) -> Option<DataType> {
        let definitions = self.definitions.borrow();
        let entry = definitions.get(&name.to_lowercase())?;
        let DefinitionKind::DataType { fields } = &entry.definition.kind else {
            return None;
        };
        Some(DataType::new(
            entry.name.clone(),
            fields.clone(),
            entry.definition.source.clone(),
        ))
    }

    /// A cell-defined variable's value — evaluated lazily, per lookup, so
    /// the expression may read cells (the reads attribute to the CONSUMING
    /// formula, which keeps the dependency graph correct).
    pub(crate) fn defined_value(
        self: &Rc<Self>,
        host: Host<'_, '_>,
        name: &str,
    ) -> Result<Option<Value>, EngineError> {
        let key = name.to_lowercase();
        let (entry_name, entry_address, expression) = {
            let definitions = self.definitions.borrow();
            let Some(entry) = definitions.get(&key) else {
                return Ok(None);
            };
            let DefinitionKind::Variable(expression) = &entry.definition.kind else {
                return Ok(None);
            };
            (entry.name.clone(), entry.address, expression.clone())
        };
        if self.resolving_definitions.borrow().contains(&key) {
            return Err(EngineError::domain(format!(
                "circular definition involving '{entry_name}'"
            )));
        }
        // The consuming formula depends on this definition CELL — record the
        // edge so a same-name redefinition (slider drag, stepper click) can
        // invalidate just the readers instead of every memo everywhere.
        self.context.record_cell_read(self.key(entry_address));
        // A mid-drag slider definition resolves to its preview value.
        if let Some(override_value) = self.slider_overrides.borrow().get(&entry_address) {
            if SliderInfo::extract(&expression, Some(&entry_name), "slider").is_some() {
                return Ok(Some(Value::Number(override_value.clone())));
            }
        }
        self.resolving_definitions.borrow_mut().insert(key.clone());
        let result = Self::evaluate_formula(host, &expression);
        self.resolving_definitions.borrow_mut().remove(&key);
        result.map(Some)
    }

    /// Where a name is defined, for immutability errors — "Budget!A:3".
    pub fn definition_owner(&self, name: &str) -> Option<String> {
        let definitions = self.definitions.borrow();
        let entry = definitions.get(&name.to_lowercase())?;
        let prefix = self
            .display_name()
            .map(|n| format!("{n}!"))
            .unwrap_or_default();
        Some(format!("{prefix}{}", entry.address))
    }

    // MARK: Slider previews

    pub fn slider_override(&self, address: CellAddress) -> Option<BigDecimal> {
        self.slider_overrides.borrow().get(&address).cloned()
    }

    /// Sets a mid-drag preview value with TARGETED invalidation: only this
    /// cell and its recorded readers drop their memos. Never the full-recalc
    /// hammer — drags must stay cheap on big workbooks.
    pub fn set_slider_override(&self, value: BigDecimal, address: CellAddress) {
        self.slider_overrides.borrow_mut().insert(address, value);
        self.context.invalidate(self.key(address));
    }

    /// Drops a preview (drag released or cancelled), same targeted scope.
    pub fn clear_slider_override(&self, address: CellAddress) {
        if self
            .slider_overrides
            .borrow_mut()
            .remove(&address)
            .is_none()
        {
            return;
        }
        self.context.invalidate(self.key(address));
    }

    /// Drops every mid-drag preview at once — no drag survives a structural
    /// edit, whose reindexing would leave overrides pinned to stale addresses.
    pub fn clear_all_slider_overrides(&self) {
        self.slider_overrides.borrow_mut().clear();
    }

    // MARK: Evaluation

    /// A cell formula against the live environment — mutation always
    /// disabled (recalc must stay reproducible), `ans` untouched.
    fn evaluate_formula(
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
