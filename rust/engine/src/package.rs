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
mod tests {
    use super::*;
    use crate::workbook::SheetPayload;
    use anzan::Value;
    use num_bigint::BigInt;
    use std::collections::HashMap;

    fn sample() -> Workbook {
        let variables = HashMap::from([(
            "rate".to_string(),
            Value::Number(anzan::BigDecimal::new(BigInt::from(1), -1)), // 0.1
        )]);
        Workbook::new(
            vec![SheetPayload::new(
                "Sheet 1",
                HashMap::from([("A:1".into(), "42".into())]),
            )],
            None,
            &variables,
            &[],
            &HashMap::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("soroban-tests-{label}-{}", unique_suffix()));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Port of `WorkbookPackageTests.packageRoundTrip`.
    #[test]
    fn package_round_trip() {
        let dir = temp_dir("round-trip");
        let path = dir.join("model.soroban");

        write(&sample(), &path, None).expect("writes");

        assert!(path.is_dir()); // it's a package, not a flat file
        assert_eq!(database_path(&path), None); // no data sheets

        let read_back = read(&path).expect("reads");
        assert_eq!(read_back.sheets[0].cells["A:1"], "42");

        // Overwriting an existing package is atomic-replace, not append.
        let mut updated = sample();
        updated.sheets[0].cells.insert("A:2".into(), "7".into());
        write(&updated, &path, None).expect("rewrites");
        assert_eq!(read(&path).expect("re-reads").sheets[0].cells["A:2"], "7");

        fs::remove_dir_all(&dir).ok();
    }

    /// Port of `WorkbookPackageTests.legacyFlatFilesStillRead` — the real
    /// Swift-written flat fixture reads via the legacy path, and saving over
    /// a flat file upgrades it to a package in place.
    #[test]
    fn legacy_flat_files_still_read() {
        // The repo fixture: a flat JSON `.soroban` written by the Swift app.
        let fixture = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/mortgage.soroban"
        ));
        let workbook = read(fixture).expect("fixture reads via the legacy path");
        assert_eq!(workbook.sheets[0].cells["B:1"], "350000");

        // A locally written flat file behaves the same, then upgrades.
        let dir = temp_dir("legacy");
        let path = dir.join("legacy.soroban");
        fs::write(&path, sample().encode().unwrap()).unwrap(); // old-style single JSON file
        let read_back = read(&path).expect("flat file reads");
        assert_eq!(read_back.sheets[0].cells["A:1"], "42");

        // Saving over a flat file upgrades it to a package in place.
        write(&read_back, &path, None).expect("upgrade write");
        assert!(path.is_dir());
        assert_eq!(
            read(&path).expect("package reads").sheets[0].cells["A:1"],
            "42"
        );

        fs::remove_dir_all(&dir).ok();
    }

    /// Port of `WorkbookPackageTests.packageCarriesTheDatabase`.
    #[test]
    fn package_carries_the_database() {
        let dir = temp_dir("with-db");
        let fake_db = dir.join("working.sqlite");
        fs::write(&fake_db, b"not really sqlite, just bytes").unwrap();
        let path = dir.join("with-data.soroban");

        write(&sample(), &path, Some(&fake_db)).expect("writes");
        let inside = database_path(&path).expect("database travels with the package");
        assert_eq!(fs::read(&inside).unwrap(), fs::read(&fake_db).unwrap());

        fs::remove_dir_all(&dir).ok();
    }

    /// Port of `WorkbookPackageTests.emptyDirectoryIsNotAWorkbook`.
    #[test]
    fn empty_directory_is_not_a_workbook() {
        let dir = temp_dir("hollow");
        let path = dir.join("hollow.soroban");
        fs::create_dir_all(&path).unwrap();
        match read(&path) {
            Err(PackageError::MissingManifest) => {}
            other => panic!("expected MissingManifest, got {other:?}"),
        }
        assert_eq!(
            PackageError::MissingManifest.to_string(),
            "the package has no workbook.json"
        );
        fs::remove_dir_all(&dir).ok();
    }
}
