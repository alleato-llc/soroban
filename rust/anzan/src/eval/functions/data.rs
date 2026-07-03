//! Port of the data function list — see the matching Swift file in
//! swift/Engine/Sources/Anzan/Functions/.
//!
//! Structure & text functions — the Value-aware builtins. Unlike numeric
//! functions, these do NOT flatten array arguments (len([1, 2]) must see the
//! array, not two numbers).

use crate::eval::json::{json_text, JsonParser};
use crate::eval::registry::{Applier, BuiltinFunction, FunctionCategory, Implementation};
use crate::eval::value::Value;
use crate::{BigDecimal, EngineError};

pub(crate) fn list() -> Vec<BuiltinFunction> {
    vec![
        BuiltinFunction {
            name: "len",
            category: FunctionCategory::Data,
            signature: "len(value)",
            summary: "Number of elements in an array or map, or characters in a string.",
            examples: &["len([1, 2, 3])", "len({name: \"Ada\", age: 36})", "len(\"hello\")"],
            arity: 1..=1,
            implementation: Implementation::Values(len),
        },
        BuiltinFunction {
            name: "first",
            category: FunctionCategory::Data,
            signature: "first(array)",
            summary: "The first element of an array (index 0).",
            examples: &["first([5, 6, 7])"],
            arity: 1..=1,
            implementation: Implementation::Values(first),
        },
        BuiltinFunction {
            name: "last",
            category: FunctionCategory::Data,
            signature: "last(array)",
            summary: "The last element of an array.",
            examples: &["last([5, 6, 7])"],
            arity: 1..=1,
            implementation: Implementation::Values(last),
        },
        BuiltinFunction {
            name: "keys",
            category: FunctionCategory::Data,
            signature: "keys(map)",
            summary: "A map's keys, as an array of strings (insertion order).",
            examples: &["keys({name: \"Ada\", age: 36})"],
            arity: 1..=1,
            implementation: Implementation::Values(keys),
        },
        BuiltinFunction {
            name: "values",
            category: FunctionCategory::Data,
            signature: "values(map)",
            summary: "A map's values, as an array (insertion order).",
            examples: &["values({a: 1, b: 2})", "sum(values({a: 1, b: 2}))"],
            arity: 1..=1,
            implementation: Implementation::Values(values),
        },
        BuiltinFunction {
            name: "map",
            category: FunctionCategory::Data,
            signature: "map(f, array)",
            summary: "Applies a function to every element: pass a lambda (x -> x * 2) or a function name (yours or a built-in). Returns the transformed array.",
            examples: &["map(x -> x * 2, [1, 2, 3])", "map(sqrt, [1, 4, 9])"],
            arity: 2..=2,
            implementation: Implementation::HigherOrder(map_fn),
        },
        BuiltinFunction {
            name: "filter",
            category: FunctionCategory::Data,
            signature: "filter(predicate, array)",
            summary: "Keeps the elements where the predicate returns nonzero (comparisons return 1/0, so x -> x > 10 reads naturally).",
            examples: &["filter(x -> x > 1, [1, 2, 3])", "filter(x -> mod(x, 2) == 0, [1, 2, 3, 4])"],
            arity: 2..=2,
            implementation: Implementation::HigherOrder(filter_fn),
        },
        BuiltinFunction {
            name: "reduce",
            category: FunctionCategory::Data,
            signature: "reduce(f, array, initial)",
            summary: "Folds an array left-to-right: f(accumulator, element), starting from `initial`. reduce((a, b) -> a + b, arr, 0) is sum.",
            examples: &["reduce((a, b) -> a + b, [1, 2, 3], 0)", "reduce((a, b) -> a * b, [1, 2, 3, 4], 1)"],
            arity: 3..=3,
            implementation: Implementation::HigherOrder(reduce_fn),
        },
        BuiltinFunction {
            name: "concat",
            category: FunctionCategory::Data,
            signature: "concat(a, b, …)",
            summary: "Joins values into one string (numbers render plainly) — or joins arrays into one array when every argument is an array.",
            examples: &["concat(\"Q\", 1)", "concat([1, 2], [3])"],
            arity: 2..=usize::MAX,
            implementation: Implementation::Values(concat),
        },
        BuiltinFunction {
            name: "toJson",
            category: FunctionCategory::Data,
            signature: "toJson(value, option?)",
            summary: "Renders a value as JSON — pretty-printed by default (you're usually reading it); pass Json.Compact for the one-line interchange form. The options are plain strings, so \"compact\" works too. Boolean fields of data types come out as true/false; numbers keep their full precision.",
            examples: &["toJson({name: \"Ada\", age: 36})", "toJson([1, 2, 3], Json.Compact)"],
            arity: 1..=2,
            implementation: Implementation::Values(to_json),
        },
        BuiltinFunction {
            name: "fromJson",
            category: FunctionCategory::Data,
            signature: "fromJson(text)",
            summary: "Parses JSON text into a value — objects become maps, arrays arrays, true/false 1/0, and numbers EXACT decimals (parsed at full precision, never through floating point). Type the result with a constructor: Person(fromJson(t)). JSON null is refused — Anzan has no null.",
            examples: &["fromJson(\"[1, 2, 3]\")", "fromJson(toJson({a: 1})).a"],
            arity: 1..=1,
            implementation: Implementation::Values(from_json),
        },
        // The range→array bridge: ranges expand IN PLACE as arguments, so
        // list(A:1..A:9) collects the expansion into one array — which is
        // what unlocks filter/map/reduce over cells.
        BuiltinFunction {
            name: "list",
            category: FunctionCategory::Data,
            signature: "list(x, y, …)",
            summary: "Collects its arguments into one array. The reason it exists: ranges expand into arguments, so list(A:1..A:9) turns a range into an array — then map/filter/reduce apply.",
            examples: &["list(1, 2, 3)", "sum(filter(x -> x > 1, list(1, 2, 3)))"],
            arity: 0..=usize::MAX,
            implementation: Implementation::Values(list_fn),
        },
        BuiltinFunction {
            name: "sort",
            category: FunctionCategory::Data,
            signature: "sort(array)",
            summary: "Sorts an array ascending — all numbers, or all strings (lexicographic).",
            examples: &["sort([3, 1, 2])", "sort([\"pear\", \"fig\"])"],
            arity: 1..=1,
            implementation: Implementation::Values(sort),
        },
        BuiltinFunction {
            name: "unique",
            category: FunctionCategory::Data,
            signature: "unique(array)",
            summary: "Drops duplicate elements (deep equality), keeping first-seen order.",
            examples: &["unique([3, 1, 3, 2, 1])", "len(unique([1, 1, 1]))"],
            arity: 1..=1,
            implementation: Implementation::Values(unique),
        },
        BuiltinFunction {
            name: "reverse",
            category: FunctionCategory::Data,
            signature: "reverse(value)",
            summary: "Reverses an array — or a string, character by character.",
            examples: &["reverse([1, 2, 3])", "reverse(\"abc\")"],
            arity: 1..=1,
            implementation: Implementation::Values(reverse),
        },
        BuiltinFunction {
            name: "seq",
            category: FunctionCategory::Data,
            signature: "seq(from, to, step = 1)",
            summary: "An array counting from `from` to `to` (inclusive when the step lands on it). Step defaults to 1, or -1 when counting down.",
            examples: &["seq(1, 5)", "seq(10, 0, -2)", "sum(map(x -> x^2, seq(1, 10)))"],
            arity: 2..=3,
            implementation: Implementation::Values(seq),
        },
    ]
}

