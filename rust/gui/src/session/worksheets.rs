//! Worksheet tabs: naming, activation, and add / rename / remove.

use super::*;

impl Session {
    // MARK: Grid (read-only in slice ②)

    /// The active sheet's name — shown on the grid tab.
    pub fn active_sheet_name(&self) -> String {
        self.store.active_sheet().name()
    }

    // MARK: Worksheets

    /// Every sheet's name, in tab order.
    pub fn sheet_names(&self) -> Vec<String> {
        self.store
            .sheets()
            .iter()
            .map(|sheet| sheet.name())
            .collect()
    }

    /// The index of the active sheet.
    pub fn active_sheet_index(&self) -> usize {
        self.store.active_index()
    }

    /// The number of open sheets.
    pub fn sheet_count(&self) -> usize {
        self.store.sheet_count()
    }

    /// True when a sheet can be removed (a workbook needs at least one).
    pub fn can_remove_sheet(&self) -> bool {
        self.store.sheet_count() > 1
    }

    /// Switch the active sheet to `index` (clamped; a no-op past the end).
    /// A view change, not a document mutation — it doesn't bump `revision`
    /// (switching tabs shouldn't mark the workbook dirty).
    pub fn activate_sheet(&mut self, index: usize) {
        if index < self.store.sheet_count() {
            self.store.set_active_index(index);
        }
    }

    /// Append a new, auto-named grid sheet and make it active. Returns its
    /// name, or an error message (e.g. the 256-sheet cap). Mirrors Swift's
    /// `SheetModel.addSheet` + activate.
    pub fn add_sheet(&mut self) -> Result<String, String> {
        let sheet = self.store.add_sheet().map_err(|error| error.to_string())?;
        let name = sheet.name();
        self.store.set_active_index(self.store.sheet_count() - 1);
        self.revision += 1;
        Ok(name)
    }

    /// Rename the active sheet, rewriting every cross-sheet reference to match
    /// (`Old!A:1` → `New!A:1`). Returns an error message on an invalid or
    /// duplicate name. Mirrors Swift's `SheetModel.renameActiveSheet`.
    pub fn rename_active_sheet(&mut self, new_name: &str) -> Result<(), String> {
        let index = self.store.active_index();
        self.store
            .rename_worksheet(index, new_name)
            .map_err(|error| error.to_string())?;
        self.revision += 1;
        Ok(())
    }

    /// Remove the active sheet (refuses the last one). Formulas that referenced
    /// it fall to "unknown sheet" errors, exactly as in the AppKit app.
    pub fn remove_active_sheet(&mut self) -> Result<(), String> {
        let index = self.store.active_index();
        self.store
            .remove_sheet(index)
            .map_err(|error| error.to_string())?;
        self.revision += 1;
        Ok(())
    }
}
