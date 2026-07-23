//! The reference window's data model — port of Documentation.swift. Built-in
//! functions document themselves (the fields are required at registration);
//! special forms, operators, and constants are curated here; user-defined
//! functions are generated live from the environment (Calculator).

use crate::calculator::FunctionDoc;
use crate::eval::registry::{FunctionCategory, FunctionRegistry};

fn doc(name: &str, signature: &str, summary: &str, examples: &[&str]) -> FunctionDoc {
    FunctionDoc {
        name: name.to_string(),
        signature: signature.to_string(),
        summary: summary.to_string(),
        examples: examples.iter().map(|e| e.to_string()).collect(),
    }
}

/// A titled group of docs, for the reference window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocCategory {
    pub title: String,
    pub entries: Vec<FunctionDoc>,
}

/// Every category shown in the reference window — Swift's
/// `Calculator.builtinDocumentation`: Special Forms first, then each
/// registry category that has entries (built-ins document themselves at
/// registration), then the curated Operators & Syntax and Constants pages.
pub fn builtin_documentation() -> Vec<DocCategory> {
    let mut categories = vec![DocCategory {
        title: "Special Forms".to_string(),
        entries: special_forms(),
    }];
    for kind in FunctionCategory::ALL {
        let entries: Vec<FunctionDoc> = FunctionRegistry::standard()
            .all()
            .into_iter()
            .filter(|f| f.category == kind)
            .map(|f| FunctionDoc {
                name: f.name.to_string(),
                signature: f.signature.to_string(),
                summary: f.summary.to_string(),
                examples: f.examples.iter().map(|e| e.to_string()).collect(),
            })
            .collect();
        if !entries.is_empty() {
            categories.push(DocCategory {
                title: kind.heading().to_string(),
                entries,
            });
        }
    }
    categories.push(DocCategory {
        title: "Operators & Syntax".to_string(),
        entries: operators(),
    });
    categories.push(DocCategory {
        title: "Constants".to_string(),
        entries: constants(),
    });
    categories
}

pub(crate) fn special_forms() -> Vec<FunctionDoc> {
    vec![
        doc(
            "if",
            "if(condition, then, else)",
            "Returns `then` when the condition is nonzero, otherwise `else`. Only the taken branch is evaluated, so the other may divide by zero — or recurse: fact(n) = if(n <= 1, 1, n * fact(n - 1)).",
            &["if(1 < 2, 10, 20)", "if(0, 1/0, 7)"],
        ),
        doc(
            "sigma",
            "∑_i=1^10(term)   ·   ∑(x, y, …)",
            "Summation. The subscript form re-evaluates the term with the index bound to each integer (type sigma_i=1^10(…) if ∑ is out of reach; compound bounds need parens: ∑_i=(n-1)^10(…)). A plain ∑(…) call simply sums its arguments.",
            &["∑_i=1^10(i^2)", "∑(1, 2, 3)"],
        ),
        doc(
            "productForm",
            "∏_i=1^5(term)   ·   ∏(x, y, …)",
            "Product — ∑'s multiplicative sibling (type product_i=1^5(…)). ∏_i=1^n(i) is an exact factorial; ∏_i=1^n(1 + r) is compound growth.",
            &["∏_i=1^5(i)", "∏(2, 3, 4)"],
        ),
    ]
}

