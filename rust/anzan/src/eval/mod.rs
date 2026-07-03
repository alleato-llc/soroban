//! The evaluation layer: values, the environment, the evaluator, the
//! function registry, and the declared-type machinery.

pub mod data_type;
pub mod environment;
pub mod evaluator;
pub mod fixed_decimal;
pub mod fixed_int;
pub mod functions;
pub(crate) mod json;
pub(crate) mod numeric;
pub mod registry;
pub mod value;
