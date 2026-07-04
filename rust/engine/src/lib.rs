//! The HOSTING layer — the port of `swift/Engine/Sources/SorobanEngine`:
//! the spreadsheet model (`Spreadsheet`, `SheetStore`, cells, the dependency
//! graph, controls, named cells) and workbook persistence. Re-exports
//! `anzan` (the Swift side's `@_exported import`), so depending on
//! `soroban-engine` gives the whole engine; apps never depend on `anzan`
//! directly.
//!
//! Anzan knows NOTHING about grids or files — this crate wires cells in
//! through the Calculator's resolver hooks. Don't add a sheet/persistence
//! dependency to `anzan`.

pub use anzan::*;

pub mod cell;
pub mod cell_address;
pub mod cell_format;
pub mod context;
pub mod controls;
pub mod csv;
pub mod data_store;
pub mod history_reflection;
pub mod journal;
pub mod named_cells;
pub mod package;
pub mod reference_rewriter;
pub(crate) mod reflection;
pub mod sheet_store;
pub mod spreadsheet;
pub mod structure;
pub mod workbook;

pub use cell::Cell;
pub use cell_address::CellAddress;
pub use cell_format::{CellAlignment, CellFormat, NumberFormat, PaletteColor};
pub use controls::{CheckboxInfo, Control, DropdownInfo, SliderInfo};
pub use data_store::{DataSheet, DataStore};
pub use sheet_store::{Sheet, SheetStore};
pub use spreadsheet::{CellDisplay, Spreadsheet};
pub use structure::{CellRewrite, StructuralChange};
pub use workbook::Workbook;
