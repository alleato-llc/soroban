//! Environment inspector and reference-window steps: the live variable /
//! function / data-type listings and the documentation lookup.

use crate::SessionWorld;
use cucumber::then;

// MARK: Inspector + reference

#[then(regex = r#"^the inspector lists the variable "(.*)"$"#)]
fn inspector_lists_variable(world: &mut SessionWorld, name: String) {
    let found = world
        .session
        .inspector_variables()
        .iter()
        .any(|row| row.label.contains(&name));
    assert!(found, "the inspector does not list a variable '{name}'");
}

#[then(regex = r#"^the inspector lists the function "(.*)"$"#)]
fn inspector_lists_function(world: &mut SessionWorld, signature: String) {
    let found = world
        .session
        .inspector_functions()
        .iter()
        .any(|row| row.label.contains(&signature));
    assert!(
        found,
        "the inspector does not list a function '{signature}'"
    );
}

#[then(regex = r#"^the inspector lists the data type "(.*)"$"#)]
fn inspector_lists_data_type(world: &mut SessionWorld, name: String) {
    let found = world
        .session
        .inspector_data_types()
        .iter()
        .any(|row| row.label.contains(&name));
    assert!(found, "the inspector does not list a data type '{name}'");
}

#[then(regex = r#"^the reference for "(.*)" documents it$"#)]
fn reference_documents(world: &mut SessionWorld, query: String) {
    let needle = query.to_lowercase();
    let found = world
        .session
        .reference(&query)
        .iter()
        .flat_map(|group| &group.entries)
        .any(|entry| entry.signature.to_lowercase().contains(&needle));
    assert!(found, "the reference documents nothing matching '{query}'");
}
