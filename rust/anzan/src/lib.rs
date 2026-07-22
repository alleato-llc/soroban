//! The Anzan language (暗算 — mental abacus calculation): lexer, parser,
//! evaluator, exact numbers, the function library, and the `Calculator`
//! facade. Knows nothing about grids or files — hosts wire cells in through
//! the calculator's resolver hooks.
//!
//! This is the Rust implementation of the language specified by
//! `spec/anzan/*.feature` and `docs/ANZAN.md`; the Swift implementation in
//! `swift/Engine/Sources/Anzan` is its reference. Behavior changes land in
//! the spec first, then in both implementations.

pub mod ast;
mod calculator;
pub mod documentation;
pub mod error;
pub mod eval;
pub mod lexer;
pub mod mode;
pub mod number;
pub mod parser;
pub mod script;

pub use calculator::{Calculator, Completion, CompletionKind, EvalOutcome, FunctionDoc};
pub use documentation::DocCategory;
pub use error::EngineError;
pub use eval::binary_format::{BinaryEditorBits, BinaryEditorPalette, BinaryEditorPresets};
pub use eval::binary_view::{
    BinaryView, Field as BinaryField, FieldSpec as BinaryFieldSpec, Kind as BinaryViewKind,
    Unavailable as BinaryViewUnavailable, EDITABLE_WIDTHS as BINARY_EDITABLE_WIDTHS,
};
pub use eval::data_type::{DataField, DataFieldType, DataType};
pub use eval::environment::{EvaluationEnvironment, UserFunction};
pub use eval::evaluator::{Evaluator, Locals, Reentry, Resolvers};
pub use eval::format_builder::{
    Field as FormatBuilderField, FieldKind as FormatBuilderFieldKind, FormatBuilder,
};
pub use eval::registry::FunctionRegistry;
pub use eval::value::{FunctionValue, HostObject, MapEntry, RecordValue, Value};
pub use mode::LanguageMode;
pub use number::{BigDecimal, PrecisionContext};
pub use parser::Parser;
pub use script::{Statement, StatementAccumulator};
