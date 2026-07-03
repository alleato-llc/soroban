//! One grid coordinate. `column` 0-based (0 = A); `row` 0-based internally,
//! rendered 1-based ("A:1") to match the formula syntax.
//!
//! ALL name↔index and 0-vs-1-based conversions live here — don't
//! re-implement column-letter or "A:1"-key parsing anywhere else.

use crate::spreadsheet::Spreadsheet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellAddress {
    pub column: usize,
    pub row: usize,
}

impl CellAddress {
    pub fn new(column: usize, row: usize) -> Self {
        Self { column, row }
    }

    /// From the user-facing forms: column name + 1-based row, bounds-checked.
    pub fn from_column_name(column_name: &str, row_number: i64) -> Option<Self> {
        let column = Self::column_index(column_name)?;
        if !(1..=Spreadsheet::ROW_COUNT as i64).contains(&row_number) {
            return None;
        }
        Some(Self::new(column, (row_number - 1) as usize))
    }

    /// From a serialization key ("A:1"), as used in workbook files.
    pub fn from_key(key: &str) -> Option<Self> {
        let (column, row) = key.split_once(':')?;
        Self::from_column_name(column, row.parse().ok()?)
    }

    /// "A" → 0, case-insensitive on the way in.
    pub fn column_index(name: &str) -> Option<usize> {
        let mut chars = name.chars();
        let first = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        let upper = first.to_ascii_uppercase();
        let value = upper as u32;
        if (65..65 + Spreadsheet::COLUMN_COUNT as u32).contains(&value) {
            Some((value - 65) as usize)
        } else {
            None
        }
    }

    pub fn column_name_for(index: usize) -> String {
        char::from_u32(65 + index as u32)
            .expect("column index in range")
            .to_string()
    }

    pub fn column_name(&self) -> String {
        Self::column_name_for(self.column)
    }

    /// 1-based, as displayed and serialized.
    pub fn row_number(&self) -> usize {
        self.row + 1
    }
}

impl fmt::Display for CellAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.column_name(), self.row_number())
    }
}
