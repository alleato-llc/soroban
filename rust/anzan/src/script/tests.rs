//! Port of ScriptTests.swift — the statement accumulator: exact joined text,
//! line attribution, and the streaming push/finish contract. User-visible
//! splitting behavior lives in spec/anzan/scripting.feature.

use super::*;

fn texts(source: &str) -> Vec<String> {
    StatementAccumulator::statements(source)
        .expect("splits")
        .into_iter()
        .map(|s| s.text)
        .collect()
}

#[test]
fn balanced_lines_pass_straight_through() {
    let statements = StatementAccumulator::statements("1 + 1\nx = 3\nx").expect("splits");
    assert_eq!(
        statements
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["1 + 1", "x = 3", "x"]
    );
    assert_eq!(
        statements.iter().map(|s| s.line).collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[test]
fn open_brackets_join_to_one_logical_line() {
    let statements = StatementAccumulator::statements("sum(\n    1, 2,\n    3\n)").expect("splits");
    assert_eq!(statements.len(), 1);
    assert_eq!(statements[0].text, "sum( 1, 2, 3 )");
    assert_eq!(statements[0].line, 1);
}

#[test]
fn blank_and_comment_lines_inside_continuation_are_skipped() {
    assert_eq!(
        texts("sum(\n\n    1,\n    # a note mid-block\n    2\n)"),
        ["sum( 1, 2 )"]
    );
}

#[test]
fn first_line_comment_reattaches_to_the_joined_statement() {
    assert_eq!(
        texts("triple(x) = (    # three of x\n    x * 3\n)"),
        ["triple(x) = ( x * 3 )  # three of x"]
    );
}

#[test]
fn brackets_inside_strings_are_text() {
    assert_eq!(texts("s = \"{ ( [\"\nlen(s)"), ["s = \"{ ( [\"", "len(s)"]);
}

#[test]
fn comment_only_lines_are_standalone_statements() {
    let statements =
        StatementAccumulator::statements("#!/usr/bin/env soroban\n# note\n1").expect("splits");
    assert_eq!(
        statements
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["#!/usr/bin/env soroban", "# note", "1"]
    );
    assert_eq!(
        statements.iter().map(|s| s.line).collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[test]
fn stray_closer_does_not_underflow() {
    // The parser owns the error; the splitter must not swallow the NEXT line.
    assert_eq!(texts(") + 1\n2 + 2"), [") + 1", "2 + 2"]);
}

#[test]
fn unterminated_block_errors_naming_the_opening_line() {
    let error = StatementAccumulator::statements("1 + 1\nnamespace Broken {\n    x() = 1")
        .expect_err("unterminated");
    assert_eq!(
        error,
        EngineError::ParseError {
            message:
                "unterminated statement — the block opened at line 2 is missing a closing bracket"
                    .into(),
            position: 0,
        }
    );
}

#[test]
fn streaming_push_reports_pending_state() {
    let mut accumulator = StatementAccumulator::new();
    assert_eq!(accumulator.push("sum("), None);
    assert!(accumulator.is_pending());
    assert_eq!(accumulator.pending_text(), "sum(");
    let statement = accumulator.push("1, 2)").expect("completes");
    assert_eq!(statement.text, "sum( 1, 2)");
    assert!(!accumulator.is_pending());
    accumulator.finish().expect("no-op when nothing pending");
}
