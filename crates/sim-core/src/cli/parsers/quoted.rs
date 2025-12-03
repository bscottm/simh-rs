// SPDX-License-Identifier: MIT

use nom::{
    character::complete::char,
    error::context,
    sequence::{preceded, terminated},
    Parser,
};

use crate::cli::{
    cli_error::{CLIError, CLIErrorKind, Location},
    cmd_table::CLIResult,
    span::Span,
};

/// Parse a quoted string (double quotes), handling escape sequences
pub fn quoted_string<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    context(
        "quoted string",
        terminated(preceded(char('"'), in_quotes), char('"')),
    )
    .parse(span)
}

/// Worker function for [`quoted_string`]
fn in_quotes<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    let mut result = String::new();
    let mut chars = span.input.chars();
    let mut consumed = 0;

    while let Some(c) = chars.next() {
        consumed += c.len_utf8();
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    consumed += escaped.len_utf8();
                    result.push(escaped);
                }
            }
            '"' => {
                // Don't consume the closing quote.
                consumed -= c.len_utf8();
                return Ok((
                    Span {
                        input: &span.input[consumed..],
                        offset: span.offset,
                        line: span.line,
                        column: span.column,
                    },
                    result,
                ));
            }
            _ => result.push(c),
        }
    }

    Err(nom::Err::Error(CLIError {
        input: span.input.to_string(),
        location: Location::from_input(span.input),
        context: vec!["quoted argument"],
        kind: CLIErrorKind::Message("No end quote found".to_string()),
    }))
}

#[cfg(test)]
mod quoted_string_tests {
    use super::{quoted_string, Span};

    #[test]
    fn parse_quoted_string() {
        let (remainder, qs) = quoted_string(Span::new("\"this is a quoted string\"")).unwrap();
        assert_eq!(qs, "this is a quoted string");
        assert!(remainder.input.is_empty(), "remainder should be empty.");

        let (remainder, qs) = quoted_string(Span::new("\"escaped \\\" quote\" rest")).unwrap();
        assert_eq!(qs, "escaped \" quote");
        assert_eq!(remainder.input, " rest", "remainder should be ' rest'.");

        let input = "\"this is an unterminated quote.";
        let unterminated = quoted_string(Span::new(input)).unwrap_err();
        if let nom::Err::Error(unquoted_err) = unterminated {
            assert!(
                format!("{}", unquoted_err).contains("No end quote found"),
                "Expected 'no end quote found', got {}",
                unquoted_err
            )
        } else {
            assert!(false, "Unquoted string test did not detect error.")
        }
    }
}
