//! Port of ParserTests.swift + SourceTextTests.swift (round-trip contract
//! and containsCellReference propagation).

use super::*;
use crate::ast::BinaryOperator::*;
use crate::ast::Expression::{self, *};

fn parse(source: &str) -> Result<Expression, EngineError> {
    Parser::parse(source, LanguageMode::Normal)
}

fn num(s: &str) -> Expression {
    Number(BigDecimal::parse(s).unwrap())
}

fn var(s: &str) -> Expression {
    Variable(s.to_string())
}

fn binary(op: crate::ast::BinaryOperator, lhs: Expression, rhs: Expression) -> Expression {
    Binary(op, Box::new(lhs), Box::new(rhs))
}

fn call(name: &str, arguments: Vec<Expression>) -> Expression {
    Call {
        name: name.to_string(),
        arguments,
    }
}

// MARK: Parser

#[test]
fn precedence() {
    // 1 + 2 * 3 → 1 + (2 * 3)
    assert_eq!(
        parse("1 + 2 * 3").unwrap(),
        binary(Add, num("1"), binary(Multiply, num("2"), num("3")))
    );
}

#[test]
fn left_associativity() {
    // 8 - 2 - 1 → (8 - 2) - 1
    assert_eq!(
        parse("8 - 2 - 1").unwrap(),
        binary(Subtract, binary(Subtract, num("8"), num("2")), num("1"))
    );
    // 8 / 4 / 2 → (8 / 4) / 2
    assert_eq!(
        parse("8 / 4 / 2").unwrap(),
        binary(Divide, binary(Divide, num("8"), num("4")), num("2"))
    );
}

#[test]
fn power_is_right_associative() {
    // 2 ^ 3 ^ 2 → 2 ^ (3 ^ 2)
    assert_eq!(
        parse("2 ^ 3 ^ 2").unwrap(),
        binary(Power, num("2"), binary(Power, num("3"), num("2")))
    );
}

#[test]
fn unary_minus_binds_looser_than_power() {
    // -2^2 → -(2^2)
    assert_eq!(
        parse("-2^2").unwrap(),
        UnaryMinus(Box::new(binary(Power, num("2"), num("2"))))
    );
    // 2^-1 → 2^(-1)
    assert_eq!(
        parse("2^-1").unwrap(),
        binary(Power, num("2"), UnaryMinus(Box::new(num("1"))))
    );
}

#[test]
fn parentheses() {
    assert_eq!(
        parse("(1 + 2) * 3").unwrap(),
        binary(Multiply, binary(Add, num("1"), num("2")), num("3"))
    );
}

#[test]
fn implicit_multiplication() {
    assert_eq!(parse("2(3)").unwrap(), binary(Multiply, num("2"), num("3")));
    assert_eq!(parse("2x").unwrap(), binary(Multiply, num("2"), var("x")));
    assert_eq!(
        parse("(2)(3)").unwrap(),
        binary(Multiply, num("2"), num("3"))
    );
    assert_eq!(
        parse("2sqrt(4)").unwrap(),
        binary(Multiply, num("2"), call("sqrt", vec![num("4")]))
    );
}

#[test]
fn function_calls() {
    assert_eq!(
        parse("min(1, 2, 3)").unwrap(),
        call("min", vec![num("1"), num("2"), num("3")])
    );
    assert_eq!(
        parse("abs(-5)").unwrap(),
        call("abs", vec![UnaryMinus(Box::new(num("5")))])
    );
}

#[test]
fn assignment() {
    assert_eq!(
        parse("x = 5 * 3").unwrap(),
        Assignment {
            name: "x".to_string(),
            value: Box::new(binary(Multiply, num("5"), num("3")))
        }
    );
}

#[test]
fn cannot_assign_to_reserved_names() {
    assert!(parse("ans = 5").is_err());
    assert!(parse("pi = 3").is_err());
}

#[test]
fn variables_and_ans() {
    assert_eq!(
        parse("ans * 2").unwrap(),
        binary(Multiply, var("ans"), num("2"))
    );
}

#[test]
fn rejects_malformed_expressions() {
    for source in ["1 +", "(1", "min(1,", "* 3", "1 2 +", ")", "1 = 2"] {
        assert!(parse(source).is_err(), "'{source}' should not parse");
    }
}

