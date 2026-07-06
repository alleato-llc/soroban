//! Named cells ('Projected Rate'), sheet-scoped λ/𝑖/𝑫 definitions, and the
//! live slider-preview overrides — everything that resolves a name on this
//! sheet or previews a control mid-drag.

use super::{Host, SheetDefinition, Spreadsheet};
use crate::cell::{Cell, Content, Definition, DefinitionKind};
use crate::cell_address::CellAddress;
use crate::controls::SliderInfo;
use anzan::ast::Parameter;
use anzan::{BigDecimal, DataType, EngineError, UserFunction, Value};
use std::collections::HashMap;
use std::rc::Rc;

impl Spreadsheet {
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
    pub(super) fn rebuild_definitions(&self) {
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
}
