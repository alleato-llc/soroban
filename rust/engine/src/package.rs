//! `.soroban` on disk — the port of
//! `swift/Engine/Sources/SorobanEngine/Persistence/WorkbookPackage.swift`:
//! a document package (a directory Finder shows as one file) holding the
//! diffable JSON manifest and, when the workbook has data sheets, their
//! SQLite store:
//!
//! ```text
//! MyModel.soroban/
//! ├── workbook.json   ← the authored model (same format as ever)
//! └── data.sqlite     ← present only when data sheets exist
//! ```
//!
//! Legacy flat `.soroban` JSON files read transparently (a "package with no
//! database"); saves always write the package shape.

use crate::workbook::{Workbook, WorkbookError};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MANIFEST_NAME: &str = "workbook.json";
pub const DATABASE_NAME: &str = "data.sqlite";

#[derive(Debug)]
pub enum PackageError {
    /// A directory package without its manifest — not a workbook.
    MissingManifest,
    Io(io::Error),
    Workbook(WorkbookError),
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageError::MissingManifest => write!(f, "the package has no {MANIFEST_NAME}"),
            PackageError::Io(error) => write!(f, "{error}"),
            PackageError::Workbook(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PackageError {}

impl From<io::Error> for PackageError {
    fn from(error: io::Error) -> Self {
        PackageError::Io(error)
    }
}

impl From<WorkbookError> for PackageError {
    fn from(error: WorkbookError) -> Self {
        PackageError::Workbook(error)
    }
}

/// Reads a `.soroban` at `path` — a directory package (via its manifest) or
/// a legacy flat JSON file, transparently.
pub fn read(path: &Path) -> Result<Workbook, PackageError> {
    if path.is_dir() {
        let manifest = path.join(MANIFEST_NAME);
        if !manifest.exists() {
            return Err(PackageError::MissingManifest);
        }
        return Ok(Workbook::decode(&fs::read(manifest)?)?);
    }
    // Legacy single-file workbook.
    Ok(Workbook::decode(&fs::read(path)?)?)
}

/// The package's database, when it has one. Its presence is meaningful:
/// `data.sqlite` exists iff the workbook has data sheets.
pub fn database_path(package: &Path) -> Option<PathBuf> {
    let candidate = package.join(DATABASE_NAME);
    candidate.exists().then_some(candidate)
}

/// Atomic write: builds the package in a sibling temp directory, then swaps
/// it in — replacing a legacy flat file with a package works too.
/// `database_path` (the live working store) is copied in when given.
///
/// Port note: Swift uses Foundation's `replaceItemAt` (an atomic
/// filesystem-level swap). std has no cross-platform equivalent for
/// directories, so the swap here is rename-aside → rename-in → delete-old;
/// the destination is complete-or-previous at every rename boundary, but the
/// window between the two renames (destination briefly absent) is wider than
/// Foundation's.
pub fn write(
    workbook: &Workbook,
    path: &Path,
    database_path: Option<&Path>,
) -> Result<(), PackageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = parent.join(format!(".{name}.saving-{}", unique_suffix()));
    fs::create_dir_all(&temp)?;
    let result = build_and_swap(workbook, path, database_path, &temp, parent, &name);
    // A successful swap renamed the temp dir away; on failure this cleans up.
    let _ = fs::remove_dir_all(&temp);
    result
}

fn build_and_swap(
    workbook: &Workbook,
    path: &Path,
    database_path: Option<&Path>,
    temp: &Path,
    parent: &Path,
    name: &str,
) -> Result<(), PackageError> {
    fs::write(temp.join(MANIFEST_NAME), workbook.encode()?)?;
    if let Some(database) = database_path {
        if database.exists() {
            fs::copy(database, temp.join(DATABASE_NAME))?;
        }
    }

    if path.exists() {
        // Swap: move the old item aside, the new one in, then delete the old.
        let displaced = parent.join(format!(".{name}.replaced-{}", unique_suffix()));
        fs::rename(path, &displaced)?;
        if let Err(error) = fs::rename(temp, path) {
            // Best-effort restore of the previous item before failing.
            let _ = fs::rename(&displaced, path);
            return Err(error.into());
        }
        if displaced.is_dir() {
            let _ = fs::remove_dir_all(&displaced);
        } else {
            let _ = fs::remove_file(&displaced);
        }
    } else {
        fs::rename(temp, path)?;
    }
    Ok(())
}

/// A collision-proof suffix for sibling temp names (Swift uses a UUID; this
/// avoids the dependency: pid + monotonic counter + wall-clock nanos).
fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
        nanos
    )
}

#[cfg(test)]
mod tests;
