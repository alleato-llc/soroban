//! The function library — one submodule per category list, all merged into
//! `FunctionRegistry::standard()`. Ports of
//! `swift/Engine/Sources/Anzan/Functions/*.swift`, arriving list by list.

use super::registry::BuiltinFunction;

/// Every builtin list, merged by the registry at first use. Lists land here
/// as they're ported; the registry asserts name uniqueness across all of
/// them.
pub(crate) fn all_function_lists() -> Vec<Vec<BuiltinFunction>> {
    vec![]
}
