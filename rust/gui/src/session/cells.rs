//! Cell read/write, Excel-style point mode, TSV copy/paste, and the
//! undo/redo edit machinery shared by every grid mutation.

use super::*;

impl Session {
    /// How one cell computes right now. Reads route through the ordinary
    /// dependency-tracked path, so this reflects the live values. Uses
    /// interior mutability, hence `&self`.
    ///
    /// The live display of one cell by address (values, errors, controls).
    pub fn display_at(&self, address: CellAddress) -> CellDisplay {
        let sheet: Rc<Sheet> = self.store.active_sheet();
        self.store.display_value_on(&sheet, address)
    }

    /// The active sheet's control cells (slider / stepper / checkbox / dropdown),
    /// each with its live display — the gui hosts an interactive widget over each.
    /// Scans only the occupied cells (sparse), so it's cheap per frame.
    pub fn control_cells(&self) -> Vec<(CellAddress, CellDisplay)> {
        let mut controls: Vec<(CellAddress, CellDisplay)> = self
            .store
            .active_sheet()
            .grid
            .raws()
            .into_keys()
            .filter_map(|address| {
                let display = self.display_at(address);
                matches!(
                    display,
                    CellDisplay::Slider(_)
                        | CellDisplay::Stepper(_)
                        | CellDisplay::Checkbox(_)
                        | CellDisplay::Dropdown(_)
                )
                .then_some((address, display))
            })
            .collect();
        // Stable order (HashMap iteration isn't) so overlays don't reshuffle.
        controls.sort_by_key(|(address, _)| (address.column, address.row));
        controls
    }

    // MARK: Editing (slice ③)

    /// The raw (unevaluated) text stored in a cell — what the edit bar shows.
    /// A data sheet reads the stored table value; a grid sheet its cell raw.
    pub fn cell_raw(&self, address: CellAddress) -> String {
        let sheet = self.store.active_sheet();
        if let Some(data) = &*sheet.data.borrow() {
            return data.raw_value(address.row, address.column);
        }
        sheet.grid.raw(address)
    }

    /// Per-column widths for the active sheet, as a full `GRID_COLS`-length
    /// vector the grid indexes directly. Unset columns report `0.0`, which the
    /// grid reads as "use the default width".
    pub fn column_widths(&self) -> Vec<f32> {
        let sheet = self.store.active_sheet();
        let widths = sheet.column_widths.borrow();
        (0..GRID_COLS)
            .map(|col| widths.get(&col).copied().unwrap_or(0.0) as f32)
            .collect()
    }

    /// Set a column's width on the active sheet. Display-only (it never touches
    /// the dependency graph), but it dirties the document so the size is saved.
    pub fn set_column_width(&mut self, col: usize, width: f32) {
        self.store
            .active_sheet()
            .column_widths
            .borrow_mut()
            .insert(col, width as f64);
        self.revision += 1;
    }

    /// Would a leading operator complete this draft? True means the draft
    /// "expects an operand", so a cell click inserts a reference (point mode)
    /// rather than committing. Mirrors the Swift `Calculator.expectsOperand`.
    pub fn expects_operand(&self, draft: &str) -> bool {
        Calculator::expects_operand(draft)
    }

