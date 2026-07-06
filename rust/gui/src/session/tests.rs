//! Unit tests for `Session`: persistence round-trips and worksheet management.
//! Kept in a sibling file rather than an inline `#[cfg(test)] mod tests` block
//! (house style — tests out of source).

use super::*;

/// A persisting session reloads its log tape and ↑/↓ input history from
/// disk on the next launch — the parity fix for the Swift `LogStore`.
#[test]
fn log_tape_and_input_history_survive_a_relaunch() {
    // Point persistence at a unique temp dir, never the real data dir.
    let dir = std::env::temp_dir().join(format!(
        "soroban-persist-test-{}-{:p}",
        std::process::id(),
        &() as *const ()
    ));
    std::env::set_var("SOROBAN_DATA_DIR", &dir);

    // First launch: type two lines, which persist on each submit.
    {
        let mut session = Session::new();
        session.set_input("1 + 1".to_string());
        session.submit();
        session.set_input("2 + 3".to_string());
        session.submit();
    }

    // Second launch: the tape and recall history come back.
    {
        let mut session = Session::new();
        let entries = session.entries();
        assert_eq!(entries.len(), 2, "the log tape reloaded");
        assert_eq!(entries[0].input, "1 + 1");
        assert!(matches!(&entries[1].outcome, Outcome::Value(v) if v == "5"));
        drop(entries);
        // ↑ recalls the newest submitted line.
        session.recall_previous();
        assert_eq!(session.input(), "2 + 3", "the ↑/↓ history reloaded");
    }

    // An ephemeral session ignores the same dir entirely.
    {
        let session = Session::ephemeral();
        assert!(session.entries().is_empty(), "ephemeral loads nothing");
    }

    std::env::remove_var("SOROBAN_DATA_DIR");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Add appends a new sheet and switches to it; switching tabs is a view
/// change (no dirtying), but add/rename/delete are document mutations.
#[test]
fn add_switch_and_delete_sheets() {
    let mut session = Session::ephemeral();
    assert_eq!(session.sheet_count(), 1);
    assert!(!session.can_remove_sheet());

    let before = session.revision();
    let name = session.add_sheet().unwrap();
    assert_eq!(session.sheet_count(), 2);
    assert_eq!(session.active_sheet_index(), 1, "the new sheet is active");
    assert_eq!(session.active_sheet_name(), name);
    assert!(
        session.revision() > before,
        "adding a sheet dirties the doc"
    );
    assert!(session.can_remove_sheet());

    // Switching back is a pure view change — the revision doesn't move.
    let at_two = session.revision();
    session.activate_sheet(0);
    assert_eq!(session.active_sheet_index(), 0);
    assert_eq!(
        session.revision(),
        at_two,
        "switching tabs isn't a mutation"
    );

    // Delete refuses the last sheet.
    session.remove_active_sheet().unwrap();
    assert_eq!(session.sheet_count(), 1);
    assert!(
        session.remove_active_sheet().is_err(),
        "can't remove the last"
    );
}

/// Renaming a sheet rewrites every cross-sheet reference to match, and a
/// duplicate/invalid name is refused.
#[test]
fn rename_rewrites_cross_sheet_references() {
    let mut session = Session::ephemeral();
    let first = session.sheet_names()[0].clone();
    session.add_sheet().unwrap(); // "Sheet 2", now active (index 1)
    session.rename_active_sheet("Numbers").unwrap();
    assert_eq!(session.active_sheet_name(), "Numbers");

    // A duplicate name (case-insensitive) is refused.
    assert!(session.rename_active_sheet(&first).is_err());

    // Put a value on Numbers, and a formula referencing it on the first sheet.
    session.set_cell_raw(CellAddress::new(0, 0), "41");
    session.activate_sheet(0);
    session.set_cell_raw(CellAddress::new(0, 0), "=Numbers!A:1 + 1");

    // Rename Numbers → Nums; the first sheet's formula is respelled.
    session.activate_sheet(1);
    session.rename_active_sheet("Nums").unwrap();
    session.activate_sheet(0);
    assert_eq!(session.cell_raw(CellAddress::new(0, 0)), "=Nums!A:1 + 1");
}
