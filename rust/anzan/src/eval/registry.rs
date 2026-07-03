//! Case-insensitive lookup of every built-in function. A `BuiltinFunction`
//! carries its arity contract, implementation, AND documentation — the doc
//! fields are deliberately required, so a function cannot be registered
//! without a signature, summary, and examples, and the documentation tests
//! evaluate every example.

use super::value::Value;
use crate::{BigDecimal, EngineError};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Where a function appears in the reference window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCategory {
    Core,
    Logic,
    Trig,
    Finance,
    Dates,
    Accounting,
    Stats,
    Data,
    Programmer,
    Controls,
}

impl FunctionCategory {
    pub const ALL: [FunctionCategory; 10] = [
        Self::Core,
        Self::Logic,
        Self::Trig,
        Self::Finance,
        Self::Dates,
        Self::Accounting,
        Self::Stats,
        Self::Data,
        Self::Programmer,
        Self::Controls,
    ];

    /// The reference-window heading.
    pub fn heading(&self) -> &'static str {
        match self {
            Self::Core => "Core & Algebra",
            Self::Logic => "Logic",
            Self::Trig => "Trigonometry",
            Self::Finance => "Finance",
            Self::Dates => "Dates",
            Self::Accounting => "Accounting",
            Self::Stats => "Statistics",
            Self::Data => "Data & Text",
            Self::Programmer => "Programmer",
            Self::Controls => "Controls",
        }
    }

    /// Single-word module name for `Module::builtin` qualified access (the
    /// heading isn't a valid identifier).
    pub fn module_name(&self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Logic => "Logic",
            Self::Trig => "Trig",
            Self::Finance => "Finance",
            Self::Dates => "Dates",
            Self::Accounting => "Accounting",
            Self::Stats => "Stats",
            Self::Data => "Data",
            Self::Programmer => "Programmer",
            Self::Controls => "Controls",
        }
    }
}

/// Applies a function value to arguments — how higher-order builtins call
/// back into the evaluator (which owns environment + depth).
pub type Applier<'a> = &'a mut dyn FnMut(&Value, &[Value]) -> Result<Value, EngineError>;

type NumericFn = fn(&[BigDecimal]) -> Result<BigDecimal, EngineError>;
type ValuesFn = fn(&[Value]) -> Result<Value, EngineError>;
type HigherOrderFn = fn(&[Value], Applier<'_>) -> Result<Value, EngineError>;

/// Most builtins are numeric: array arguments flatten in place exactly like
/// cell ranges (`sum(arr)` ≡ `sum(A:1..A:9)`), and arity is checked AFTER
/// flattening. Value-level builtins (len, keys, …) see structures as-is.
/// Higher-order builtins (map, filter, reduce) additionally receive the
/// applier.
pub enum Implementation {
    Numeric(NumericFn),
    Values(ValuesFn),
    HigherOrder(HigherOrderFn),
}

/// Accepted argument counts: `lo..=hi`; `hi == usize::MAX` means variadic.
pub type Arity = std::ops::RangeInclusive<usize>;

pub struct BuiltinFunction {
    pub name: &'static str,
    pub category: FunctionCategory,
    pub signature: &'static str,
    pub summary: &'static str,
    pub examples: &'static [&'static str],
    pub arity: Arity,
    pub implementation: Implementation,
}

impl BuiltinFunction {
    /// Human-readable arity for error messages: "1", "1 to 2",
    /// "at least 2".
    pub fn arity_description(&self) -> String {
        let (lo, hi) = (*self.arity.start(), *self.arity.end());
        if lo == hi {
            return lo.to_string();
        }
        if hi == usize::MAX {
            return format!("at least {lo}");
        }
        format!("{lo} to {hi}")
    }
}

pub struct FunctionRegistry {
    functions: HashMap<String, BuiltinFunction>,
}

