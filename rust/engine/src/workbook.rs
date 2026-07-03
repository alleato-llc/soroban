//! The `.soroban` workbook file payload — the port of
//! `swift/Engine/Sources/SorobanEngine/Persistence/Workbook.swift` (+ the
//! `Calculator.restoreSession(from:)` extension from Calculator+Workbook.swift):
//! a versioned JSON envelope holding raw cell contents and user variables
//! (cells + the variables their formulas depend on make a workbook
//! self-contained). Pure data — file I/O and panels live in the app layer.
//!
//! The JSON schema is the interchange contract (docs/FORMAT.md): a workbook
//! saved by the Swift app must decode here, and vice versa. Field names,
//! nesting, and decode defaults mirror Swift's Codable implementation exactly.

use anzan::{Calculator, DataType, UserFunction, Value};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// `Workbook.formatIdentifier`.
pub const FORMAT_IDENTIFIER: &str = "soroban-workbook";
/// `Workbook.currentVersion` — v2: `functions` became an ordered list of
/// source lines (was a name→source map) to carry typed operator/function
/// overloads.
pub const CURRENT_VERSION: i64 = 2;

/// One worksheet's payload (`Workbook.SheetPayload`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetPayload {
    pub name: String,
    /// "A:1" → raw cell content, exactly as typed (markers included).
    /// Empty for data sheets (their values live in data.sqlite).
    #[serde(default)]
    pub cells: HashMap<String, String>,
    /// "data" marks a sheet backed by a table in the package's data.sqlite;
    /// None/absent means a normal grid sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The data.sqlite table backing a data sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    /// Non-default column widths, keyed by column name ("A") in points.
    #[serde(default, rename = "columnWidths")]
    pub column_widths: HashMap<String, f64>,
    /// Non-default row heights, keyed by 1-based row number ("5") in points.
    #[serde(default, rename = "rowHeights")]
    pub row_heights: HashMap<String, f64>,
    /// Per-cell presentation, keyed "A:1" — only non-default formats.
    /// Decodes to empty for files written before formatting existed.
    /// TODO: type this as `CellFormat` once cell_format.rs lands (being
    /// ported concurrently); until then a raw-JSON passthrough keeps
    /// round-trips lossless without depending on the type.
    #[serde(default)]
    pub formats: HashMap<String, serde_json::Value>,
    /// Named cells, keyed "A:1" → the name ('Projected Rate' syntax).
    #[serde(default)]
    pub names: HashMap<String, String>,
}

impl SheetPayload {
    pub fn new(name: impl Into<String>, cells: HashMap<String, String>) -> Self {
        Self {
            name: name.into(),
            cells,
            kind: None,
            table: None,
            column_widths: HashMap::new(),
            row_heights: HashMap::new(),
            formats: HashMap::new(),
            names: HashMap::new(),
        }
    }

    pub fn is_data(&self) -> bool {
        self.kind.as_deref() == Some("data")
    }
}

/// The `.soroban` workbook envelope.
///
/// Serialization notes (matching Swift's custom `encode(to:)`): `format`,
/// `version`, `sheets`, `variables`, `functions`, and `dataTypes` always
/// encode; `activeSheet` encodes only when present; `namespaces`/`imports`
/// encode only when non-empty. The legacy flat single-sheet fields
/// (`cells`/`columnWidths`/`rowHeights`) are read-only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Workbook {
    pub format: String,
    pub version: i64,
    /// Ordered worksheets. Always at least one after decoding.
    pub sheets: Vec<SheetPayload>,
    /// Which sheet was active when saved.
    #[serde(rename = "activeSheet", skip_serializing_if = "Option::is_none")]
    pub active_sheet: Option<String>,
    /// Variable name → value via `Value`'s canonical string (round-trips
    /// exactly — numbers as before, structures as their canonical literals).
    pub variables: HashMap<String, String>,
    /// Function definition lines ("f(x) = x * 2"), in order — re-evaluated on
    /// open. A list, not a name→source map, because one name can have several
    /// typed overloads. Decodes a legacy name→source object too
    /// (pre-overload files).
    pub functions: Vec<String>,
    /// Data type name → original declaration line ("data Person { … }").
    /// Re-evaluated on open BEFORE variables (record variables persist as
    /// constructor calls). Decodes to empty for older files. Excludes
    /// namespace members (qualified `Bits::BitField` names) — those restore
    /// via `namespaces`.
    #[serde(rename = "dataTypes")]
    pub data_types: HashMap<String, String>,
    /// Namespace declaration lines ("namespace Bits { … }"), in order —
    /// replayed on open to re-register their (qualified) members. Decodes
    /// empty for older files. (docs/MODULES.md 2c)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<String>,
    /// Imported namespace names, restored (after `namespaces`) by replaying
    /// `import Name`. Decodes empty for older files.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
}

