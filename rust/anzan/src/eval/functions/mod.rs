//! The function library — one submodule per category list, all merged into
//! `FunctionRegistry::standard()`. Ports of
//! `swift/Engine/Sources/Anzan/Functions/*.swift`.

pub(crate) mod accounting;
pub(crate) mod controls;
pub(crate) mod core;
pub(crate) mod data;
pub(crate) mod dates;
pub(crate) mod finance;
pub(crate) mod programmer;
pub(crate) mod stats;
pub(crate) mod trig;

use super::registry::BuiltinFunction;

/// Every builtin list, merged by the registry at first use. The registry
/// asserts name uniqueness across all of them.
pub(crate) fn all_function_lists() -> Vec<Vec<BuiltinFunction>> {
    vec![
        core::list(),
        trig::list(),
        finance::list(),
        accounting::list(),
        stats::list(),
        dates::list(),
        data::list(),
        programmer::list(),
        controls::list(),
    ]
}
