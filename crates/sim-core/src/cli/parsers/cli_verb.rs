// SPDX-License-Identifier: MIT

use nom::{
    branch::alt,
    character::complete::{alpha1, char, line_ending, not_line_ending, space0},
    combinator::{complete, map, opt, value},
    multi::many0,
    sequence::{pair, preceded},
    IResult, Parser,
};

use crate::cli::{cli_error::CLIError, cmd_table::CLIResult, span::Span};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Command parsing:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

// Parse a comment line (# to end of line)
fn skip_comment<'a>(span: Span<'a>) -> CLIResult<'a, ()> {
    value((), pair(preceded(space0, char('#')), not_line_ending)).parse(span)
}

// Skip whitespace, comments, and blank lines
fn skip_noise<'a>(span: Span<'a>) -> CLIResult<'a, ()> {
    value(
        (),
        many0(alt((
            value((), line_ending),
            value((), preceded(space0, line_ending)),
            skip_comment,
        ))),
    )
    .parse(span)
}

// Main command parser that skips noise first
pub fn command_line<'a>(span: Span<'a>) -> CLIResult<'a, Option<(String, Span<'a>)>> {
    preceded(
        skip_noise,
        alt((
            // Shell command (already handles leading space via skip_noise)
            map(preceded(char('!'), not_line_ending), |s: Span<'a>| {
                Some((format!("!{}", s.input.trim()), s))
            }),
            // Regular command
            preceded(
                space0,
                map(pair(alpha1, not_line_ending), |(cmd, remainder): (Span, Span)| {
                    Some((cmd.input.to_string(), remainder))
                }),
            ),
            // EOF
            value(None, space0),
        )),
    )
    .parse(span)
}

/// Consume the input to the end-of-the line
///
/// Skips remaining space and optional comment. Helps ensure that there
/// is no remaining parsable input to a command.
///
/// Returns a [`CLIError`] when there is extraneous input that isn't
/// whitespace or a comment.
pub fn consume_eol<'a>() -> impl Fn(Span<'a>) -> IResult<Span<'a>, (), CLIError> + 'a {
    move |span: Span<'a>| {
        let result = (space0, opt(complete(preceded(char('#'), not_line_ending)))).parse(span);

        match result {
            Ok((remainder, _)) if !remainder.input.is_empty() => Err(nom::Err::Error(
                CLIError::extraneous_input(span, "command end-of-line"),
            )),
            Ok((remainder, _)) => Ok((remainder, ())),
            Err(err) => Err(err),
        }
    }
}
