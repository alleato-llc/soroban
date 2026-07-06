//! Cell formats (display-only styling) and named cell locations, including a
//! rename's reference rewriting.

use super::*;

impl Session {
    // MARK: Formats (slice ④)

    /// The format applied to a cell (the default when none is set).
    pub fn cell_format(&self, address: CellAddress) -> CellFormat {
        self.store
            .active_sheet()
            .formats
            .borrow()
            .get(&address)
            .cloned()
            .unwrap_or_default()
    }

    /// Set a cell's format as an undoable step. Formats are display-only, so
    /// there's no recalc; a default format is pruned from the sparse map.
    pub fn apply_format(&mut self, address: CellAddress, new: CellFormat) {
        let old = self.cell_format(address);
        if old == new {
            return;
        }
        self.write_format(address, &new);
        self.push_edit(Edit::Format { address, old, new });
    }

    /// Low-level format write; default formats are removed (the sparse-map rule).
    pub(crate) fn write_format(&self, address: CellAddress, format: &CellFormat) {
        let sheet = self.store.active_sheet();
        let mut formats = sheet.formats.borrow_mut();
        if format.is_default() {
            formats.remove(&address);
        } else {
            formats.insert(address, format.clone());
        }
    }

    // MARK: Named cells (slice ④)

    /// The name given to a cell location, if any (`'Projected Rate'`).
    pub fn cell_name(&self, address: CellAddress) -> Option<String> {
        self.store
            .active_sheet()
            .grid
            .cell_names()
            .into_iter()
            .find(|(a, _)| *a == address)
            .map(|(_, name)| name)
    }

    /// Name a cell (empty clears the name). A rename — replacing an existing
    /// name with a new one — rewrites every `'Old'` reference to `'New'` across
    /// the sheet, all as one undoable step. Returns the engine's validation
    /// error (duplicate/too long/illegal character) so the caller can revert.
    pub fn set_cell_name(&mut self, address: CellAddress, name: &str) -> Result<(), String> {
        let trimmed = name.trim();
        let old_name = self.cell_name(address);
        let new_name = (!trimmed.is_empty()).then(|| trimmed.to_string());
        if old_name == new_name {
            return Ok(());
        }

        let grid = self.store.active_sheet().grid.clone();
        // Validate + apply the name change first; a duplicate name errors here
        // before any references are touched.
        grid.set_cell_name(new_name.as_deref(), address)
            .map_err(|error| error.to_string())?;

        // On a rename, rewrite references `'Old'` → `'New'` in every cell.
        let cell_changes = match (&old_name, &new_name) {
            (Some(old), Some(new)) => self.rename_references(old, new),
            _ => Vec::new(),
        };
        for change in &cell_changes {
            self.write_raw(change.address, &change.new);
        }
        self.store.recalculate();
        self.push_edit(Edit::Name {
            address,
            old_name,
            new_name,
            cell_changes,
        });
        Ok(())
    }

    /// The reference rewrites a rename triggers: every cell whose raw mentions
    /// `'old'` gets it respelled to `'new'` (token-precise, spacing preserved).
    fn rename_references(&self, old: &str, new: &str) -> Vec<CellChange> {
        let sheet = self.store.active_sheet();
        let sheet_name = sheet.name();
        let replacement = format!("'{new}'");
        let mut changes = Vec::new();
        for (address, raw) in sheet.grid.raws() {
            if let Some(new_raw) =
                NamedCells::rewriting(&raw, old, Some(&sheet_name), true, &replacement)
            {
                changes.push(CellChange {
                    address,
                    old: raw,
                    new: new_raw,
                });
            }
        }
        changes
    }
}
