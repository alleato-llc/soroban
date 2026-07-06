//! Free helper functions shared across the evaluator's submodules: namespace
//! name arithmetic and the type/operator lookups the dispatch and call paths
//! lean on.

use crate::ast::{BinaryOperator, TypeAnnotation};
use crate::eval::value::Value;

/// The namespace a qualified name lives in — `Bits::area` → `Bits`,
/// `A::B::area` → `A::B`, a plain name → `None`. The prefix before the LAST
/// `::`, so a nested member's home is its immediate (innermost) namespace.
pub(super) fn home_namespace(name: &str) -> Option<&str> {
    name.rfind("::").map(|i| &name[..i])
}

/// Qualified candidates for an unqualified `name` seen inside `namespace`,
/// walking UP the nesting chain: in `A::B`, `c` is tried as `A::B::c` then
/// `A::c` (then the caller falls through to global). Empty when there's no
/// home context or the name is already qualified. The single source of truth
/// for sibling resolution — `.variable`, `call`, and `tail_step` all iterate
/// it, so they stay in sync.
pub(super) fn sibling_candidates(name: &str, namespace: Option<&str>) -> Vec<String> {
    let Some(namespace) = namespace else {
        return Vec::new();
    };
    if name.contains("::") {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let mut prefix = namespace;
    loop {
        candidates.push(format!("{prefix}::{name}"));
        let Some(separator) = prefix.rfind("::") else {
            break;
        };
        prefix = &prefix[..separator];
    }
    candidates
}

/// Does a runtime value satisfy a parameter's type annotation? Booleans are
/// numbers in Anzan, so `Boolean` matches a number; a named type matches a
/// record of that type (case-insensitive, like the call namespace).
pub(super) fn type_matches(value: &Value, annotation: &TypeAnnotation) -> bool {
    match (annotation, value) {
        (TypeAnnotation::Number, Value::Number(_))
        | (TypeAnnotation::Boolean, Value::Number(_)) => true,
        (TypeAnnotation::String, Value::String(_)) => true,
        (TypeAnnotation::Named(type_name), Value::Record(record)) => {
            record.type_name.eq_ignore_ascii_case(type_name)
        }
        _ => false,
    }
}

/// A binary operator by its symbol — the operator-overload definition check
/// (the Swift `BinaryOperator(rawValue:)`).
pub(super) fn binary_operator_named(name: &str) -> Option<BinaryOperator> {
    match name {
        "+" => Some(BinaryOperator::Add),
        "-" => Some(BinaryOperator::Subtract),
        "*" => Some(BinaryOperator::Multiply),
        "/" => Some(BinaryOperator::Divide),
        "%" => Some(BinaryOperator::Modulo),
        "^" => Some(BinaryOperator::Power),
        _ => None,
    }
}
