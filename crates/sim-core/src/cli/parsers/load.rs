// SPDX-License-Identifier: MIT

use crate::cli::{
    cmd_repl::CmdContext,
    cmd_table::CLIResult,
    parsers::{consume_eol, filename_noglob, parse_all_switches},
    Span,
};
use nom::{
    combinator::{cut, map, opt},
    sequence::{pair, terminated},
    Parser,
};

pub struct LoadCommand {
    pub switches: Vec<char>,
    pub path: String,
}

pub fn parse_load_command<'a>(_context: &mut CmdContext, args: Span<'a>) -> CLIResult<'a, LoadCommand> {
    map(
        terminated(pair(opt(parse_all_switches()), filename_noglob), consume_eol()),
        |(switches, path)| LoadCommand {
            switches: switches.unwrap_or_default(),
            path,
        },
    )
    .parse(args)
}
