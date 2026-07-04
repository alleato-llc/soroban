//! The scratch-persistence journal — the port of
//! `swift/Engine/Sources/SorobanEngine/Persistence/WorkbookJournal.swift`:
//! cell edits append one JSON line each (O(1) per edit) instead of rewriting
//! the whole workbook; a periodic snapshot (the normal Workbook encode)
//! compacts it. `.soroban` interchange files never contain a journal — this
//! exists only for live autosave.
//!
//! Replay is order-preserving and idempotent: entries are absolute cell
//! values, so replaying a stale journal over a newer snapshot converges on
//! the same final state (the crash-consistency property compaction relies
//! on: snapshot first, truncate second — a crash in between is safe).

use crate::cell_address::CellAddress;
use crate::workbook::Workbook;
use serde::{Deserialize, Serialize};

/// One journaled cell edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Sheet name (matched case-insensitively on replay).
    pub sheet: String,
    /// "A:1"-form cell key.
    pub cell: String,
    /// Raw contents; empty string clears the cell.
    pub raw: String,
}

impl Entry {
    pub fn new(sheet: impl Into<String>, cell: impl Into<String>, raw: impl Into<String>) -> Self {
        Self {
            sheet: sheet.into(),
            cell: cell.into(),
            raw: raw.into(),
        }
    }
}

/// One compact JSON line per entry (JSONL), newline-terminated.
pub fn encode_line(entry: &Entry) -> Option<Vec<u8>> {
    let mut data = serde_json::to_vec(entry).ok()?;
    data.push(0x0A);
    Some(data)
}

/// Tolerant line-by-line decode — a torn final line (crash mid-append)
/// or hand-mangled line is skipped, not fatal.
pub fn decode(data: &[u8]) -> Vec<Entry> {
    data.split(|&byte| byte == 0x0A)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

/// Replays entries onto a workbook (in order; last writer wins).
/// Entries for unknown sheets or invalid keys are skipped.
pub fn apply(entries: &[Entry], workbook: &mut Workbook) {
    for entry in entries {
        let needle = entry.sheet.to_lowercase();
        let Some(index) = workbook
            .sheets
            .iter()
            .position(|sheet| sheet.name.to_lowercase() == needle)
        else {
            continue;
        };
        if CellAddress::from_key(&entry.cell).is_none() {
            continue;
        }
        if entry.raw.is_empty() {
            workbook.sheets[index].cells.remove(&entry.cell);
        } else {
            workbook.sheets[index]
                .cells
                .insert(entry.cell.clone(), entry.raw.clone());
        }
    }
}

#[cfg(test)]
mod tests {
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
}
