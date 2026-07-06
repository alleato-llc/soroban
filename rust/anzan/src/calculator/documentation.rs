//! Documentation lookups: one name's `man` page and the full reference
//! catalogue (built-ins plus the live environment's functions/data types).

use super::{Calculator, FunctionDoc};
use crate::eval::data_type::DataType;
use crate::eval::environment::UserFunction;
use crate::eval::registry::FunctionRegistry;

impl Calculator {
    /// One function's documentation, for man/the autocomplete hint footer.
    /// Covers built-ins, the user's own functions and data types, and
    /// sheet-scoped λ cells. (The Swift side also curates special-form/
    /// operator/constant entries — ported with Documentation.swift.)
    pub fn documentation_for(&self, name: &str) -> Option<FunctionDoc> {
        if let Some(builtin) = FunctionRegistry::standard().function(name) {
            return Some(FunctionDoc {
                name: builtin.name.to_string(),
                signature: builtin.signature.to_string(),
                summary: builtin.summary.to_string(),
                examples: builtin.examples.iter().map(|e| e.to_string()).collect(),
            });
        }
        // Curated entries, in the Swift lookup order: special forms, then
        // constants (man pi, man Json, …), then operator/syntax pages
        // (man modes, man arithmetic, …).
        for curated in [
            crate::documentation::special_forms(),
            crate::documentation::constants(),
            crate::documentation::operators(),
        ] {
            if let Some(entry) = curated
                .into_iter()
                .find(|d| d.name.eq_ignore_ascii_case(name))
            {
                return Some(entry);
            }
        }
        if let Some(resolve) = &self.resolvers.scoped_function {
            if let Some(scoped) = resolve(name) {
                return Some(Self::doc_for_user(&scoped));
            }
        }
        if let Some(user) = self.environment.function(name) {
            return Some(Self::doc_for_user(user));
        }
        if let Some(data_type) = self.environment.data_type(name) {
            return Some(Self::doc_for_type(data_type));
        }
        None
    }

    /// Everything the reference window shows, in display order. Instance
    /// method because "Your Functions"/"Your Data Types" read the live
    /// environment; the built-in categories follow.
    pub fn documentation(&self) -> Vec<crate::documentation::DocCategory> {
        let mut categories: Vec<crate::documentation::DocCategory> = Vec::new();

        let mut user_functions: Vec<UserFunction> =
            self.environment.user_functions().into_values().collect();
        user_functions.sort_by_key(|f| f.name.to_lowercase());
        if !user_functions.is_empty() {
            categories.push(crate::documentation::DocCategory {
                title: "Your Functions".to_string(),
                entries: user_functions.iter().map(Self::doc_for_user).collect(),
            });
        }

        let mut user_data_types: Vec<DataType> = self
            .environment
            .user_data_types()
            .values()
            .cloned()
            .collect();
        user_data_types.sort_by_key(|t| t.name.to_lowercase());
        if !user_data_types.is_empty() {
            categories.push(crate::documentation::DocCategory {
                title: "Your Data Types".to_string(),
                entries: user_data_types.iter().map(Self::doc_for_type).collect(),
            });
        }

        categories.extend(crate::documentation::builtin_documentation());
        categories
    }

    fn doc_for_user(function: &UserFunction) -> FunctionDoc {
        FunctionDoc {
            name: function.name.clone(),
            signature: function.signature(),
            summary: function.documentation().unwrap_or_else(|| {
                format!(
                    "Defined in this workbook. Add documentation with a trailing comment: {}(…) = … # what it does",
                    function.name
                )
            }),
            examples: vec![function.source.clone()],
        }
    }

    /// A data type's docs — same `# doc comment` contract as functions; the
    /// declaration line is the clickable example.
    fn doc_for_type(data_type: &DataType) -> FunctionDoc {
        let fields: Vec<String> = data_type
            .fields
            .iter()
            .map(|f| format!("{}: …", f.name))
            .collect();
        FunctionDoc {
            name: data_type.name.clone(),
            signature: data_type.declaration(),
            summary: data_type.documentation().unwrap_or_else(|| {
                format!(
                    "Declared in this workbook. Construct with {}({}).",
                    data_type.name,
                    fields.join(", ")
                )
            }),
            examples: vec![data_type.source.clone()],
        }
    }
}
