// SPDX-License-Identifier: MIT

use std::fmt;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::{complete::satisfy, one_of},
    combinator::cut,
    multi::fold_many1,
    sequence::preceded,
    Parser,
};

use crate::cli::{cli_error::CLIError, cmd_table::CLIResult, parsers::types::InputRadix, span::Span};

/// Parse scalars
///
/// Parse a scalar value, which can be prefixed with "0d" (decimal), "0b" (binary), "0o" (octal) or "0x"
/// (hexadecimal) to override the default radix.
///
/// ## Parameters
/// - `radix`: The default radix if the input span isn't prefixed.
///
/// ## Returns
/// The accumulated value parsed from the input span. Parsing stops at the first unrecognized digit.
pub fn parse_scalar<'a>(radix: InputRadix) -> impl FnMut(Span<'a>) -> CLIResult<'a, u64> {
    move |span: Span<'a>| {
        alt((
            preceded(tag("0d"), cut(parse_decimal)),
            preceded(tag("0b"), cut(parse_binary)),
            preceded(tag("0o"), cut(parse_octal)),
            preceded(tag("0x"), cut(parse_hexadecimal)),
            // Fallback to the unprefixed default radix...
            cut(match radix {
                InputRadix::Bin => parse_binary,
                InputRadix::Dec => parse_decimal,
                InputRadix::Hex => parse_hexadecimal,
                InputRadix::Oct => parse_octal,
            }),
        ))
        .parse(span)
    }
}

/// Parse a binary scalar
fn parse_binary<'a>(span: Span<'a>) -> CLIResult<'a, u64> {
    fold_many1(
        one_of("01"),
        || 0u64,
        |acc, c| (acc << 1) | c.to_digit(2).unwrap() as u64,
    )
    .parse(span)
}

/// Parse a decimal scalar
fn parse_decimal<'a>(span: Span<'a>) -> CLIResult<'a, u64> {
    fold_many1(
        satisfy(|c| c.is_ascii_digit()),
        || 0u64,
        |acc, c| acc * 10 + c.to_digit(10).unwrap() as u64,
    )
    .parse(span)
}

/// Parse an octal scalar
fn parse_octal<'a>(span: Span<'a>) -> CLIResult<'a, u64> {
    let (remainder, digits) = take_while1(|c: char| c.is_ascii_digit()).parse(span)?;

    // Convert the octal string to u64
    let value = u64::from_str_radix(digits.input, 8).map_err(|_| {
        nom::Err::Error(CLIError::invalid_scalar(
            digits,
            "octal constant",
            InputRadix::Oct,
        ))
    })?;

    Ok((remainder, value))
}

/// Parse a hexadecimal scalar
fn parse_hexadecimal<'a>(span: Span<'a>) -> CLIResult<'a, u64> {
    fold_many1(
        satisfy(|c| c.is_ascii_hexdigit()),
        || 0u64,
        |acc, c| (acc << 4) | c.to_digit(16).unwrap() as u64,
    )
    .parse(span)
}

impl fmt::Display for InputRadix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Bin => "binary",
                Self::Dec => "decimal",
                Self::Oct => "octal",
                Self::Hex => "hexadecimal",
            }
        )
    }
}
