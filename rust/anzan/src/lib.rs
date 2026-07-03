//! The Anzan language (暗算 — mental abacus calculation): lexer, parser,
//! evaluator, exact numbers, the function library, and the `Calculator`
//! facade. Knows nothing about grids or files — hosts wire cells in through
//! the calculator's resolver hooks.
//!
//! This is the Rust implementation of the language specified by
//! `spec/anzan/*.feature` and `docs/ANZAN.md`; the Swift implementation in
//! `swift/Engine/Sources/Anzan` is its reference. Behavior changes land in
//! the spec first, then in both implementations.

mod calculator;
pub mod number;
pub mod error;
pub mod mode;

pub use calculator::{Calculator, EvalOutcome};
pub use error::EngineError;
pub use mode::LanguageMode;
pub use number::{BigDecimal, PrecisionContext};
