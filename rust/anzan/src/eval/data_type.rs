//! A user-declared record type: `data Person { name: String, age: Number,
//! active: Boolean }`. Like `UserFunction`, the original `source` line is
//! kept for workbook serialization — including any trailing `# doc comment`,
//! which is the type's documentation.
//!
//! Construction goes through the type's CONSTRUCTOR (the type name, called
//! like a function): named fields — `Person(name: "Ada", age: 36, active:
//! true)` — or one map. There is deliberately no positional form (user
//! decision: field names at every call site).
//!
//! Value validation (`DataFieldType::validate`) arrives with the `Value`
//! port; the declaration side lives here because the parser produces it.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct DataType {
    /// As declared — must start with a capital letter. Constructor calls are
    /// case-insensitive, like every function.
    pub name: String,
    /// Declaration order — instances canonicalize their fields to it.
    pub fields: Vec<DataField>,
    pub source: String,
}

impl DataType {
    pub fn new(name: impl Into<String>, fields: Vec<DataField>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields,
            source: source.into(),
        }
    }

    /// The trailing `# …` comment of the declaration, if any — the user's
    /// own documentation, shown by man()/the reference window.
    pub fn documentation(&self) -> Option<String> {
        let hash = self.source.find('#')?;
        let text = self.source[hash + 1..].trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }

    /// Display form: `data Person { name: String, age: Number }`.
    pub fn declaration(&self) -> String {
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|f| format!("{}: {}", f.name, f.field_type.label()))
            .collect();
        format!("data {} {{ {} }}", self.name, fields.join(", "))
    }

    /// "name, age, active" — for error messages.
    pub(crate) fn field_list(&self) -> String {
        self.fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One declared field. Names are case-sensitive (they become map-style keys);
/// duplicates are rejected at parse time.
#[derive(Debug, Clone, PartialEq)]
pub struct DataField {
    pub name: String,
    pub field_type: DataFieldType,
}

impl DataField {
    pub fn new(name: impl Into<String>, field_type: DataFieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
        }
    }
}

/// A field's type: a built-in scalar (Boolean fields hold the engine's 1/0
/// but render/serialize as true/false), `Record(name)` — another declared
/// data type, so records nest (`data Line { a: Point, b: Point }`) — or the
/// composite `List(T)` (`[String]`, `[[Number]]`) and `Map(T)`
/// (`{String: Number}`, a string-keyed map of `T`).
#[derive(Debug, Clone, PartialEq)]
pub enum DataFieldType {
    Number,
    String,
    Boolean,
    /// A declared data type, e.g. Point.
    Record(String),
    /// [T]
    List(Box<DataFieldType>),
    /// {String: T} — string-keyed map of T.
    Map(Box<DataFieldType>),
}

impl DataFieldType {
    /// Parses a LEAF type from a single token (scalar or a data-type name);
    /// the parser handles the composite `[…]` / `{…}` forms. `None` otherwise.
    pub fn parsing(text: &str) -> Option<Self> {
        match text.to_lowercase().as_str() {
            "number" => Some(Self::Number),
            "string" => Some(Self::String),
            "boolean" => Some(Self::Boolean),
            _ => {
                let first = text.chars().next()?;
                if first.is_uppercase() {
                    Some(Self::Record(text.to_string()))
                } else {
                    None
                }
            }
        }
    }

    /// Canonical spelling — `Number` / `Point` / `[String]` /
    /// `{String: Number}`.
    pub fn label(&self) -> String {
        match self {
            Self::Number => "Number".to_string(),
            Self::String => "String".to_string(),
            Self::Boolean => "Boolean".to_string(),
            Self::Record(name) => name.clone(),
            Self::List(element) => format!("[{}]", element.label()),
            Self::Map(value_type) => format!("{{String: {}}}", value_type.label()),
        }
    }

    /// Within a `namespace`, qualify a record-type reference: an unqualified
    /// `Point` mapped by `scope` (a simple lowercased name → its qualified
    /// form, accumulated from the enclosing namespaces) becomes `Bits::Point`;
    /// an already-qualified (`Other::T`) or out-of-scope global name is left
    /// alone. Recurses into list/map element types.
    pub(crate) fn qualified(&self, scope: &HashMap<String, String>) -> DataFieldType {
        match self {
            Self::Number | Self::String | Self::Boolean => self.clone(),
            Self::Record(name) => {
                if !name.contains("::") {
                    if let Some(qualified) = scope.get(&name.to_lowercase()) {
                        return Self::Record(qualified.clone());
                    }
                }
                self.clone()
            }
            Self::List(element) => Self::List(Box::new(element.qualified(scope))),
            Self::Map(value_type) => Self::Map(Box::new(value_type.qualified(scope))),
        }
    }
}

// MARK: - Value validation (the constructor's type checks)

use super::value::{MapEntry, Value};
use crate::EngineError;

impl DataFieldType {
    /// Validates a value against this type, recursing into list/map
    /// elements. Booleans are the engine's 1/0, but a Boolean field is
    /// strict (exactly 0/1, so `active: 7` is caught). Records are
    /// already-validated immutable instances, so a type-name check suffices
    /// (no re-validation, no cycles).
    pub(crate) fn validate(
        &self,
        value: &Value,
        field: &str,
        type_name: &str,
    ) -> Result<Value, EngineError> {
        let mismatch = || {
            EngineError::domain(format!(
                "'{field}' of {type_name} is a {} — got {}",
                self.label(),
                value.kind_name()
            ))
        };
        match self {
            Self::Number => {
                if !matches!(value, Value::Number(_)) {
                    return Err(mismatch());
                }
            }
            Self::String => {
                if !matches!(value, Value::String(_)) {
                    return Err(mismatch());
                }
            }
            Self::Boolean => {
                let ok = matches!(value, Value::Number(flag)
                    if flag.is_zero() || *flag == crate::BigDecimal::one());
                if !ok {
                    return Err(EngineError::domain(format!(
                        "'{field}' of {type_name} is a Boolean — use true or false"
                    )));
                }
            }
            Self::Record(expected) => {
                let ok = matches!(value, Value::Record(record)
                    if record.type_name.eq_ignore_ascii_case(expected));
                if !ok {
                    return Err(mismatch());
                }
            }
            Self::List(element) => {
                let Value::Array(items) = value else {
                    return Err(mismatch());
                };
                let validated: Result<Vec<Value>, EngineError> = items
                    .iter()
                    .map(|item| element.validate(item, field, type_name))
                    .collect();
                return Ok(Value::Array(validated?));
            }
            Self::Map(value_type) => {
                let Value::Map(entries) = value else {
                    return Err(mismatch());
                };
                let validated: Result<Vec<MapEntry>, EngineError> = entries
                    .iter()
                    .map(|e| {
                        Ok(MapEntry::new(
                            e.key.clone(),
                            value_type.validate(&e.value, field, type_name)?,
                        ))
                    })
                    .collect();
                return Ok(Value::Map(validated?));
            }
        }
        Ok(value.clone())
    }
}
