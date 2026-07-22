//! The Examples menu's data: real, valid expressions surveying the engine's
//! depth, grouped by language component.
//!
//! The categories and expressions are copied VERBATIM from the Swift app's
//! `CalculatorSession.welcomeCategories`
//! (`swift/App/Sources/Session/CalculatorSession.swift`) — that list is the
//! source of truth and shared vocabulary across the apps; edit there first,
//! then mirror here.

/// The example categories, in menu order: `(name, expressions)`.
pub(crate) const CATEGORIES: &[(&str, &[&str])] = &[
    // The flagship: data types + recursion + namespaces + finance, one line.
    (
        "Showcase",
        &[
            "namespace Cash { data Change { quarters: Number, dimes: Number, nickels: Number, pennies: Number }; coins(c, d) = if(c < d, 0, 1 + coins(c - d, d)); makeChange(c) = Change(quarters: coins(c, 25), dimes: coins(mod(c, 25), 10), nickels: coins(mod(mod(c, 25), 10), 5), pennies: coins(mod(mod(mod(c, 25), 10), 5), 1)); changeForDollar(cost) = makeChange((1 - cost) * 100) }",
            "Cash::changeForDollar(0.95)",
        ],
    ),
    (
        "Higher-order",
        &[
            "map(n -> n * n, filter(x -> mod(x, 2) == 0, seq(1, 20)))",
            "reduce((a, b) -> a * b, seq(1, 10), 1)",
            "sum(map(x -> x^2, seq(1, 10)))",
            "len(filter(x -> x > 5, [3, 7, 2, 9, 5, 11]))",
        ],
    ),
    ("Reductions", &["∑_i=1^100(1 / i^2)", "∏_i=1^10(i)"]),
    (
        "Finance",
        &[
            "pmt(0.0425/12, 360, 450000)",
            "round(100000 * (1 + 0.05/12)^(12 * 10), 2)",
            "npv(0.1, -1000, 300, 400, 500, 600)",
            "fv(0.06, 10, -1200)",
            "ipmt(0.05/12, 1, 360, 200000)",
        ],
    ),
    (
        "Statistics",
        &[
            "stdev(82, 91, 77, 88, 64, 95)",
            "percentile(seq(1, 100), 0.9)",
            "median(seq(1, 99))",
            "forecast(8, 1, 2, 3, 4, 2, 4, 6, 8)",
        ],
    ),
    (
        "Combinatorics",
        &[
            "fact(52) / (fact(5) * fact(47))",
            "choose(52, 5)",
            "perm(10, 3)",
            "lcm(12, 18)",
        ],
    ),
    (
        "Structures",
        &[
            "sort([5, 2, 8, 1, 9, 3])",
            "unique([3, 1, 4, 1, 5, 9, 2, 6, 5, 3])",
            "keys({alpha: 1, beta: 2, gamma: 3})",
            "concat([1, 2, 3], [4, 5, 6])",
            "{name: \"Ada\", born: 1815}.born",
        ],
    ),
    (
        "JSON & data types",
        &[
            "toJson({name: \"Ada\", scores: [91, 88, 95]})",
            r#"fromJson("{\"x\": 3, \"y\": 4}")"#,
            "data Point { x: Number, y: Number }",
        ],
    ),
    (
        "Definitions & logic",
        &[
            "compound(p, r, n) = p * (1 + r)^n",
            "if(gcd(17, 5) == 1, \"coprime\", \"shares a factor\")",
        ],
    ),
    (
        "Programmer",
        &[
            "0xFF + 0b1010",
            "fromBase(\"FF\", 16)",
            "bitXor(12, 10)",
            "log(2, 1024)",
        ],
    ),
    (
        "Dates",
        &["edate(today(), 6)", "networkdays(today(), today() + 30)"],
    ),
    ("Scientific", &["atan2(1, 1) * 4", "exp(1)"]),
    (
        "Simple",
        &["sqrt(3^2 + 4^2)", "2 ^ 64", "x = 12 * 80.5", "ans * 1.0825"],
    ),
];

/// The longest label that fits rime's fixed-width dropdown panel (200 px at
/// size-13 text) without wrapping onto a second line.
pub(crate) const LABEL_MAX: usize = 26;

/// A flyout row's label for `example`: the expression itself when it fits,
/// else its first [`LABEL_MAX`]` - 1` characters plus an ellipsis (the full
/// text is still what selection inserts — only the label truncates).
pub(crate) fn menu_label(example: &str) -> String {
    if example.chars().count() <= LABEL_MAX {
        return example.to_string();
    }
    let head: String = example.chars().take(LABEL_MAX - 1).collect();
    format!("{}…", head.trim_end())
}

#[cfg(test)]
mod tests;
