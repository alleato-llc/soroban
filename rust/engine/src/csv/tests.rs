//! Tests for CSV parse/encode round-tripping.

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
