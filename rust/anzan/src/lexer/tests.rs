//! Port of LexerTests.swift — same cases, same expected token streams.

use super::*;
use TokenKind::*;

fn kinds(source: &str) -> Vec<TokenKind> {
    Lexer::tokenize(source, LanguageMode::Normal)
        .unwrap_or_else(|e| panic!("'{source}' should lex: {e}"))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

fn num(s: &str) -> TokenKind {
    Number(BigDecimal::parse(s).unwrap())
}

fn ident(s: &str) -> TokenKind {
    Identifier(s.to_string())
}

#[test]
fn tokenizes_arithmetic() {
    assert_eq!(
        kinds("1 + 2*3"),
        vec![num("1"), Plus, num("2"), Star, num("3"), End]
    );
}

#[test]
fn tokenizes_all_operators() {
    assert_eq!(
        kinds("+-*/%^=(),"),
        vec![Plus, Minus, Star, Slash, Percent, Caret, Assign, LeftParen, RightParen, Comma, End]
    );
}

#[test]
fn tokenizes_number_formats() {
    assert_eq!(
        kinds("1_000 2.5e-3 .5 1E2"),
        vec![num("1000"), num("0.0025"), num("0.5"), num("100"), End]
    );
}

#[test]
fn e_not_followed_by_digits_is_not_an_exponent() {
    // `2e` lexes as number 2 then identifier e (Euler's constant) —
    // implicit multiplication handles the rest.
    assert_eq!(kinds("2e"), vec![num("2"), ident("e"), End]);
    assert_eq!(
        kinds("2e+x"),
        vec![num("2"), ident("e"), Plus, ident("x"), End]
    );
}

#[test]
fn tokenizes_identifiers() {
    assert_eq!(
        kinds("rate_2 = pmt(x)"),
        vec![
            ident("rate_2"),
            Assign,
            ident("pmt"),
            LeftParen,
            ident("x"),
            RightParen,
            End
        ]
    );
}

#[test]
fn records_positions() {
    let tokens = Lexer::tokenize("12 + ab", LanguageMode::Normal).unwrap();
    let ranges: Vec<_> = tokens.into_iter().map(|t| t.range).collect();
    assert_eq!(ranges, vec![0..2, 3..4, 5..7, 7..7]);
}

#[test]
fn rejects_unknown_characters() {
    // '#' became the comment marker — '@' is still illegal.
    assert_eq!(
        Lexer::tokenize("1 @ 2", LanguageMode::Normal),
        Err(EngineError::LexError {
            message: "unexpected character '@'".to_string(),
            position: 2
        })
    );
}

#[test]
fn comments_run_to_end_of_line() {
    assert_eq!(kinds("1 + 2 # three # four").len(), 4); // 1 + 2 end
    assert_eq!(kinds("# only a comment"), vec![End]);
}

#[test]
fn rejects_malformed_numbers() {
    assert!(Lexer::tokenize("1.2.3", LanguageMode::Normal).is_err());
}

// MARK: split_comment (the string-aware splitter both hosts use)

#[test]
fn split_comment_respects_strings() {
    assert_eq!(
        Lexer::split_comment(r#"greet = "a # b" # doc"#),
        (r#"greet = "a # b" "#.to_string(), Some("doc".to_string()))
    );
    assert_eq!(Lexer::split_comment("1 + 2"), ("1 + 2".to_string(), None));
    assert_eq!(
        Lexer::split_comment("# note only"),
        ("".to_string(), Some("note only".to_string()))
    );
}

// MARK: Tokens beyond the Swift lexer suite, pinned here for the port
// (cell references, strings, quoted names, radix literals — the Swift side
// covers these through parser/evaluator tests).

#[test]
fn cell_references_and_pins() {
    assert_eq!(
        kinds("A:1"),
        vec![
            CellReference {
                column: "A".into(),
                row: 1,
                pin_column: false,
                pin_row: false
            },
            End
        ]
    );
    assert_eq!(
        kinds("$A:$12"),
        vec![
            CellReference {
                column: "A".into(),
                row: 12,
                pin_column: true,
                pin_row: true
            },
            End
        ]
    );
    // A ':' not followed by digits keeps the identifier and the colon apart.
    assert_eq!(kinds("a: b"), vec![ident("a"), Colon, ident("b"), End]);
    // A dangling '$' is a loud lex error.
    assert!(Lexer::tokenize("$ 5", LanguageMode::Normal).is_err());
    assert!(Lexer::tokenize("$A + 1", LanguageMode::Normal).is_err());
}

#[test]
fn string_literals_and_escapes() {
    assert_eq!(kinds(r#""hi""#), vec![String("hi".to_string()), End]);
    assert_eq!(
        kinds(r#""a\tb\n\"q\"""#),
        vec![String("a\tb\n\"q\"".to_string()), End]
    );
    assert!(Lexer::tokenize(r#""open"#, LanguageMode::Normal).is_err());
    assert!(Lexer::tokenize(r#""bad \q escape""#, LanguageMode::Normal).is_err());
}

#[test]
fn radix_literals() {
    assert_eq!(kinds("0xFF"), vec![num("255"), End]);
    assert_eq!(kinds("0b1010"), vec![num("10"), End]);
    assert_eq!(kinds("0xDEAD_BEEF"), vec![num("3735928559"), End]);
    assert!(Lexer::tokenize("0xFG", LanguageMode::Normal).is_err());
    assert!(Lexer::tokenize("0x1.5", LanguageMode::Normal).is_err());
    assert!(Lexer::tokenize("0x", LanguageMode::Normal).is_err());
}

#[test]
fn typographic_operators_and_math_symbols() {
    assert_eq!(
        kinds("2 × 3 ÷ 4 − 1"),
        vec![
            num("2"),
            Star,
            num("3"),
            Slash,
            num("4"),
            Minus,
            num("1"),
            End
        ]
    );
    assert_eq!(
        kinds("∑(1)"),
        vec![ident("sigma"), LeftParen, num("1"), RightParen, End]
    );
    assert_eq!(
        kinds("∏(1)"),
        vec![ident("product"), LeftParen, num("1"), RightParen, End]
    );
    assert_eq!(kinds("√9"), vec![SqrtSign, num("9"), End]);
    assert_eq!(
        kinds("1 ≤ 2 ≠ 3"),
        vec![num("1"), LessOrEqual, num("2"), NotEqual, num("3"), End]
    );
}

#[test]
fn two_char_operators_win() {
    assert_eq!(kinds("a != b"), vec![ident("a"), NotEqual, ident("b"), End]);
    assert_eq!(
        kinds("Budget!A:1"),
        vec![
            ident("Budget"),
            Bang,
            CellReference {
                column: "A".into(),
                row: 1,
                pin_column: false,
                pin_row: false
            },
            End
        ]
    );
    assert_eq!(kinds("x -> x"), vec![ident("x"), Arrow, ident("x"), End]);
    // `..` starts a token only where a token can start (after a cell
    // reference); a number scanner that has already taken `1.` errors —
    // matching Swift, where numeric ranges don't exist.
    assert_eq!(
        kinds("A:1..A:9"),
        vec![
            CellReference {
                column: "A".into(),
                row: 1,
                pin_column: false,
                pin_row: false
            },
            DotDot,
            CellReference {
                column: "A".into(),
                row: 9,
                pin_column: false,
                pin_row: false
            },
            End
        ]
    );
    assert!(Lexer::tokenize("1..2", LanguageMode::Normal).is_err());
    assert_eq!(
        kinds("Geo::Pt"),
        vec![ident("Geo"), ColonColon, ident("Pt"), End]
    );
    assert_eq!(
        kinds("1 << 2 >> 3"),
        vec![num("1"), ShiftLeft, num("2"), ShiftRight, num("3"), End]
    );
}

#[test]
fn quoted_names() {
    assert_eq!(
        kinds("'Q1 Budget'!B:2"),
        vec![
            QuotedName("Q1 Budget".to_string()),
            Bang,
            CellReference {
                column: "B".into(),
                row: 2,
                pin_column: false,
                pin_row: false
            },
            End
        ]
    );
    assert!(Lexer::tokenize("'unterminated", LanguageMode::Normal).is_err());
}