#[test]
fn errors_carry_positions() {
    assert_eq!(
        parse("(1 + 2"),
        Err(EngineError::ParseError {
            message: "expected ')'".to_string(),
            position: 6
        })
    );
}

// MARK: Expression source printing
//
// `source_text` is the persistence contract for lambdas (a saved
// `f = x -> …` re-enters through it), so the bar is RE-PARSEABILITY for
// every AST shape: parse → print → parse must reproduce the tree.

#[test]
fn round_trips() {
    for source in [
        // numbers, variables, operators (each binary + comparison + unary)
        "1.5 + 2",
        "a - b",
        "a * b",
        "a / b",
        "a % b",
        "2 ^ 10",
        "-x",
        "a < b",
        "a > b",
        "a <= b",
        "a >= b",
        "a == b",
        "a != b",
        // strings, structures, access
        "\"say \\\"hi\\\"\\n\"",
        "[1, 2, [3]]",
        "{a: 1, \"two words\": 2}",
        "arr[0]",
        "m.key",
        "people[1].age",
        "{a: [1, 2]}.a[1]",
        // cells: bare, qualified, quoted-qualified, ranges in calls
        "A:1 + 2",
        "Budget!B:7",
        "'Q1 Budget'!C:3",
        "sum(A:1..B:9, 5)",
        "sum('Q1 Budget'!A:1..A:9)",
        // calls, conditionals, definitions, assignment, reductions, man
        "round(pmt(0.05/12, 360, 200000), 2)",
        "if(a < b, 1, 2 / 0)",
        "x = a + 1",
        "f(x, y) = x * y + 1",
        "sigma_i=1^10(i^2)",
        "product_k=(n - 1)^(m + 1)(k)",
        "man pmt",
        // lambdas (the original consumer), with structures inside
        "x -> x * 2",
        "(a, b) -> a + b",
        "() -> 7",
        "f = x -> sum([x, A:1])",
        // named cells
        "'Projected Rate' * 12",
        "Budget!'Rate' + 1",
        // declarations (beyond the Swift list — data/namespace/import)
        "data Person { name: String, age: Number, active: Boolean }",
        "data Poly { pts: [Number], tags: {String: String} }",
        "namespace Geo { data Point { x: Number, y: Number } }",
        "import Geo",
        "dist(p: Point) = p.x",
    ] {
        let parsed = parse(source).unwrap_or_else(|e| panic!("'{source}' should parse: {e}"));
        let printed = parsed.source_text();
        let reparsed = parse(&printed)
            .unwrap_or_else(|e| panic!("printed form '{printed}' didn't re-parse: {e}"));
        assert_eq!(
            reparsed, parsed,
            "printed form '{printed}' didn't round-trip"
        );
    }
}

// MARK: containsCellReference propagation — decides formula-vs-label
// classification; every node kind must propagate it.

#[test]
fn detects_refs() {
    for source in [
        "A:1",
        "Budget!A:1",
        "sum(A:1..A:9)",
        "-A:1",
        "A:1 + 1",
        "1 + A:1",
        "f(A:1)",
        "x = A:1",
        "g(x) = A:1 * x",
        "if(A:1, 1, 2)",
        "if(1, A:1, 2)",
        "if(1, 2, A:1)",
        "sigma_i=A:1^2(i)",
        "sigma_i=1^A:1(i)",
        "sigma_i=1^2(A:1)",
        "A:1 < 2",
        "[A:1]",
        "{a: A:1}",
        "[1][A:1]",
        "(x -> A:1)",
        "{a: 1}[\"a\"] + A:1",
        "'A Name'", // named cells ARE cell references
    ] {
        assert!(
            parse(source).unwrap().contains_cell_reference(),
            "'{source}' should contain a cell reference"
        );
    }
}

#[test]
fn detects_no_refs() {
    for source in [
        "1 + 2",
        "x * y",
        "f(1)",
        "x = 1",
        "f(x) = x",
        "if(1, 2, 3)",
        "sigma_i=1^3(i)",
        "\"text\"",
        "[1, {a: 2}]",
        "m.key",
        "arr[0]",
        "x -> x * 2",
        "a < b",
        "man pmt",
    ] {
        assert!(
            !parse(source).unwrap().contains_cell_reference(),
            "'{source}' should NOT contain a cell reference"
        );
    }
}

