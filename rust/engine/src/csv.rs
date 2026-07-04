//! Minimal RFC 4180-style CSV — the port of the `CSV` enum in
//! `swift/Engine/Sources/SorobanEngine/Persistence/DataStore.swift`:
//! quoted fields, escaped quotes (`""`), CR/LF/CRLF line ends.
//!
//! The contract: `encode` is `parse`'s exact inverse — fields are quoted only
//! when they need it, and `parse(&encode(rows)) == rows`.

/// Parses CSV text into rows of fields.
///
/// Port note: Swift matches `"\r\n"` as ONE `Character` (grapheme cluster);
/// here CR and LF arrive as separate `char`s, but the outcome is identical —
/// the LF after a CR ends an empty single-field row, which the
/// blank-row-dropping rule discards, exactly as Swift discards blank rows.
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut iterator = text.chars();
    let mut pending: Option<char> = None;

    fn end_field(row: &mut Vec<String>, field: &mut String) {
        row.push(std::mem::take(field));
    }
    fn end_row(rows: &mut Vec<Vec<String>>, row: &mut Vec<String>, field: &mut String) {
        end_field(row, field);
        let finished = std::mem::take(row);
        if !(finished.len() == 1 && finished[0].is_empty()) {
            rows.push(finished);
        }
    }

    while let Some(c) = pending.take().or_else(|| iterator.next()) {
        if in_quotes {
            if c == '"' {
                match iterator.next() {
                    Some('"') => field.push('"'), // escaped quote
                    Some(next) => {
                        in_quotes = false; // closing quote
                        pending = Some(next);
                    }
                    None => in_quotes = false,
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() => in_quotes = true,
                ',' => end_field(&mut row, &mut field),
                '\n' | '\r' => end_row(&mut rows, &mut row, &mut field),
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        end_row(&mut rows, &mut row, &mut field);
    }
    rows
}

/// The inverse of `parse`: rows → RFC 4180-style text (`\n` line ends).
/// Fields are quoted only when they need it (comma, quote, or newline);
/// quotes double inside quoted fields. `parse(&encode(rows)) == rows`.
pub fn encode(rows: &[Vec<String>]) -> String {
    let body = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|field| encode_field(field))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if rows.is_empty() {
        body
    } else {
        body + "\n"
    }
}

/// Swift quotes on `Character.isNewline` — the same set of newline scalars.
fn is_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

fn encode_field(field: &str) -> String {
    if !field.chars().any(|c| c == ',' || c == '"' || is_newline(c)) {
        return field.to_string();
    }
    format!("\"{}\"", field.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(table: &[&[&str]]) -> Vec<Vec<String>> {
        table
            .iter()
            .map(|row| row.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    /// Port of `CSVTests.encodeQuotesOnlyWhenNeeded`.
    #[test]
    fn encode_quotes_only_when_needed() {
        let table = rows(&[
            &["plain", "with,comma", "with \"quotes\"", "multi\nline", ""],
            &["1200", "second row"],
        ]);
        let encoded = encode(&table);
        assert_eq!(
            encoded,
            "plain,\"with,comma\",\"with \"\"quotes\"\"\",\"multi\nline\",\n1200,second row\n"
        );
        // The contract: a perfect round-trip through parse.
        assert_eq!(parse(&encoded), table);
        assert_eq!(encode(&[]), "");
    }

    /// Port of `CSVTests.coversTheUsualSuspects`.
    #[test]
    fn covers_the_usual_suspects() {
        assert_eq!(
            parse("a,b,c\n1,2,3"),
            rows(&[&["a", "b", "c"], &["1", "2", "3"]])
        );
        assert_eq!(
            parse("a,\"b, with comma\",c"),
            rows(&[&["a", "b, with comma", "c"]])
        );
        assert_eq!(
            parse("\"he said \"\"hi\"\"\",2"),
            rows(&[&["he said \"hi\"", "2"]])
        );
        assert_eq!(parse("a,b\r\n1,2\r\n"), rows(&[&["a", "b"], &["1", "2"]])); // CRLF
        assert_eq!(parse("a,b\r1,2"), rows(&[&["a", "b"], &["1", "2"]])); // bare CR
        assert_eq!(parse("\"multi\nline\",x"), rows(&[&["multi\nline", "x"]])); // newline in quotes
        assert_eq!(parse("a,,c"), rows(&[&["a", "", "c"]])); // empty field
        assert_eq!(parse(""), Vec::<Vec<String>>::new());
        assert_eq!(parse("solo"), rows(&[&["solo"]]));
    }
}
