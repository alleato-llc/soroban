//! The environment inspector (live variables / functions / data types) and the
//! reference-documentation window.

use super::*;

impl Session {
    // MARK: Inspector (slice ⑤)

    /// Live variables: log-defined user variables (`name = value`) and the
    /// active sheet's 𝑖 definitions, sorted case-insensitively.
    pub fn inspector_variables(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = {
            let calculator = self.calculator.borrow();
            calculator
                .environment()
                .user_variables()
                .iter()
                .map(|(name, value)| InspectorRow {
                    label: name.clone(),
                    detail: value.display_description(),
                    origin: Origin::Log,
                })
                .collect()
        };
        // Sheet-scoped 𝑖 definitions (name a value in a cell).
        for definition in self.active_definitions(SheetDefinitionKind::Variable) {
            let detail = match self.display_at(definition.address) {
                CellDisplay::Value(number) => number.to_string(),
                _ => String::new(),
            };
            rows.push(InspectorRow {
                label: definition.name,
                detail,
                origin: Origin::Cell(definition.address),
            });
        }
        // Named cell locations (name a place; value is the cell's).
        for (address, name) in self.store.active_sheet().grid.cell_names() {
            let detail = match self.display_at(address) {
                CellDisplay::Value(number) => number.to_string(),
                _ => String::new(),
            };
            rows.push(InspectorRow {
                label: name,
                detail,
                origin: Origin::Cell(address),
            });
        }
        sort_rows(&mut rows);
        rows
    }

    /// User functions: log-defined signatures and the sheet's λ definitions.
    pub fn inspector_functions(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = {
            let calculator = self.calculator.borrow();
            calculator
                .environment()
                .user_functions()
                .values()
                .map(|function| InspectorRow {
                    label: function.signature(),
                    detail: function.documentation().unwrap_or_default(),
                    origin: Origin::Log,
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::Function) {
            rows.push(InspectorRow {
                label: definition.signature(),
                detail: String::new(),
                origin: Origin::Cell(definition.address),
            });
        }
        sort_rows(&mut rows);
        rows
    }

    /// Declared data types: log-defined and the sheet's 𝑫 definitions.
    pub fn inspector_data_types(&self) -> Vec<InspectorRow> {
        let mut rows: Vec<InspectorRow> = {
            let calculator = self.calculator.borrow();
            calculator
                .environment()
                .user_data_types()
                .values()
                .map(|data_type| InspectorRow {
                    label: data_type.name.clone(),
                    detail: String::new(),
                    origin: Origin::Log,
                })
                .collect()
        };
        for definition in self.active_definitions(SheetDefinitionKind::DataType) {
            rows.push(InspectorRow {
                label: definition.name,
                detail: String::new(),
                origin: Origin::Cell(definition.address),
            });
        }
        sort_rows(&mut rows);
        rows
    }

    // MARK: Reference window (slice ⑤)

    /// The reference documentation, filtered by `query` (matched against each
    /// entry's signature and summary, case-insensitively). Empty query returns
    /// everything; categories with no surviving entries are dropped. Includes
    /// the user's own functions and data types first (via `Calculator`).
    pub fn reference(&self, query: &str) -> Vec<DocGroup> {
        let needle = query.trim().to_lowercase();
        self.calculator
            .borrow()
            .documentation()
            .into_iter()
            .filter_map(|category| {
                let entries: Vec<DocEntry> = category
                    .entries
                    .into_iter()
                    .filter(|entry| {
                        needle.is_empty()
                            || entry.name.to_lowercase().contains(&needle)
                            || entry.signature.to_lowercase().contains(&needle)
                            || entry.summary.to_lowercase().contains(&needle)
                    })
                    .map(|entry| DocEntry {
                        signature: entry.signature,
                        summary: entry.summary,
                    })
                    .collect();
                (!entries.is_empty()).then_some(DocGroup {
                    title: category.title,
                    entries,
                })
            })
            .collect()
    }

    /// The active sheet's definition cells of one kind (name + address, sorted
    /// later by the caller). Kept private — the gui reads the four groups.
    fn active_definitions(
        &self,
        kind: SheetDefinitionKind,
    ) -> Vec<soroban_engine::spreadsheet::SheetDefinition> {
        self.store
            .active_sheet()
            .grid
            .definitions()
            .into_values()
            .filter(|definition| definition.kind() == kind)
            .collect()
    }
}

/// Sort inspector rows case-insensitively by label (the reading order the
/// Swift inspector uses).
fn sort_rows(rows: &mut [InspectorRow]) {
    rows.sort_by_key(|a| a.label.to_lowercase());
}
