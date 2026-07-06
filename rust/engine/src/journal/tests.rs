//! Tests for the workbook journal (WAL) replay.

use super::*;
use crate::workbook::SheetPayload;
use std::collections::HashMap;

fn base_workbook() -> Workbook {
    Workbook::new(
        vec![
            SheetPayload::new("Sheet 1", HashMap::from([("A:1".into(), "1".into())])),
            SheetPayload::new("Budget", HashMap::new()),
        ],
        None,
        &HashMap::new(),
        &[],
        &HashMap::new(),
        Vec::new(),
        Vec::new(),
    )
}

/// Port of `WorkbookJournalTests.linesRoundTrip`.
#[test]
fn lines_round_trip() {
    let entries = [
        Entry::new("Sheet 1", "A:1", "42"),
        Entry::new("Budget", "B:2", "sum(A:1..A:9) # note"),
        Entry::new("Sheet 1", "A:1", ""),
    ];
    let mut data = Vec::new();
    for entry in &entries {
        data.extend(encode_line(entry).expect("encodes"));
    }
    assert_eq!(decode(&data), entries);
}

/// Port of `WorkbookJournalTests.replayAppliesInOrderLastWriterWins`.
#[test]
fn replay_applies_in_order_last_writer_wins() {
    let mut workbook = base_workbook();
    apply(
        &[
            Entry::new("Sheet 1", "A:1", "10"),
            Entry::new("Sheet 1", "A:1", "20"), // supersedes
            Entry::new("budget", "B:2", "5"),   // case-insensitive sheet
            Entry::new("Sheet 1", "A:2", "kept"),
            Entry::new("Sheet 1", "A:2", ""), // clears
        ],
        &mut workbook,
    );

    assert_eq!(
        workbook.sheets[0].cells.get("A:1").map(String::as_str),
        Some("20")
    );
    assert_eq!(workbook.sheets[0].cells.get("A:2"), None);
    assert_eq!(
        workbook.sheets[1].cells.get("B:2").map(String::as_str),
        Some("5")
    );
}

/// Port of `WorkbookJournalTests.replayIsIdempotentOverAFreshSnapshot`.
/// The crash-consistency property: a snapshot already containing the
/// journal's effects + a replay of that same journal = same state.
#[test]
fn replay_is_idempotent_over_a_fresh_snapshot() {
    let entries = [
        Entry::new("Sheet 1", "A:1", "10"),
        Entry::new("Sheet 1", "A:1", "20"),
    ];
    let mut snapshot = base_workbook();
    apply(&entries, &mut snapshot); // "compacted" state
    let mut replayed = snapshot.clone();
    apply(&entries, &mut replayed); // crash before truncate
    assert_eq!(replayed, snapshot);
}

/// Port of `WorkbookJournalTests.junkIsSkippedNotFatal`.
/// Torn final line (crash mid-append) + unknown sheet + bad cell key.
#[test]
fn junk_is_skipped_not_fatal() {
    let mut data = Vec::new();
    data.extend(encode_line(&Entry::new("Sheet 1", "A:1", "7")).unwrap());
    data.extend(b"{\"sheet\": \"Sheet 1\", \"cel"); // torn
    let decoded = decode(&data);
    assert_eq!(decoded.len(), 1);

    let mut workbook = base_workbook();
    apply(
        &[
            Entry::new("Nope", "A:1", "9"),
            Entry::new("Sheet 1", "ZZ:9", "9"),
            Entry::new("Sheet 1", "A:3", "9"),
        ],
        &mut workbook,
    );
    assert_eq!(
        workbook.sheets[0].cells.get("A:3").map(String::as_str),
        Some("9")
    );
    assert_eq!(workbook.sheets[0].cells.len(), 2); // A:1 original + A:3
}