    /// Excel point mode: a click on `address` while editing `draft`, with
    /// `extend` set for a shift-click. When the draft ends expecting an operand
    /// (after `=`, an operator, `(`, `,`, `..`, …), the clicked cell's reference
    /// is spliced onto the draft and editing continues ([`PointClick::Inserted`]);
    /// otherwise the click means "I'm done here" and the caller commits
    /// ([`PointClick::Commit`]). The inserted reference is the cell's **name**
    /// when it has one (`'Rate'`), else its `A:1` address — names read more
    /// naturally, like Excel's defined names.
    ///
    /// Two continuations reuse the last splice (its memory lives in
    /// `point_anchor`, cleared by [`clear_point_anchor`] as an edit begins or
    /// ends): if the draft still equals what the last splice left, a plain
    /// **re-click replaces** that reference and a **shift-click extends** it into
    /// a `first..this` range (addresses, since ranges don't carry names). Once
    /// it's already a range, a further shift-click replaces it with the single
    /// clicked cell — matching the Swift `SheetModel`.
    ///
    /// [`clear_point_anchor`]: Session::clear_point_anchor
    pub fn point_click(&mut self, draft: &str, address: CellAddress, extend: bool) -> PointClick {
        if !self.wants_reference_insertion(draft) {
            self.point_anchor = None;
            return PointClick::Commit;
        }
        // Reuse the previous splice only when the draft is untouched since it.
        let anchor = self
            .point_anchor
            .as_ref()
            .filter(|a| a.draft == draft)
            .cloned();
        let (new_draft, reference) = match anchor {
            Some(anchor) if extend && !anchor.reference.contains("..") => {
                // Widen the just-inserted reference into a range: B:1 → B:1..B:4.
                let base = &draft[..draft.len() - anchor.reference.len()];
                let range = format!("{}..{}", anchor.address, address);
                (format!("{base}{range}"), range)
            }
            Some(anchor) => {
                // Re-click (or shift-click past a range) replaces the reference.
                let base = &draft[..draft.len() - anchor.reference.len()];
                let reference = self.reference_text(address);
                (format!("{base}{reference}"), reference)
            }
            None => {
                // Fresh insert: append onto the operand-expecting draft.
                let reference = self.reference_text(address);
                (format!("{draft}{reference}"), reference)
            }
        };
        self.point_anchor = Some(PointAnchor {
            draft: new_draft.clone(),
            reference,
            address,
        });
        PointClick::Inserted(new_draft)
    }

    /// Should a click insert a reference (vs. commit)? Yes when the draft still
    /// expects an operand, OR when it's exactly what our last splice left — that
    /// second case is how a re-click or shift-click keeps editing even though a
    /// complete `=B:1` no longer "expects an operand". Mirrors the Swift
    /// `wantsReferenceInsertion`.
    fn wants_reference_insertion(&self, draft: &str) -> bool {
        Calculator::expects_operand(draft)
            || self.point_anchor.as_ref().is_some_and(|a| a.draft == draft)
    }

    /// Forget the last point-mode splice — the shell calls this as an edit
    /// begins or ends so a stale anchor can't hijack a later click (the Swift
    /// `beginEditing`/`endEditing` reset).
    pub fn clear_point_anchor(&mut self) {
        self.point_anchor = None;
    }

    /// The text a point-mode click inserts for `address`: a quoted name if the
    /// cell is named on its sheet, else the bare `A:1` address.
    fn reference_text(&self, address: CellAddress) -> String {
        match self.cell_name(address) {
            Some(name) => format!("'{name}'"),
            None => address.to_string(),
        }
    }

    /// Commit one cell's raw content as an undoable edit, then recalculate.
    /// A no-op when the content is unchanged.
    pub fn set_cell_raw(&mut self, address: CellAddress, raw: &str) {
        let old = self.cell_raw(address);
        if old == raw {
            return;
        }
        // Data-sheet edits write through to SQLite (bounds-checked against the
        // table), not the dependency graph — no undo step, like the Swift app.
        let sheet = self.store.active_sheet();
        if let Some(data) = &*sheet.data.borrow() {
            if data.set_raw_value(raw, address.row, address.column).is_ok() {
                self.store.recalculate();
                self.revision += 1;
            }
            return;
        }
        self.apply_edit(vec![CellChange {
            address,
            old,
            new: raw.to_string(),
        }]);
    }