fn len(arguments: &[Value]) -> Result<Value, EngineError> {
    match &arguments[0] {
        Value::Array(items) => Ok(Value::Number(BigDecimal::from_int(items.len() as i64))),
        Value::Map(entries) => Ok(Value::Number(BigDecimal::from_int(entries.len() as i64))),
        Value::Record(record) => Ok(Value::Number(BigDecimal::from_int(
            record.entries.len() as i64
        ))),
        // Character count, not bytes — strings index by character.
        Value::String(text) => Ok(Value::Number(BigDecimal::from_int(
            text.chars().count() as i64
        ))),
        Value::Number(_)
        | Value::FixedInt(_)
        | Value::FixedDecimal(_)
        | Value::Function(_)
        | Value::Host(_) => Err(EngineError::domain(
            "len() works on arrays, maps, and strings",
        )),
    }
}

fn first(arguments: &[Value]) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[0] else {
        return Err(EngineError::domain(format!(
            "first() works on arrays, got {}",
            arguments[0].kind_name()
        )));
    };
    items
        .first()
        .cloned()
        .ok_or_else(|| EngineError::domain("first() of an empty array"))
}

fn last(arguments: &[Value]) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[0] else {
        return Err(EngineError::domain(format!(
            "last() works on arrays, got {}",
            arguments[0].kind_name()
        )));
    };
    items
        .last()
        .cloned()
        .ok_or_else(|| EngineError::domain("last() of an empty array"))
}