/// v2+: an ordered list of source lines. Legacy (v1): a name→source map,
/// flattened in key order (mirrors Swift's `sorted { $0.key < $1.key }`).
#[derive(Deserialize)]
#[serde(untagged)]
enum FunctionsField {
    Lines(Vec<String>),
    Legacy(std::collections::BTreeMap<String, String>),
}

/// The raw wire shape, including the legacy flat single-sheet fields —
/// mirrors Swift's `init(from decoder:)`.
#[derive(Deserialize)]
struct WorkbookWire {
    format: String,
    version: i64,
    variables: HashMap<String, String>,
    #[serde(default)]
    functions: Option<FunctionsField>,
    #[serde(default, rename = "dataTypes")]
    data_types: HashMap<String, String>,
    #[serde(default)]
    namespaces: Vec<String>,
    #[serde(default)]
    imports: Vec<String>,
    #[serde(default, rename = "activeSheet")]
    active_sheet: Option<String>,
    #[serde(default)]
    sheets: Option<Vec<SheetPayload>>,
    // Legacy flat single-sheet fields (read-only).
    #[serde(default)]
    cells: Option<HashMap<String, String>>,
    #[serde(default, rename = "columnWidths")]
    column_widths: Option<HashMap<String, f64>>,
    #[serde(default, rename = "rowHeights")]
    row_heights: Option<HashMap<String, f64>>,
}

impl<'de> Deserialize<'de> for Workbook {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WorkbookWire::deserialize(deserializer)?;
        let functions = match wire.functions {
            Some(FunctionsField::Lines(lines)) => lines,
            // BTreeMap iterates in key order — Swift sorts the legacy map's
            // keys before flattening to values.
            Some(FunctionsField::Legacy(map)) => map.into_values().collect(),
            None => Vec::new(),
        };
        let sheets = match wire.sheets {
            Some(decoded) if !decoded.is_empty() => decoded,
            _ => {
                // Legacy flat format: the whole file was one implicit sheet.
                let mut sheet = SheetPayload::new("Sheet 1", wire.cells.unwrap_or_default());
                sheet.column_widths = wire.column_widths.unwrap_or_default();
                sheet.row_heights = wire.row_heights.unwrap_or_default();
                vec![sheet]
            }
        };
        // Swift's `decode(_:forKey:)` fails on a missing key; serde has
        // already enforced `format`/`version`/`variables` the same way.
        Ok(Workbook {
            format: wire.format,
            version: wire.version,
            sheets,
            active_sheet: wire.active_sheet,
            variables: wire.variables,
            functions,
            data_types: wire.data_types,
            namespaces: wire.namespaces,
            imports: wire.imports,
        })
    }
}

impl Workbook {
    /// The primary constructor (Swift's `init(sheets:activeSheet:variables:
    /// functions:dataTypes:namespaces:imports:)`). Namespace members carry
    /// qualified names; they restore via `namespaces`, so they're kept out of
    /// the flat variable/function/type maps.
    pub fn new(
        sheets: Vec<SheetPayload>,
        active_sheet: Option<String>,
        variables: &HashMap<String, Value>,
        functions: &[UserFunction],
        data_types: &HashMap<String, DataType>,
        namespaces: Vec<String>,
        imports: Vec<String>,
    ) -> Self {
        Self {
            format: FORMAT_IDENTIFIER.to_string(),
            version: CURRENT_VERSION,
            sheets,
            active_sheet,
            variables: variables
                .iter()
                .filter(|(k, _)| !k.contains("::"))
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
            functions: functions
                .iter()
                .filter(|f| !f.name.contains("::"))
                .map(|f| f.source.clone())
                .collect(),
            data_types: data_types
                .values()
                .filter(|t| !t.name.contains("::"))
                .map(|t| (t.name.clone(), t.source.clone()))
                .collect(),
            namespaces,
            imports,
        }
    }

