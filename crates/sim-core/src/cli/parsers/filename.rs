// SPDX-License-Identifier: MIT

use nom::{branch::alt, bytes::complete::take_while1, character::complete::space0, Parser};

use crate::cli::{cmd_table::CLIResult, parsers::quoted::quoted_string, span::Span};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Nom parsers for arguments:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// File name(-ish) token parser with glob characters.
///
/// Accepts a quoted string or an unquoted string that contains path characters (slashes, dots, alphanumeric) and
/// globb-able characters. Skips leading whitespace.
pub fn filename_token<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    let (span, _) = space0.parse(span)?;
    alt((quoted_string, unquoted_filename)).parse(span)
}

fn unquoted_filename<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    take_while1(|c: char| c.is_alphanumeric() || "/\\._-:*?[]{,}~".contains(c))
        .map(|s: Span<'a>| s.input.to_string())
        .parse(span)
}

/// File name(-ish) token parser.
///
/// Accepts a quoted string or an unquoted string that contains path characters (slashes, dots, alphanumeric)
/// Skips leading whitespace.
pub fn filename_noglob<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    let (span, _) = space0.parse(span)?;
    alt((quoted_string, noglobbed_filename)).parse(span)
}

fn noglobbed_filename<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    take_while1(|c: char| c.is_alphanumeric() || "/\\._-:".contains(c))
        .map(|s: Span<'a>| s.input.to_string())
        .parse(span)
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Tests (that don't require a simulation environment):
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[cfg(test)]
mod parse_filenames {
    use super::*;

    // Test the filename_token parser.
    #[test]
    fn parse_filename_token() {
        let (remainder, fname) = filename_token(Span::new(r#""quoted filename.txt" rest"#)).unwrap();
        assert_eq!(fname, "quoted filename.txt");
        assert_eq!(remainder.input, " rest", "remainder should be ' rest'.");

        let (remainder, fname) = filename_token(Span::new(r#"unquoted_filename-123.txt rest"#)).unwrap();
        assert_eq!(fname, "unquoted_filename-123.txt");
        assert_eq!(remainder.input, " rest", "remainder should be ' rest'.");

        let (_, fname) = filename_token(Span::new(r#"globbing*.txt"#)).unwrap();
        assert_eq!(fname, "globbing*.txt");
    }
}
