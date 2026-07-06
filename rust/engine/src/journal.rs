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
mod tests;