// MARK: Mode-parameterized parsing (the invariants CLAUDE.md pins)

#[test]
fn programmer_mode_glyphs() {
    let p = |s: &str| Parser::parse(s, LanguageMode::Programmer);
    // ^ → bitXor, & → bitAnd, | → bitOr, % → mod, ~ → bitNot
    assert_eq!(
        p("5 ^ 3").unwrap(),
        call("bitXor", vec![num("5"), num("3")])
    );
    assert_eq!(
        p("5 & 3").unwrap(),
        call("bitAnd", vec![num("5"), num("3")])
    );
    assert_eq!(p("5 | 3").unwrap(), call("bitOr", vec![num("5"), num("3")]));
    assert_eq!(p("5 % 3").unwrap(), call("mod", vec![num("5"), num("3")]));
    assert_eq!(p("~5").unwrap(), call("bitNot", vec![num("5")]));
    assert_eq!(
        p("1 << 4").unwrap(),
        call("bitShift", vec![num("1"), num("4")])
    );
    // >> negates the count.
    assert_eq!(
        p("16 >> 2").unwrap(),
        call("bitShift", vec![num("16"), UnaryMinus(Box::new(num("2")))])
    );
}

#[test]
fn out_of_mode_glyphs_error_loudly() {
    for source in ["5 & 3", "5 | 3", "1 << 2", "8 >> 1", "~5"] {
        let err = parse(source).unwrap_err().to_string();
        assert!(
            err.contains("Programmer-mode operator"),
            "'{source}' error should mention the mode: {err}"
        );
    }
}

#[test]
fn programmer_mode_round_trips_re_spell_glyphs() {
    // The renderer re-spells the canonical calls as glyphs under the same
    // mode, and the result re-parses to the same tree.
    for source in [
        "5 ^ 3", "5 & 3", "5 | 3", "5 % 3", "~5", "1 << 4", "16 >> 2", "2 3",
    ] {
        let Ok(parsed) = Parser::parse(source, LanguageMode::Programmer) else {
            continue; // "2 3" stays an error in every mode
        };
        let printed = parsed.source_text_in(LanguageMode::Programmer);
        let reparsed = Parser::parse(&printed, LanguageMode::Programmer)
            .unwrap_or_else(|e| panic!("printed '{printed}' didn't re-parse: {e}"));
        assert_eq!(
            reparsed, parsed,
            "printed form '{printed}' didn't round-trip"
        );
    }
}

#[test]
fn named_arguments_desugar_to_one_map() {
    assert_eq!(
        parse("Person(name: \"Ada\", age: 36)").unwrap(),
        call(
            "Person",
            vec![MapLiteral(vec![
                crate::ast::MapLiteralEntry {
                    key: "name".to_string(),
                    value: StringLiteral("Ada".to_string())
                },
                crate::ast::MapLiteralEntry {
                    key: "age".to_string(),
                    value: num("36")
                },
            ])]
        )
    );
    // Compact multi-letter key:number fuses into a cell-reference token and
    // decomposes back; a single-letter one stays a cell reference.
    assert_eq!(
        parse("f(age:36)").unwrap(),
        call(
            "f",
            vec![MapLiteral(vec![crate::ast::MapLiteralEntry {
                key: "age".to_string(),
                value: num("36")
            }])]
        )
    );
    assert_eq!(
        parse("f(a:1)").unwrap(),
        call(
            "f",
            vec![CellReference {
                sheet: None,
                column: "a".to_string(),
                row: 1
            }]
        )
    );
}

#[test]
fn compact_map_keys_decompose() {
    assert_eq!(
        parse("{b:1}").unwrap(),
        MapLiteral(vec![crate::ast::MapLiteralEntry {
            key: "b".to_string(),
            value: num("1")
        }])
    );
}

#[test]
fn chained_comparisons_are_rejected() {
    let err = parse("1 < 2 < 3").unwrap_err().to_string();
    assert!(err.contains("can't be chained"), "got: {err}");
}
