// SPDX-License-Identifier: MIT

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while1},
    character::complete::{char, multispace1, one_of, space0},
    combinator::{complete, cut, map, opt, verify},
    error::context,
    multi::separated_list1,
    sequence::{delimited, pair, preceded, terminated},
    Err as NomErr, Parser,
};

use crate::env::MEM_RESOURCE_NAME;

use crate::cli::{
    cli_error::CLIError, cmd_repl::CmdContext, cmd_table::CLIResult, parsers::*, repl_state::REPLState,
    span::Span,
};

pub struct ExamineCommand {
    pub outfile: Option<String>,
    pub switches: Vec<char>,
    pub mask_op: Option<MaskOperation>,
    pub search_op: Option<SearchOperation>,
    pub what: ExamineArgs,
}

#[derive(Debug)]
pub struct ExamineArgs {
    pub device: Option<String>,
    pub arg: Examinee,
}

#[derive(Debug)]
pub enum Examinee {
    /// Show all non-MEM resources for the device.
    State,
    /// Like State but include all array elements.
    All,
    /// Specific named resources (with optional array slice).
    ResourceList { resources: Vec<ExaminedResource> },
}

#[derive(Debug, PartialEq)]
pub enum ExaminedResource {
    Whole {
        res_name: String,
    },
    Slice {
        res_name: String,
        start_offset: usize,
        end_offset: usize,
    },
}

impl ExaminedResource {
    pub fn resource_name(&self) -> &String {
        match self {
            Self::Whole { res_name } => res_name,
            Self::Slice { res_name, .. } => res_name,
        }
    }

    pub fn from_parts(res_name: String, range: Option<(usize, usize)>) -> Self {
        match range {
            None => Self::Whole { res_name },
            Some((start, end)) => Self::Slice {
                res_name,
                start_offset: start,
                end_offset: end,
            },
        }
    }
}

pub const EXAMINE_CTX: &str = "EXAMINE";

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Parsers
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

pub fn parse_examine_command<'a>(context: &mut CmdContext, args: Span<'a>) -> CLIResult<'a, ExamineCommand> {
    let radix = context.state.input_radix();

    map(
        (
            parse_outfile,
            opt(parse_switches("2achdmo")),
            opt(parse_mask_op(radix)),
            opt(parse_search_op(radix)),
            |span| parse_examinee(context.state, span),
        ),
        |(outfile, switches, mask_op, search_op, what)| ExamineCommand {
            outfile,
            switches: switches.unwrap_or_default(),
            mask_op,
            search_op,
            what,
        },
    )
    .parse(args)
}

pub fn parse_outfile<'a>(span: Span<'a>) -> CLIResult<'a, Option<String>> {
    let (span, _) = space0.parse(span)?;

    context(
        "EXAMINE result file",
        opt(map(pair(tag("@"), filename_noglob), |result| result.1)),
    )
    .parse(span)
}

fn parse_examinee<'a>(state: &REPLState, span: Span<'a>) -> CLIResult<'a, ExamineArgs> {
    let (span, _) = space0.parse(span)?;

    let (span, opt_device_name) = opt(terminated(parse_device_name, multispace1)).parse(span)?;

    if let Some(ref dev) = opt_device_name {
        if !state.valid_device(dev) {
            return Err(nom::Err::Error(CLIError::invalid_device(span, EXAMINE_CTX, dev)));
        }
    }

    let (remainder, arg) = alt((
        map(terminated(tag_no_case("state"), cut(consume_eol())), |_| {
            Examinee::State
        }),
        map(terminated(tag_no_case("all"), cut(consume_eol())), |_| {
            Examinee::All
        }),
        map(
            terminated(
                |s| parse_resource_list(state.input_radix(), s),
                cut(consume_eol()),
            ),
            |resources| Examinee::ResourceList { resources },
        ),
        map(
            terminated(
                |s| parse_bare_address_range(state.input_radix(), s),
                cut(consume_eol()),
            ),
            |range| Examinee::ResourceList {
                resources: vec![range],
            },
        ),
    ))
    .parse(span)?;

    Ok((
        remainder,
        ExamineArgs {
            device: opt_device_name.map(|s| s.to_string()),
            arg,
        },
    ))
}

fn parse_resource<'a>(radix: InputRadix, span: Span<'a>) -> CLIResult<'a, ExaminedResource> {
    preceded(
        space0,
        alt((
            |span| parse_named_resource(radix, span),
            |span| parse_bare_address_range(radix, span),
        )),
    )
    .parse(span)
}

fn parse_named_resource<'a>(radix: InputRadix, span: Span<'a>) -> CLIResult<'a, ExaminedResource> {
    complete((
        context(
            "resource name",
            verify(
                take_while1(|c: char| c.is_alphanumeric() || "_-".contains(c)),
                |s: &Span| s.input.chars().next().map_or(false, |c| c.is_alphabetic()),
            ),
        ),
        context(
            "[range]",
            opt(delimited(
                preceded(space0, char('[')),
                |span| parse_array_range(radix, span),
                preceded(space0, char(']')),
            )),
        ),
    ))
    .parse(span)
    .map(|(remainder, (resource_span, offset_span))| {
        (
            remainder,
            ExaminedResource::from_parts(resource_span.input.to_uppercase(), offset_span),
        )
    })
}

pub fn parse_resource_list<'a>(radix: InputRadix, span: Span<'a>) -> CLIResult<'a, Vec<ExaminedResource>> {
    let (span, _) = space0.parse(span)?;
    context(
        "resource list",
        separated_list1(char(','), |s| parse_resource(radix, s)),
    )
    .parse(span)
}

pub fn parse_array_range<'a>(radix: InputRadix, span: Span<'a>) -> CLIResult<'a, (usize, usize)> {
    // 1. Parse the structure and map the logic in one go
    let (remainder, (start, end)) = map(
        (
            preceded(space0, context("start array range", parse_scalar(radix))),
            opt(pair(
                preceded(space0, one_of("-:/")),
                preceded(space0, context("range end or length", parse_scalar(radix))),
            )),
        ),
        |(s, opt)| {
            let start = s as usize;
            match opt {
                Some(('/', len)) => (start, start + (len as usize).saturating_sub(1)),
                Some((_, end)) => (start, end as usize),
                None => (start, start),
            }
        },
    )
    .parse(span)?;

    // 2. Immediate validation check
    if start <= end {
        Ok((remainder, (start, end)))
    } else {
        Err(NomErr::Failure(CLIError::message(
            span,
            "range",
            "end occurs before start".to_string(),
        )))
    }
}

pub fn parse_bare_address_range<'a>(radix: InputRadix, span: Span<'a>) -> CLIResult<'a, ExaminedResource> {
    let (span, _) = space0.parse(span)?;
    context("address range", |s| parse_array_range(radix, s))
        .parse(span)
        .map(|(remainder, range)| {
            (
                remainder,
                ExaminedResource::from_parts(MEM_RESOURCE_NAME.to_string(), Some(range)),
            )
        })
}
