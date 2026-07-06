//! Calculator (the log) steps: setup, evaluation, mode, autocomplete, function
//! definitions, and error/note outcomes.

use crate::{last_outcome, SessionWorld};
use cucumber::{given, then, when};
use soroban_gui::session::{Outcome, Session};

// MARK: Setup

#[given("a fresh session")]
fn a_fresh_session(_world: &mut SessionWorld) {
    // `Self::fresh` already built one per scenario; this reads as intent.
}

// MARK: Calculator (the log)

#[when(regex = r#"^I enter "(.*)"$"#)]
fn i_enter(world: &mut SessionWorld, expression: String) {
    world.session.set_input(expression);
    world.session.submit();
}

#[then(regex = r#"^the result is "(.*)"$"#)]
fn the_result_is(world: &mut SessionWorld, expected: String) {
    match &last_outcome(world) {
        Outcome::Value(value) => assert_eq!(
            *value, expected,
            "result is '{value}', expected '{expected}'"
        ),
        other => panic!("expected a value '{expected}', got {other:?}"),
    }
}

#[then(regex = r#"^the mode is "(.*)"$"#)]
fn the_mode_is(world: &mut SessionWorld, expected: String) {
    assert_eq!(
        world.session.language_mode().name(),
        expected,
        "mode is '{}', expected '{expected}'",
        world.session.language_mode().name()
    );
}

#[then(regex = r#"^the suggestions for "(.*)" include "(.*)"$"#)]
fn suggestions_include(world: &mut SessionWorld, draft: String, name: String) {
    let names: Vec<String> = world
        .session
        .suggestions(&draft)
        .into_iter()
        .map(|completion| completion.name)
        .collect();
    assert!(
        names.iter().any(|candidate| candidate == &name),
        "suggestions for '{draft}' = {names:?}, expected to include '{name}'"
    );
}

#[then(regex = r#"^the suggestions for "(.*)" are empty$"#)]
fn suggestions_empty(world: &mut SessionWorld, draft: String) {
    let names: Vec<String> = world
        .session
        .suggestions(&draft)
        .into_iter()
        .map(|completion| completion.name)
        .collect();
    assert!(
        names.is_empty(),
        "expected no suggestions for '{draft}', got {names:?}"
    );
}

#[then(regex = r#"^completing "(.*)" at "(.*)" yields "(.*)"$"#)]
fn completing_yields(world: &mut SessionWorld, draft: String, name: String, expected: String) {
    let completion = world
        .session
        .suggestions(&draft)
        .into_iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("no suggestion '{name}' for '{draft}'"));
    assert_eq!(
        Session::apply_completion(&draft, &completion),
        expected,
        "completing '{draft}' at '{name}'"
    );
}

#[then(regex = r#"^the log defines a function "(.*)"$"#)]
fn the_log_defines_a_function(world: &mut SessionWorld, signature: String) {
    match &last_outcome(world) {
        Outcome::Function(actual) => assert!(
            actual.contains(&signature),
            "defined '{actual}', expected a signature containing '{signature}'"
        ),
        other => panic!("expected a function definition '{signature}', got {other:?}"),
    }
}

#[then(regex = r#"^the last line fails mentioning "(.*)"$"#)]
fn the_last_line_fails(world: &mut SessionWorld, fragment: String) {
    match &last_outcome(world) {
        Outcome::Error { message, .. } => assert!(
            message.contains(&fragment),
            "failed with '{message}', expected it to mention '{fragment}'"
        ),
        other => panic!("expected an error mentioning '{fragment}', got {other:?}"),
    }
}

#[then(regex = r#"^the last line is a note "(.*)"$"#)]
fn the_last_line_is_a_note(world: &mut SessionWorld, expected: String) {
    match &last_outcome(world) {
        Outcome::Comment(text) => {
            assert_eq!(*text, expected, "note is '{text}', expected '{expected}'")
        }
        other => panic!("expected a note '{expected}', got {other:?}"),
    }
}
