//! SQLite-backed storage for DATA sheets — the port of
//! `swift/Engine/Sources/SorobanEngine/Persistence/DataStore.swift`:
//! imported records at volumes the JSON manifest shouldn't carry. Values are
//! read lazily (indexed lookups / single range queries), so opening a
//! workbook never loads tables into memory. Swift links the macOS system
//! SQLite; here `rusqlite` (bundled) plays that role — still a deliberately
//! small wrapper.

use crate::cell_address::CellAddress;
use crate::spreadsheet::Spreadsheet;
use anzan::{BigDecimal, EngineError};
use rusqlite::{Connection, OpenFlags};
use std::fmt;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    pub name: String,
    pub rows: usize,
    pub columns: usize,
}

#[derive(Debug)]
pub enum DataStoreError {
    Sqlite(String),
}

impl fmt::Display for DataStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let DataStoreError::Sqlite(message) = self;
        write!(f, "data store error: {message}")
    }
}

impl std::error::Error for DataStoreError {}

impl From<rusqlite::Error> for DataStoreError {
    fn from(error: rusqlite::Error) -> Self {
        DataStoreError::Sqlite(error.to_string())
    }
}

pub struct DataStore {
    path: PathBuf,
    conn: Connection,
}

impl DataStore {
    pub fn new(path: &Path) -> Result<Self, DataStoreError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        // `journal_mode` returns a row, so this must be a query, not execute.
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tables(
                name TEXT PRIMARY KEY COLLATE NOCASE,
                rows INTEGER NOT NULL, cols INTEGER NOT NULL)",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cells(
                t TEXT COLLATE NOCASE, r INTEGER, c INTEGER, v TEXT NOT NULL,
                PRIMARY KEY(t, r, c)) WITHOUT ROWID",
            [],
        )?;
        Ok(Self {
            path: path.to_path_buf(),
            conn,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Flush the write-ahead log into the main database file so a byte copy of
    /// `path` (what the package writer does) captures every committed row.
    /// Without this, recent imports/edits sit in the `-wal` and are lost when
    /// only `data.sqlite` is copied into a `.soroban` package.
    pub fn checkpoint(&self) -> Result<(), DataStoreError> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    // MARK: Tables

    pub fn tables(&self) -> Result<Vec<TableInfo>, DataStoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT name, rows, cols FROM tables ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(TableInfo {
                name: row.get(0)?,
                rows: row.get::<_, i64>(1)? as usize,
                columns: row.get::<_, i64>(2)? as usize,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn info(&self, name: &str) -> Option<TableInfo> {
        let needle = name.to_lowercase();
        self.tables()
            .ok()?
            .into_iter()
            .find(|info| info.name.to_lowercase() == needle)
    }

    /// Imports a rectangular table (one transaction; empty values skipped —
    /// the store is sparse like the grid).
    pub fn create_table(&self, name: &str, rows: &[Vec<String>]) -> Result<(), DataStoreError> {
        let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
        self.conn.execute_batch("BEGIN")?;
        let result = (|| -> Result<(), DataStoreError> {
            let mut statement = self
                .conn
                .prepare("INSERT OR REPLACE INTO cells(t, r, c, v) VALUES(?, ?, ?, ?)")?;
            for (r, row) in rows.iter().enumerate() {
                for (c, value) in row.iter().enumerate() {
                    if value.is_empty() {
                        continue;
                    }
                    statement.execute(rusqlite::params![name, r as i64, c as i64, value])?;
                }
            }
            self.conn.execute(
                "INSERT OR REPLACE INTO tables(name, rows, cols) VALUES(?, ?, ?)",
                rusqlite::params![name, rows.len() as i64, column_count as i64],
            )?;
            self.conn.execute_batch("COMMIT")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        result
    }

    /// Edits one cell: empty/None deletes (the store is sparse). The imported
    /// table is the workbook's own copy — edits never touch the source CSV.
    pub fn set_value(
        &self,
        value: Option<&str>,
        table: &str,
        row: usize,
        column: usize,
    ) -> Result<(), DataStoreError> {
        match value {
            Some(value) if !value.is_empty() => {
                self.conn.execute(
                    "INSERT OR REPLACE INTO cells(t, r, c, v) VALUES(?, ?, ?, ?)",
                    rusqlite::params![table, row as i64, column as i64, value],
                )?;
            }
            _ => {
                self.conn.execute(
                    "DELETE FROM cells WHERE t = ? AND r = ? AND c = ?",
                    rusqlite::params![table, row as i64, column as i64],
                )?;
            }
        }
        Ok(())
    }

    pub fn drop_table(&self, name: &str) -> Result<(), DataStoreError> {
        self.conn.execute_batch("BEGIN")?;
        let _ = self
            .conn
            .execute("DELETE FROM cells WHERE t = ?", rusqlite::params![name]);
        let _ = self
            .conn
            .execute("DELETE FROM tables WHERE name = ?", rusqlite::params![name]);
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    // MARK: Values (0-based row/column)

    /// Errors read as absent, like Swift's cached-statement path.
    pub fn value(&self, table: &str, row: usize, column: usize) -> Option<String> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT v FROM cells WHERE t = ? AND r = ? AND c = ?")
            .ok()?;
        statement
            .query_row(rusqlite::params![table, row as i64, column as i64], |r| {
                r.get::<_, String>(0)
            })
            .ok()
    }

    /// All stored values in a rectangle, one query (for range expansion).
    /// Returns `(row, column, value)` triples in row-major order.
    pub fn values(
        &self,
        table: &str,
        rows: RangeInclusive<usize>,
        columns: RangeInclusive<usize>,
    ) -> Result<Vec<(usize, usize, String)>, DataStoreError> {
        let mut statement = self.conn.prepare(
            "SELECT r, c, v FROM cells
             WHERE t = ? AND r BETWEEN ? AND ? AND c BETWEEN ? AND ?
             ORDER BY r, c",
        )?;
        let mapped = statement.query_map(
            rusqlite::params![
                table,
                *rows.start() as i64,
                *rows.end() as i64,
                *columns.start() as i64,
                *columns.end() as i64
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as usize,
                    row.get::<_, i64>(1)? as usize,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        mapped.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// A worksheet backed by a DataStore table: lazily fetched, editable within
/// its OWN row bounds (a data sheet can exceed the grid's 1,000 rows —
/// references like `Sales!C:50000` are valid against its table size).
pub struct DataSheet {
    table: String,
    row_count: usize,
    column_count: usize,
    store: Rc<DataStore>,
}

impl DataSheet {
    pub fn new(table: &str, store: Rc<DataStore>) -> Option<Self> {
        let info = store.info(table)?;
        Some(Self {
            table: table.to_string(),
            row_count: info.rows,
            column_count: info.columns.min(Spreadsheet::COLUMN_COUNT),
            store,
        })
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    /// Bounded by the TABLE, not the grid's row count.
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// Capped at the grid's 26 columns.
    pub fn column_count(&self) -> usize {
        self.column_count
    }

    /// Raw stored text (UI display / copy). 0-based.
    pub fn raw_value(&self, row: usize, column: usize) -> String {
        self.store
            .value(&self.table, row, column)
            .unwrap_or_default()
    }

    /// Edits one cell of the imported copy (0-based; within the table's
    /// rectangle — growing the table is a future feature).
    pub fn set_raw_value(&self, value: &str, row: usize, column: usize) -> Result<(), EngineError> {
        if row >= self.row_count || column >= self.column_count {
            return Err(EngineError::domain("cell is outside this data sheet"));
        }
        self.store
            .set_value(Some(value), &self.table, row, column)
            .map_err(|error| EngineError::domain(error.to_string()))
    }

    /// Resolver semantics, mirroring the grid: empty → 0, text → error.
    /// Row is 1-based (reference syntax), bounded by the TABLE, not the grid.
    pub fn numeric_value(&self, column: &str, row: i64) -> Result<BigDecimal, EngineError> {
        let column_index = CellAddress::column_index(column).filter(|&c| c < self.column_count);
        let (Some(column_index), true) = (
            column_index,
            (1..=self.row_count.max(1) as i64).contains(&row),
        ) else {
            return Err(EngineError::domain(format!(
                "cell {column}:{row} is outside this data sheet"
            )));
        };
        let Some(text) = self
            .store
            .value(&self.table, (row - 1) as usize, column_index)
        else {
            return Ok(BigDecimal::zero());
        };
        BigDecimal::parse(&text)
            .ok_or_else(|| EngineError::domain(format!("cell {column}:{row} is not a number")))
    }

    /// Range expansion, grid-consistent: numeric values only, text and empty
    /// skipped. One SQL query regardless of rectangle size.
    pub fn numeric_values(
        &self,
        from_column: &str,
        from_row: i64,
        to_column: &str,
        to_row: i64,
    ) -> Result<Vec<BigDecimal>, EngineError> {
        let (Some(from), Some(to), true) = (
            CellAddress::column_index(from_column),
            CellAddress::column_index(to_column),
            from_row >= 1 && to_row >= 1,
        ) else {
            return Err(EngineError::domain("range is outside this data sheet"));
        };
        let rows = (from_row.min(to_row) - 1) as usize..=(from_row.max(to_row) - 1) as usize;
        let columns = from.min(to)..=from.max(to);
        Ok(self
            .store
            .values(&self.table, rows, columns)
            .map_err(|error| EngineError::domain(error.to_string()))?
            .into_iter()
            .filter_map(|(_, _, value)| BigDecimal::parse(&value)) // text (headers) skipped
            .collect())
    }
}

#[cfg(test)]
mod tests;