    /// TSV of the raw cell contents in the inclusive `(r0..=r1, c0..=c1)` rect —
    /// rows on `\n`, cells on `\t` (Excel/Numbers interchange). For copy/cut.
    pub fn selection_tsv(&self, r0: usize, r1: usize, c0: usize, c1: usize) -> String {
        (r0..=r1)
            .map(|row| {
                (c0..=c1)
                    .map(|col| self.cell_raw(CellAddress::new(col, row)))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear every cell in the inclusive rect as one undoable edit (cut).
    pub fn clear_range(&mut self, r0: usize, r1: usize, c0: usize, c1: usize) {
        let mut changes = Vec::new();
        for row in r0..=r1 {
            for col in c0..=c1 {
                let address = CellAddress::new(col, row);
                let old = self.cell_raw(address);
                if !old.is_empty() {
                    changes.push(CellChange {
                        address,
                        old,
                        new: String::new(),
                    });
                }
            }
        }
        if !changes.is_empty() {
            self.apply_edit(changes);
        }
    }

    /// Write a TSV block with its top-left at `anchor`, clipped to the grid, as
    /// one undoable edit. Rows split on `\n` (trailing `\r` tolerated), cells on
    /// `\t` — the inverse of [`Self::selection_tsv`], and Excel/Numbers-pasteable.
    pub fn paste_tsv(&mut self, anchor: CellAddress, tsv: &str) {
        let mut changes = Vec::new();
        for (drow, line) in tsv.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            for (dcol, field) in line.split('\t').enumerate() {
                let row = anchor.row + drow;
                let col = anchor.column + dcol;
                if row >= GRID_ROWS || col >= GRID_COLS {
                    continue;
                }
                let address = CellAddress::new(col, row);
                let old = self.cell_raw(address);
                if old != field {
                    changes.push(CellChange {
                        address,
                        old,
                        new: field.to_string(),
                    });
                }
            }
        }
        if !changes.is_empty() {
            self.apply_edit(changes);
        }
    }

    /// Apply a group of cell changes as one undo step (route every mutation
    /// through here so it stays undoable — the Swift `applyEdit` rule).
    fn apply_edit(&mut self, changes: Vec<CellChange>) {
        for change in &changes {
            self.write_raw(change.address, &change.new);
        }
        self.store.recalculate();
        self.push_edit(Edit::Cells(changes));
    }

    /// Record one undoable step, capping the stack and clearing redo.
    pub(crate) fn push_edit(&mut self, edit: Edit) {
        self.undo_stack.push(edit);
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.revision += 1;
    }

    /// Apply one side of an edit — `forward` for the "new" state (do/redo),
    /// else the "old" state (undo). Cell content recalculates; a format change
    /// is display-only.
    fn apply_side(&self, edit: &Edit, forward: bool) {
        match edit {
            Edit::Cells(changes) => {
                for change in changes {
                    let raw = if forward { &change.new } else { &change.old };
                    self.write_raw(change.address, raw);
                }
                self.store.recalculate();
            }
            Edit::Format { address, old, new } => {
                self.write_format(*address, if forward { new } else { old });
            }
            Edit::Name {
                address,
                old_name,
                new_name,
                cell_changes,
            } => {
                let grid = self.store.active_sheet().grid.clone();
                let name = if forward { new_name } else { old_name };
                // A later edit may have claimed the name; skip on failure
                // rather than crash (the Swift `try? setCellName` rule).
                let _ = grid.set_cell_name(name.as_deref(), *address);
                for change in cell_changes {
                    let raw = if forward { &change.new } else { &change.old };
                    self.write_raw(change.address, raw);
                }
                self.store.recalculate();
            }
        }
    }

    /// Low-level cell write (empty string clears the cell); no undo bookkeeping.
    pub(crate) fn write_raw(&self, address: CellAddress, raw: &str) {
        let grid = self.store.active_sheet().grid.clone();
        grid.set_cell(if raw.is_empty() { None } else { Some(raw) }, address);
    }

    /// Undo the most recent step.
    pub fn undo(&mut self) {
        if let Some(edit) = self.undo_stack.pop() {
            self.apply_side(&edit, false);
            self.redo_stack.push(edit);
            self.revision += 1;
        }
    }

    /// Redo the most recently undone step.
    pub fn redo(&mut self) {
        if let Some(edit) = self.redo_stack.pop() {
            self.apply_side(&edit, true);
            self.undo_stack.push(edit);
            self.revision += 1;
        }
    }
}