impl FunctionRegistry {
    /// The one shared registry — every builtin list merged, names asserted
    /// unique at first use.
    pub fn standard() -> &'static FunctionRegistry {
        static STANDARD: OnceLock<FunctionRegistry> = OnceLock::new();
        STANDARD.get_or_init(|| {
            let mut registry = FunctionRegistry {
                functions: HashMap::new(),
            };
            for list in super::functions::all_function_lists() {
                registry.register(list);
            }
            registry
        })
    }

    fn register(&mut self, list: Vec<BuiltinFunction>) {
        for function in list {
            let key = function.name.to_lowercase();
            assert!(
                !self.functions.contains_key(&key),
                "duplicate function {}",
                function.name
            );
            self.functions.insert(key, function);
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(&name.to_lowercase())
    }

    /// Looks up, checks arity, and applies. Numeric builtins flatten array
    /// arguments first (the structure analogue of range expansion), so the
    /// arity check sees the flattened count — `max(arr)` works like
    /// `max(A:1..A:9)`. The applier comes from the evaluator; higher-order
    /// builtins use it to invoke their function arguments.
    pub fn call(
        &self,
        name: &str,
        arguments: &[Value],
        applier: Applier<'_>,
    ) -> Result<Value, EngineError> {
        let Some(function) = self.functions.get(&name.to_lowercase()) else {
            return Err(EngineError::UnknownFunction {
                name: name.to_string(),
            });
        };
        match &function.implementation {
            Implementation::Numeric(apply) => {
                let mut numbers = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    numbers.extend(argument.flattened_numbers(function.name)?);
                }
                if !function.arity.contains(&numbers.len()) {
                    return Err(EngineError::ArityMismatch {
                        function: function.name.to_string(),
                        expected: function.arity_description(),
                        got: numbers.len(),
                    });
                }
                Ok(Value::Number(apply(&numbers)?))
            }
            Implementation::Values(apply) => {
                if !function.arity.contains(&arguments.len()) {
                    return Err(EngineError::ArityMismatch {
                        function: function.name.to_string(),
                        expected: function.arity_description(),
                        got: arguments.len(),
                    });
                }
                apply(arguments)
            }
            Implementation::HigherOrder(apply) => {
                if !function.arity.contains(&arguments.len()) {
                    return Err(EngineError::ArityMismatch {
                        function: function.name.to_string(),
                        expected: function.arity_description(),
                        got: arguments.len(),
                    });
                }
                apply(arguments, applier)
            }
        }
    }

    /// All function names, for UI listing/autocomplete.
    pub fn names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.functions.values().map(|f| f.name).collect();
        names.sort_unstable();
        names
    }

    /// Every registered function, for the reference window.
    pub fn all(&self) -> Vec<&BuiltinFunction> {
        let mut all: Vec<&BuiltinFunction> = self.functions.values().collect();
        all.sort_by_key(|f| f.name.to_lowercase());
        all
    }

    pub fn function(&self, name: &str) -> Option<&BuiltinFunction> {
        self.functions.get(&name.to_lowercase())
    }

    /// Is `name` a builtin module (a category)? — used to treat
    /// `import Finance` as a no-op (its members are already in the global
    /// prelude).
    pub fn is_module(&self, name: &str) -> bool {
        FunctionCategory::ALL
            .iter()
            .any(|c| c.module_name().eq_ignore_ascii_case(name))
    }

    /// Resolve a qualified builtin `Module::name` → the bare builtin name
    /// when that builtin exists AND belongs to that module
    /// (`Finance::pmt` → `pmt`, `Finance::sqrt` → `None`). The bare name
    /// stays globally available too (the prelude); the qualified form is an
    /// additive, disambiguating alias.
    pub fn resolve_qualified(&self, qualified: &str) -> Option<String> {
        let (module, name) = qualified.split_once("::")?;
        let function = self.function(name)?;
        if function.category.module_name().eq_ignore_ascii_case(module) {
            Some(name.to_string())
        } else {
            None
        }
    }
}
