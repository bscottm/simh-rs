// SPDX-License-Identifier: MIT

use nom::{
    bytes::complete::take_while1,
    character::complete::space0,
    combinator::{map, verify},
    error::ErrorKind,
    sequence::preceded,
    Parser,
};

use crate::cli::{
    cli_error::{CLIError, CLIErrorKind},
    cmd_table::CLIResult,
    repl_state::REPLState,
    span::Span,
};

/// Parse a device name (not validated)
pub fn parse_device_name<'a>(span: Span<'a>) -> CLIResult<'a, String> {
    preceded(
        space0,
        take_while1(|c: char| {
            (c.is_alphabetic() || ['_', '-'].contains(&c)) && !c.is_whitespace() && c != '#'
        })
        .map(|s: Span<'a>| s.input.to_uppercase()),
    )
    .parse(span)
}

/// Parse a device name, verifying that it exists in the simulator environment.
pub fn parse_valid_device_name<'a, 'r>(
    repl: &'r REPLState,
) -> impl FnMut(Span<'a>) -> CLIResult<'a, String> + 'r {
    move |input: Span<'a>| {
        let (input, _) = space0.parse(input)?;

        map(
            verify(parse_device_name, |name| repl.valid_device(name)),
            |name| name.to_string(),
        )
        .parse(input)
        .map_err(|err| {
            let cli_err = match err {
                nom::Err::Error(e) => match e.kind {
                    CLIErrorKind::Nom(ErrorKind::Verify) => {
                        CLIError::invalid_device(input, "valid device name", input.input)
                    }
                    _ => e,
                },
                nom::Err::Failure(e) => e,
                nom::Err::Incomplete(_) => CLIError::incomplete_input(input, "valid device name"),
            };
            nom::Err::Error(cli_err)
        })
    }
}
