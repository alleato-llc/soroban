//! The document: data sheets (CSV import, the working SQLite store), and the
//! workbook lifecycle — build/save, open, new, and post-swap reset.

use super::*;

impl Session {
    // MARK: Data sheets

    /// True when the active sheet is a DataStore-backed table (not a grid).
    pub fn active_is_data(&self) -> bool {
        self.store.active_sheet().is_data()
    }

    /// Rows the grid should render for the active sheet — the whole grid for a
    /// calculation sheet, or the table's height (capped at 10,000) for a data
    /// sheet. Mirrors Swift's `visibleRowCount`.
    pub fn visible_row_count(&self) -> usize {
        let sheet = self.store.active_sheet();
        let count = match &*sheet.data.borrow() {
            Some(data) => data.row_count().clamp(1, 10_000),
            None => Spreadsheet::ROW_COUNT,
        };
        count
    }

    /// Columns to render for the active sheet (the table's width for a data
    /// sheet, else the grid's 26).
    pub fn visible_column_count(&self) -> usize {
        let sheet = self.store.active_sheet();
        let count = match &*sheet.data.borrow() {
            Some(data) => data.column_count().max(1),
            None => Spreadsheet::COLUMN_COUNT,
        };
        count
    }

    /// The working store, opened lazily at `working_db` on first need.
    fn ensure_data_store(&mut self) -> Result<Rc<DataStore>, String> {
        if let Some(store) = &self.data_store {
            return Ok(Rc::clone(store));
        }
        let store = Rc::new(DataStore::new(&self.working_db).map_err(|error| error.to_string())?);
        self.data_store = Some(Rc::clone(&store));
        Ok(store)
    }

    /// The working-database path to fold into a save — `Some` iff the document
    /// has any data sheet, so `data.sqlite` exists in a package iff it's needed.
    fn working_database_url(&self) -> Option<PathBuf> {
        self.store
            .sheets()
            .iter()
            .any(|sheet| sheet.is_data())
            .then(|| self.working_db.clone())
    }

    /// Reset the working database to `copy_from` (a package's `data.sqlite`) or
    /// to empty (`None`): drop the connection, clear the file + WAL/SHM, then
    /// copy the source in. Mirrors Swift's `prepareWorkingDatabase`.
    fn prepare_working_database(&mut self, copy_from: Option<&Path>) {
        self.data_store = None; // close the connection before touching the file
        for suffix in ["", "-wal", "-shm"] {
            let mut path = self.working_db.clone().into_os_string();
            path.push(suffix);
            let _ = std::fs::remove_file(path);
        }
        if let Some(source) = copy_from {
            let _ = std::fs::copy(source, &self.working_db);
        }
    }