    /// Single-sheet convenience (tests, simple tooling).
    pub fn single_sheet(
        cells: HashMap<String, String>,
        variables: &HashMap<String, Value>,
        functions: &[UserFunction],
        column_widths: HashMap<String, f64>,
        row_heights: HashMap<String, f64>,
    ) -> Self {
        let mut sheet = SheetPayload::new("Sheet 1", cells);
        sheet.column_widths = column_widths;
        sheet.row_heights = row_heights;
        Self::new(
            vec![sheet],
            None,
            variables,
            functions,
            &HashMap::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    /// Back-compat view of the first sheet (kept for older call sites).
    pub fn cells(&self) -> &HashMap<String, String> {
        static EMPTY: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
        self.sheets
            .first()
            .map(|s| &s.cells)
            .unwrap_or_else(|| EMPTY.get_or_init(HashMap::new))
    }

    /// Parsed variables; entries that fail to parse are dropped (they could
    /// only come from a hand-edited file). Numbers take the fast path;
    /// structured values re-parse from their canonical literals.
    pub fn parsed_variables(&self) -> HashMap<String, Value> {
        self.variables
            .iter()
            .filter_map(|(k, v)| Value::parsing(v).map(|value| (k.clone(), value)))
            .collect()
    }

    // MARK: Codec

    /// Encodes as pretty-printed, sorted-key JSON — diffable and
    /// hand-editable, like Swift's `[.prettyPrinted, .sortedKeys]`. (Routing
    /// through `serde_json::Value` sorts object keys: its map is a BTreeMap.)
    pub fn encode(&self) -> Result<Vec<u8>, WorkbookError> {
        let value = serde_json::to_value(self).map_err(|_| WorkbookError::NotAWorkbook)?;
        serde_json::to_vec_pretty(&value).map_err(|_| WorkbookError::NotAWorkbook)
    }

    /// Decodes and validates: anything unparseable is "not a workbook";
    /// files from the future are rejected with a clear message.
    pub fn decode(data: &[u8]) -> Result<Workbook, WorkbookError> {
        let workbook: Workbook =
            serde_json::from_slice(data).map_err(|_| WorkbookError::NotAWorkbook)?;
        if workbook.format != FORMAT_IDENTIFIER {
            return Err(WorkbookError::NotAWorkbook);
        }
        if workbook.version > CURRENT_VERSION {
            return Err(WorkbookError::UnsupportedVersion(workbook.version));
        }
        Ok(workbook)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkbookError {
    NotAWorkbook,
    UnsupportedVersion(i64),
}

impl fmt::Display for WorkbookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkbookError::NotAWorkbook => write!(f, "This file is not a Soroban workbook."),
            WorkbookError::UnsupportedVersion(version) => write!(
                f,
                "This workbook uses format version {version}, which needs a newer version of Soroban."
            ),
        }
    }
}

impl std::error::Error for WorkbookError {}

// MARK: Session restore (Calculator+Workbook.swift)

/// The workbook half of session restore. Lives on the HOSTING side of the
/// module boundary: `Calculator` (anzan) knows nothing about `Workbook` —
/// this function is the one place the two meet.
///
/// Replaces the session's definitions and variables from a workbook.
/// Order matters: namespaces (which register their qualified members) →
/// imports (which need those namespaces) → data types → functions →
/// variables (a persisted record variable is a constructor CALL and needs
/// its type back first; a variable may use an imported name). `ans` is
/// never touched.
pub fn restore_session(calculator: &mut Calculator, workbook: &Workbook) {
    let environment = calculator.environment_mut();
    environment.replace_user_functions(HashMap::new());
    environment.replace_user_data_types(HashMap::new());
    environment.clear_imports();
    environment.clear_namespace_sources();
    environment.clear_namespace_variables();
    for source in &workbook.namespaces {
        // Re-registers the namespace's members; re-records the source.
        let _ = calculator.evaluate(source);
    }
    for namespace in &workbook.imports {
        let _ = calculator.evaluate(&format!("import {namespace}"));
    }
    let mut data_type_sources: Vec<&String> = workbook.data_types.values().collect();
    data_type_sources.sort();
    for source in data_type_sources {
        // Bad hand-edited lines are dropped.
        let _ = calculator.evaluate(source);
    }
    let mut function_sources: Vec<&String> = workbook.functions.iter().collect();
    function_sources.sort();
    for source in function_sources {
        let _ = calculator.evaluate(source);
    }
    calculator.restore_variables(&workbook.variables);
}

#[cfg(test)]
mod tests {
    use super::*;
    use anzan::EvalOutcome;

    fn decode_fixture() -> Workbook {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/mortgage.soroban"
        );
        let data = std::fs::read(path).expect("fixture readable");
        Workbook::decode(&data).expect("fixture decodes")
    }

