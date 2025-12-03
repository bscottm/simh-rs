// SPDX-License-Identifier: MIT

use nom::{
    bytes::complete::take_while1,
    character::complete::{char, space0, space1},
    combinator::{cut, map},
    multi::fold_many0,
    sequence::preceded,
    Parser,
};

use crate::cli::{cli_error::CLIError, cmd_table::CLIResult, span::Span};

/// Parse validated flag groups from the input.
///
/// The first flag group is allowed to follow any amount of leading whitespace,
/// but any subsequent flag groups must be preceded by at least one whitespace.
/// This means "-a -bc" is valid, but "-a-bc" is an error.
///
/// ## Parameters
/// - `valid_flags`: A string containing all valid flag characters
///
/// ## Returns
/// A vector of characters with the flag characters.
pub fn parse_switches<'a>(valid_flags: &'static str) -> impl FnMut(Span<'a>) -> CLIResult<'a, Vec<char>> {
    parse_switches_internal(Some(valid_flags))
}

/// Parse flag groups from the input.
///
/// This is a more permissive version of [`parse_switches`] that does not validate the flags against a known
/// set.
pub fn parse_all_switches<'a>() -> impl FnMut(Span<'a>) -> CLIResult<'a, Vec<char>> {
    parse_switches_internal(None)
}

/// Internal helper to parse flag groups, with optional validation against a set of valid flags.
fn parse_switches_internal<'a>(
    valid_flags: Option<&'static str>,
) -> impl FnMut(Span<'a>) -> CLIResult<'a, Vec<char>> {
    move |span: Span<'a>| {
        // Consume any leading whitespace.
        let (trimmed, _) = space0(span)?;
        // Parse the first flag group with cut after the '-'
        let (remainder, first) = preceded(char('-'), cut(parse_flag_group(valid_flags))).parse(trimmed)?;

        // Parse additional flag groups, each of which must be preceded by at least one space.
        fold_many0(
            preceded(space1, preceded(char('-'), cut(parse_flag_group(valid_flags)))),
            move || first.clone(),
            |mut acc, group| {
                acc.extend(group);
                acc
            },
        )
        .parse(remainder)
    }
}

/// Parse a single flag group (without the leading '-'), validating each character
fn parse_flag_group<'a>(
    valid_flags: Option<&'static str>,
) -> impl FnMut(Span<'a>) -> CLIResult<'a, Vec<char>> {
    move |span: Span<'a>| {
        let (remainder, chars) = map(take_while1(|c: char| c.is_alphabetic()), |s: Span<'a>| {
            s.input.chars().collect::<Vec<char>>()
        })
        .parse(span)?;

        // Validate each flag
        if let Some(flags) = valid_flags {
            for &flag in &chars {
                if !flags.contains(flag) {
                    return Err(nom::Err::Error(CLIError::invalid_flag(
                        span,
                        "switch flags",
                        flag,
                    )));
                }
            }
        }

        Ok((remainder, chars))
    }
}