fn keys(arguments: &[Value]) -> Result<Value, EngineError> {
    let entries = match &arguments[0] {
        Value::Map(entries) => entries,
        Value::Record(record) => &record.entries,
        other => {
            return Err(EngineError::domain(format!(
                "keys() works on maps, got {}",
                other.kind_name()
            )));
        }
    };
    Ok(Value::Array(
        entries
            .iter()
            .map(|entry| Value::String(entry.key.clone()))
            .collect(),
    ))
}

fn values(arguments: &[Value]) -> Result<Value, EngineError> {
    let entries = match &arguments[0] {
        Value::Map(entries) => entries,
        Value::Record(record) => &record.entries,
        other => {
            return Err(EngineError::domain(format!(
                "values() works on maps, got {}",
                other.kind_name()
            )));
        }
    };
    Ok(Value::Array(
        entries.iter().map(|entry| entry.value.clone()).collect(),
    ))
}

fn map_fn(arguments: &[Value], applier: Applier<'_>) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[1] else {
        return Err(EngineError::domain(format!(
            "map() wants (function, array) — got {} second",
            arguments[1].kind_name()
        )));
    };
    let mut mapped = Vec::with_capacity(items.len());
    for item in items {
        mapped.push(applier(&arguments[0], std::slice::from_ref(item))?);
    }
    Ok(Value::Array(mapped))
}

fn filter_fn(arguments: &[Value], applier: Applier<'_>) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[1] else {
        return Err(EngineError::domain(format!(
            "filter() wants (predicate, array) — got {} second",
            arguments[1].kind_name()
        )));
    };
    let mut kept = Vec::new();
    for item in items {
        let verdict = applier(&arguments[0], std::slice::from_ref(item))?
            .as_number("the filter() predicate's result")?;
        if !verdict.is_zero() {
            kept.push(item.clone());
        }
    }
    Ok(Value::Array(kept))
}

fn reduce_fn(arguments: &[Value], applier: Applier<'_>) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[1] else {
        return Err(EngineError::domain(format!(
            "reduce() wants (function, array, initial) — got {} second",
            arguments[1].kind_name()
        )));
    };
    let mut accumulator = arguments[2].clone();
    for item in items {
        accumulator = applier(&arguments[0], &[accumulator, item.clone()])?;
    }
    Ok(accumulator)
}

fn concat(arguments: &[Value]) -> Result<Value, EngineError> {
    // All arrays → array concatenation; otherwise string concatenation.
    let mut joined: Vec<Value> = Vec::new();
    let mut all_arrays = true;
    for argument in arguments {
        if let Value::Array(items) = argument {
            joined.extend(items.iter().cloned());
        } else {
            all_arrays = false;
            break;
        }
    }
    if all_arrays {
        return Ok(Value::Array(joined));
    }
    Ok(Value::String(
        arguments
            .iter()
            .map(Value::display_text)
            .collect::<Vec<_>>()
            .join(""),
    ))
}