pub(crate) fn operators() -> Vec<FunctionDoc> {
    vec![
        doc(
            "arithmetic",
            "+  −  ×(*)  ÷(/)  ^  %",
            "Exact decimal arithmetic — 0.1 + 0.2 is exactly 0.3. ^ is power (right-associative); postfix % is percent (3% → 0.03), and mod(x, y) is modulo. Typographic × ÷ − · paste fine. Implicit multiplication works: 2(3 + 4), 2x, 2π. In Programmer mode ^ and % — plus & | << >> ~ — read as bitwise/modulo instead; see man modes.",
            &["0.1 + 0.2", "2^10", "3%", "2π"],
        ),
        doc(
            "comparisons",
            "<  >  <=  >=  ==  !=   (≤ ≥ ≠)",
            "Comparisons return 1 (true) or 0 (false) — feed them to if(). They can't chain; use and(a < b, b < c).",
            &["2 < 3", "0.1 + 0.2 == 0.3"],
        ),
        doc(
            "assignment",
            "x = expr   ·   f(a, b) = expr",
            "Variables and custom functions. Functions compose and may recurse (via if); parameters shadow globals; built-in names are protected. Both are saved in workbooks.",
            &["x = 12 * 80.5", "double(n) = n * 2"],
        ),
        doc(
            "cells",
            "A:1   ·   A:1..B:9",
            "Grid references — column letter, colon, 1-based row — usable in cells AND the calculation log. Ranges (rectangles allowed) expand inside functions; empty and text cells are skipped.",
            &["sum(A:1..B:3)", "count(A:1..A:9)"],
        ),
        doc(
            "sqrtSign",
            "√x",
            "Prefix square root — binds like unary minus, so √2^2 = √(2²) = 2.",
            &["√16", "√(2 + 2)"],
        ),
        doc(
            "degrees",
            "x°",
            "Postfix degrees→radians: x° is x × π/180 (π at the engine's 50-digit precision), so sin(90°) = 1. Works in every mode, chains like % (A:1°, (a + b)°). The trig functions themselves always take radians — ° is how you hand them degrees.",
            &["sin(90°)", "cos(180°)", "90° == pi / 2"],
        ),
        doc(
            "strings",
            "\"text\"   ·   +",
            "Double-quoted string values (escapes: \\\" \\\\ \\n \\t). + concatenates as soon as either side is a string; == compares. In a cell, a formula that returns a string displays as text.",
            &["\"Q\" + 1", "greeting = \"hello\""],
        ),
        doc(
            "arrays",
            "[a, b, …]   ·   arr[0]",
            "Array values — elements are any expressions and nest freely. Indexing is 0-based. Numeric functions accept arrays like ranges: sum(arr), max(arr). Arrays live in the log and in formulas; a cell can't display one.",
            &["[1, 2, 3][0]", "sum([1, 2, 3])", "len([[1, 2], [3]])"],
        ),
        doc(
            "maps",
            "{key: value, …}   ·   m.key   ·   m[\"key\"]",
            "Maps hold named values, nest with arrays, and read via .key or [\"key\"] (keys are case-sensitive). Build records: person = {name: \"Ada\", age: 36}.",
            &["{name: \"Ada\", age: 36}.age", "{a: 1, b: 2}[\"b\"]"],
        ),
        doc(
            "data types",
            "data Person { name: String, age: Number, active: Boolean }",
            "Declares a typed record (fields: Number, String, Boolean). Construct with named fields or from a map — never positionally. Instances read like maps (p.name), collect into arrays, and work with map/filter/reduce; toJson() keeps Boolean fields honest. In a cell, a plain declaration makes a sheet-scoped 𝑫 type.",
            &["data Pt { x: Number, y: Number }", "p = Pt(x: 3, y: 4)", "sqrt(p.x^2 + p.y^2)"],
        ),
        doc(
            "lambdas",
            "x -> expr   ·   (a, b) -> expr",
            "Anonymous functions, for map/filter/reduce — or assign one: f = x -> x * 2, then f(3). A bare function name is a value too: map(sqrt, arr). Lambdas close over function parameters by value.",
            &["map(x -> x ^ 2, [1, 2, 3])", "double = x -> x * 2"],
        ),
        doc(
            "modes",
            ":mode normal · programmer · scientific [eng]",
            "Input/display DIALECTS for the calculation log. Normal (default): ^ is power, postfix % is percent, and bit ops are functions (bitAnd, bitOr, bitXor, bitShift, bitNot). Programmer: ^ is XOR, & AND, | OR, << >> shifts, % modulo, ~ NOT (Python precedence; power becomes pow). Scientific: the grammar is Normal's, but a plain numeric result ECHOES in scientific notation (123456 * 2 → 2.46912e5) — or engineering notation via :mode scientific eng (246.912e3, exponent a multiple of 3); Money and grouped numbers keep their own display. A dialect only changes which glyphs you type and read — the stored formula is always canonical, so it never means two things. Currency ($10) and thousands grouping (138,561) are CORE grammar, in every mode. SWITCH: Settings → Mode (⌘,) or the input-bar mode icon, or type :mode programmer (or scientific / normal) — the :mode command works in both the app log and the CLI. Grid cells are always Normal.",
            &["5 ^ 3", "pow(2, 10)", "bitAnd(12, 10)"],
        ),
    ]
}

pub(crate) fn constants() -> Vec<FunctionDoc> {
    vec![
        doc("pi", "pi · π", "The circle constant, to 60 digits.", &["2π", "sin(pi / 2)"]),
        doc("tau", "tau · τ", "2π.", &["τ ÷ π"]),
        doc("e", "e", "Euler's number, to 60 digits.", &["ln(e)"]),
        doc(
            "ans",
            "ans",
            "The result of the previous calculation in the log.",
            &["1 + 1", "ans * 2"],
        ),
        doc(
            "true",
            "true · false",
            "1 and 0 — the engine's truth values, matching what comparisons return.",
            &["if(true, 10, 20)", "true == 1"],
        ),
        doc(
            "Json",
            "Json.Pretty · Json.Compact",
            "Formatting options for toJson() — named constants instead of a magic flag. Pretty is the default; Json.Compact packs to one line. They're plain string values (\"pretty\" / \"compact\") carried in a constant map.",
            &["toJson({a: 1}, Json.Compact)", "Json.Pretty"],
        ),
        doc(
            "Rounding",
            "Rounding.Bankers · Rounding.HalfUp",
            "Rounding modes for Decimal() — Bankers (round half to even, the default and the engine's standard) or HalfUp (round half away from zero). Named constants like Json; plain string values in a constant map.",
            &["Decimal(1.005, 5, 2, Rounding.HalfUp)", "Rounding.Bankers"],
        ),
    ]
}