    /// The Rust half of the interchange proof: a REAL Swift-written workbook
    /// decodes, sheets and all.
    #[test]
    fn decodes_swift_written_fixture() {
        let workbook = decode_fixture();
        let names: Vec<&str> = workbook.sheets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Loan", "What If"]);
        assert_eq!(workbook.version, 1);
        assert_eq!(workbook.active_sheet.as_deref(), Some("Loan"));
        // v1 legacy name→source functions map flattens to sorted source lines.
        assert_eq!(workbook.functions.len(), 1);
        assert!(workbook.functions[0].starts_with("monthly(apr, years, principal)"));
        assert_eq!(workbook.sheets[0].cells["B:1"], "350000");
        assert_eq!(workbook.sheets[0].column_widths["A"], 150.0);
        assert!(workbook.sheets[1].row_heights.is_empty());
    }

    #[test]
    fn fixture_restores_into_a_calculator() {
        let workbook = decode_fixture();
        let mut calculator = Calculator::new();
        restore_session(&mut calculator, &workbook);
        // The workbook's function is live again (self-contained file).
        match calculator.evaluate("monthly(0.065, 30, 350000)") {
            Ok(EvalOutcome::Value(_)) => {}
            other => panic!("expected a value, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_through_encode() {
        let workbook = decode_fixture();
        let encoded = workbook.encode().expect("encodes");
        let again = Workbook::decode(&encoded).expect("re-decodes");
        assert_eq!(workbook, again);
    }

    #[test]
    fn rejects_future_versions_and_foreign_files() {
        let future = br#"{"format":"soroban-workbook","version":99,"variables":{}}"#;
        assert_eq!(
            Workbook::decode(future),
            Err(WorkbookError::UnsupportedVersion(99))
        );
        let foreign = br#"{"format":"something-else","version":1,"variables":{}}"#;
        assert_eq!(Workbook::decode(foreign), Err(WorkbookError::NotAWorkbook));
        assert_eq!(
            Workbook::decode(b"not json"),
            Err(WorkbookError::NotAWorkbook)
        );
    }

    #[test]
    fn legacy_flat_single_sheet_still_opens() {
        let flat = br#"{
            "format": "soroban-workbook", "version": 1,
            "cells": {"A:1": "42"}, "columnWidths": {"A": 120},
            "variables": {"x": "1.5"}
        }"#;
        let workbook = Workbook::decode(flat).expect("legacy decodes");
        assert_eq!(workbook.sheets.len(), 1);
        assert_eq!(workbook.sheets[0].name, "Sheet 1");
        assert_eq!(workbook.sheets[0].cells["A:1"], "42");
        assert_eq!(workbook.sheets[0].column_widths["A"], 120.0);
        let parsed = workbook.parsed_variables();
        assert_eq!(parsed["x"].to_string(), "1.5");
    }

    #[test]
    fn missing_variables_is_not_a_workbook() {
        // Swift's decode requires `variables`; keep the strictness identical.
        let no_vars = br#"{"format":"soroban-workbook","version":1}"#;
        assert_eq!(Workbook::decode(no_vars), Err(WorkbookError::NotAWorkbook));
    }

    #[test]
    fn formats_pass_through_losslessly() {
        let with_formats = br#"{
            "format": "soroban-workbook", "version": 2, "variables": {},
            "sheets": [{"name": "S", "cells": {},
                        "formats": {"A:1": {"bold": true, "style": "currency",
                                            "decimals": 2, "symbol": "$"}}}]
        }"#;
        let workbook = Workbook::decode(with_formats).expect("decodes");
        let encoded = workbook.encode().expect("encodes");
        let again = Workbook::decode(&encoded).expect("re-decodes");
        assert_eq!(
            again.sheets[0].formats["A:1"]["symbol"],
            serde_json::Value::String("$".into())
        );
        assert_eq!(workbook, again);
    }

    #[test]
    fn encode_omits_empty_namespaces_and_imports() {
        let workbook = Workbook::single_sheet(
            HashMap::new(),
            &HashMap::new(),
            &[],
            HashMap::new(),
            HashMap::new(),
        );
        let text = String::from_utf8(workbook.encode().unwrap()).unwrap();
        assert!(!text.contains("\"namespaces\""));
        assert!(!text.contains("\"imports\""));
        assert!(!text.contains("\"activeSheet\""));
        assert!(text.contains("\"dataTypes\""));
    }
}