fn to_json(arguments: &[Value]) -> Result<Value, EngineError> {
    let mut pretty = true; // reading is the common case; compact is opt-in
    if arguments.len() == 2 {
        let Value::String(option) = &arguments[1] else {
            return Err(EngineError::domain(format!(
                "toJson's option is Json.Pretty or Json.Compact — got {}",
                arguments[1].kind_name()
            )));
        };
        match option.to_lowercase().as_str() {
            "pretty" => pretty = true,
            "compact" => pretty = false,
            _ => {
                return Err(EngineError::domain(format!(
                    "unknown toJson option \"{option}\" — use Json.Pretty or Json.Compact"
                )));
            }
        }
    }
    Ok(Value::String(json_text(&arguments[0], pretty)?))
}

fn from_json(arguments: &[Value]) -> Result<Value, EngineError> {
    let Value::String(text) = &arguments[0] else {
        return Err(EngineError::domain(format!(
            "fromJson() wants JSON text, got {}",
            arguments[0].kind_name()
        )));
    };
    JsonParser::parse(text)
}

fn list_fn(arguments: &[Value]) -> Result<Value, EngineError> {
    Ok(Value::Array(arguments.to_vec()))
}

fn sort(arguments: &[Value]) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[0] else {
        return Err(EngineError::domain(format!(
            "sort() works on arrays, got {}",
            arguments[0].kind_name()
        )));
    };
    let mut numbers: Vec<BigDecimal> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for item in items {
        if let Value::Number(n) = item {
            numbers.push(n.clone());
        }
        if let Value::String(s) = item {
            texts.push(s.clone());
        }
    }
    if numbers.len() == items.len() {
        numbers.sort();
        return Ok(Value::Array(
            numbers.into_iter().map(Value::Number).collect(),
        ));
    }
    if texts.len() == items.len() {
        texts.sort();
        return Ok(Value::Array(texts.into_iter().map(Value::String).collect()));
    }
    Err(EngineError::domain(
        "sort() needs all numbers or all strings",
    ))
}

fn unique(arguments: &[Value]) -> Result<Value, EngineError> {
    let Value::Array(items) = &arguments[0] else {
        return Err(EngineError::domain(format!(
            "unique() works on arrays, got {}",
            arguments[0].kind_name()
        )));
    };
    let mut seen: Vec<Value> = Vec::new();
    for item in items {
        if !seen.contains(item) {
            seen.push(item.clone());
        }
    }
    Ok(Value::Array(seen))
}

fn reverse(arguments: &[Value]) -> Result<Value, EngineError> {
    match &arguments[0] {
        Value::Array(items) => Ok(Value::Array(items.iter().rev().cloned().collect())),
        Value::String(text) => Ok(Value::String(text.chars().rev().collect())),
        other => Err(EngineError::domain(format!(
            "reverse() works on arrays and strings, got {}",
            other.kind_name()
        ))),
    }
}

fn seq(arguments: &[Value]) -> Result<Value, EngineError> {
    let from = arguments[0].as_number("seq's start")?;
    let to = arguments[1].as_number("seq's end")?;
    let step = if arguments.len() > 2 {
        let step = arguments[2].as_number("seq's step")?;
        if step.is_zero() {
            return Err(EngineError::domain("seq's step can't be 0"));
        }
        step
    } else if from <= to {
        BigDecimal::one()
    } else {
        -BigDecimal::one()
    };
    let mut values: Vec<Value> = Vec::new();
    let mut current = from;
    loop {
        let in_range = if step.is_negative() {
            current >= to
        } else {
            current <= to
        };
        if !in_range {
            break;
        }
        values.push(Value::Number(current.clone()));
        if values.len() >= 100_000 {
            return Err(EngineError::domain("seq spans more than 100,000 values"));
        }
        current = &current + &step;
    }
    Ok(Value::Array(values))
}