    /// Import a CSV file as a new data sheet (a SQLite-backed table). Returns an
    /// optional note (e.g. that columns past the 26th were dropped). Mirrors
    /// Swift's `SheetModel.importCSV`.
    pub fn import_csv(&mut self, path: &Path) -> Result<Option<String>, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        // UTF-8, falling back to a byte-as-char (Latin-1) read like the Swift app.
        let text = String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
        let mut rows = csv::parse(&text);
        if rows.iter().all(|row| row.is_empty()) {
            return Err("the CSV file is empty".into());
        }
        // Cap at the grid's column count (extra columns are dropped).
        let mut truncated = false;
        for row in &mut rows {
            if row.len() > Spreadsheet::COLUMN_COUNT {
                row.truncate(Spreadsheet::COLUMN_COUNT);
                truncated = true;
            }
        }
        let name = self.unique_sheet_name(Self::sanitized_name(path));
        let store = self.ensure_data_store()?;
        store
            .create_table(&name, &rows)
            .map_err(|error| error.to_string())?;
        let data = DataSheet::new(&name, Rc::clone(&store))
            .ok_or_else(|| "the imported table could not be opened".to_string())?;
        let sheet = self.store.make_data_sheet(&name, data);
        let mut sheets = self.store.sheets();
        sheets.push(Rc::clone(&sheet));
        self.store.replace_sheets(sheets, Some(&name));
        self.revision += 1;
        Ok(
            truncated
                .then(|| format!("Columns beyond {} were dropped.", Spreadsheet::COLUMN_COUNT)),
        )
    }

    /// A sanitized base table/sheet name from a file: the stem with `!`/`'`
    /// (reference-syntax breakers) blanked, trimmed, defaulting to "Data", and
    /// truncated to leave room for a de-dup " <n>" suffix.
    fn sanitized_name(path: &Path) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Data");
        let cleaned: String = stem
            .chars()
            .map(|c| if c == '!' || c == '\'' { ' ' } else { c })
            .collect();
        let trimmed = cleaned.trim();
        let base = if trimmed.is_empty() { "Data" } else { trimmed };
        base.chars()
            .take(SheetStore::MAX_NAME_LENGTH.saturating_sub(4))
            .collect()
    }

    /// `base`, or `base 2` / `base 3` / … until it's unused (case-insensitive).
    fn unique_sheet_name(&self, base: String) -> String {
        if self.store.sheet_named(&base).is_none() {
            return base;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} {n}");
            if self.store.sheet_named(&candidate).is_none() {
                return candidate;
            }
            n += 1;
        }
    }

    // MARK: Workbook (slice ⑥)

    /// A monotonic mutation counter — the shell compares it to a saved baseline
    /// to show the dirty indicator.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Snapshot the document into a `Workbook`: every sheet's raw cells, named
    /// cells, and formats, plus the log's variables, functions, and data types.
    /// Data sheets carry only a `kind`/`table` marker — their rows live in the
    /// package's `data.sqlite`, folded in by `save_to`.
    fn build_workbook(&self) -> Workbook {
        let payloads: Vec<SheetPayload> = self
            .store
            .sheets()
            .iter()
            .map(|sheet| {
                let mut payload = SheetPayload::new(
                    sheet.name(),
                    sheet
                        .grid
                        .raws()
                        .into_iter()
                        .map(|(address, raw)| (address.to_string(), raw))
                        .collect::<HashMap<String, String>>(),
                );
                payload.names = sheet
                    .grid
                    .cell_names()
                    .into_iter()
                    .map(|(address, name)| (address.to_string(), name))
                    .collect();
                payload.column_widths = sheet
                    .column_widths
                    .borrow()
                    .iter()
                    .map(|(col, width)| (CellAddress::column_name_for(*col), *width))
                    .collect();
                payload.formats = sheet
                    .formats
                    .borrow()
                    .iter()
                    .map(|(address, format)| (address.to_string(), format.clone()))
                    .collect();
                if let Some(data) = &*sheet.data.borrow() {
                    payload.kind = Some("data".to_string());
                    payload.table = Some(data.table().to_string());
                }
                payload
            })
            .collect();

        let calculator = self.calculator.borrow();
        let environment = calculator.environment();
        let functions: Vec<UserFunction> = environment
            .all_user_functions()
            .into_iter()
            .cloned()
            .collect();
        Workbook::new(
            payloads,
            None,
            environment.user_variables(),
            &functions,
            environment.user_data_types(),
            environment.namespace_sources().to_vec(),
            environment.imported_namespaces().to_vec(),
        )
    }

    /// Write the document to a `.soroban` package, folding in the working
    /// `data.sqlite` when the document has any data sheets.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let workbook = self.build_workbook();
        let database = self.working_database_url();
        // Flush the WAL so the byte copy of the working DB captures every row.
        if database.is_some() {
            if let Some(store) = &self.data_store {
                store.checkpoint().map_err(|error| error.to_string())?;
            }
        }
        package::write(&workbook, path, database.as_deref()).map_err(|error| error.to_string())
    }

    /// Open a `.soroban` (package or legacy flat file), replacing the current
    /// document. The package's `data.sqlite` (if any) is copied into the working
    /// store first; restore order is types → functions → variables (via
    /// `restore_session`), then the sheets.
    pub fn open_from(&mut self, path: &Path) -> Result<(), String> {
        let workbook = package::read(path).map_err(|error| error.to_string())?;
        self.prepare_working_database(package::database_path(path).as_deref());
        self.load_workbook(workbook);
        Ok(())
    }

    /// Reset to an empty single-sheet document (New).
    pub fn new_workbook(&mut self) {
        self.prepare_working_database(None); // discard any data sheets' working db
        let (calculator, store) = Self::fresh_engine();
        self.calculator = calculator;
        self.store = store;
        self.install_log_source(); // the new store needs the tape rewired
        self.reset_document_state();
    }

    /// Rebuild the engine from a decoded workbook and swap it in.
    fn load_workbook(&mut self, workbook: Workbook) {
        let (calculator, store) = Self::fresh_engine();
        restore_session(&mut calculator.borrow_mut(), &workbook);
        let mut sheets = Vec::new();
        for payload in &workbook.sheets {
            // A data sheet reattaches to its table in the (already-copied)
            // working database; a corrupt/missing table degrades to an empty
            // grid sheet rather than failing the whole open.
            if payload.is_data() {
                let data = payload.table.as_deref().and_then(|table| {
                    self.ensure_data_store()
                        .ok()
                        .and_then(|store| DataSheet::new(table, store))
                });
                match data {
                    Some(data) => sheets.push(store.make_data_sheet(&payload.name, data)),
                    None => sheets.push(store.make_sheet(&payload.name)),
                }
                continue;
            }
            let sheet = store.make_sheet(&payload.name);
            let contents: HashMap<CellAddress, String> = payload
                .cells
                .iter()
                .filter_map(|(key, raw)| CellAddress::from_key(key).map(|a| (a, raw.clone())))
                .collect();
            let names: HashMap<CellAddress, String> = payload
                .names
                .iter()
                .filter_map(|(key, name)| CellAddress::from_key(key).map(|a| (a, name.clone())))
                .collect();
            sheet.grid.load(&contents);
            sheet.grid.load_cell_names(names);
            *sheet.column_widths.borrow_mut() = payload
                .column_widths
                .iter()
                .filter_map(|(name, width)| {
                    CellAddress::column_index(name).map(|col| (col, *width))
                })
                .collect();
            *sheet.formats.borrow_mut() = payload
                .formats
                .iter()
                .filter_map(|(key, format)| {
                    CellAddress::from_key(key).map(|address| (address, format.clone()))
                })
                .collect();
            sheets.push(sheet);
        }
        let first = workbook.sheets.first().map(|payload| payload.name.clone());
        store.replace_sheets(sheets, first.as_deref());
        self.calculator = calculator;
        self.store = store;
        self.install_log_source(); // the new store needs the tape rewired
        self.reset_document_state();
    }

    /// Clear the per-document transient state after New/Open (the log tape is a
    /// global running history, so it's kept).
    fn reset_document_state(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.binary = None;
        self.input.clear();
        self.history_cursor = None;
    }
}
