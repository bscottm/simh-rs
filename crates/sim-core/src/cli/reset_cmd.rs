// SPDX-License-Identifier: MIT

use nom::{
    character::complete::space0,
    combinator::{complete, opt, recognize, verify},
    error::{context, ErrorKind},
    sequence::terminated,
    Parser,
};

use crate::env::SimRequest;

use crate::cli::{
    cli_error::{CLIError, CLIErrorKind},
    cmd_repl::CmdContext,
    cmd_table::CLIResult,
    parsers::{consume_eol, parse_device_name},
    repl_state::REPLState,
    span::Span,
};

enum ResetCommand {
    All,
    Specific(String),
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
/// "RESET" command action function.
///
/// Reset the simulator's processor and devices to an initial state.
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
pub fn reset_command<'a>(context: &mut CmdContext, args: Span<'a>) -> Result<(), CLIError> {
    // Note the ".1" - extracts the ResetCommand from the (remainder, result)
    // tuple.
    let cmd = parse_reset_args(context.state).parse(args)?.1;
    let what = match cmd {
        ResetCommand::All => None,
        ResetCommand::Specific(dev) => Some(dev),
    };

    context.state.transact(args, "RESET", SimRequest::Reset(what))?;

    writeln!(context.state.output_sink.borrow_mut(), "Reset complete.").ok();
    Ok(())
}

fn parse_reset_args<'a>(
    repl: &'a REPLState,
) -> impl Parser<Span<'a>, Output = ResetCommand, Error = CLIError> {
    terminated(|input| reset_arg_parser(input, repl), consume_eol())
}

/// Parse RESET's argument, verifying that if provided it is either "ALL" or a valid device.
fn reset_arg_parser<'a>(span: Span<'a>, repl: &'a REPLState) -> CLIResult<'a, ResetCommand> {
    let (span, _) = space0.parse(span)?;
    context(
        "RESET argument",
        verify(
            opt(recognize(complete(parse_device_name))),
            // Validate: if a device name is given, it must be "ALL" or an existing device.
            |opt_span: &Option<Span<'a>>| {
                opt_span.as_ref().map_or(true, |s| {
                    s.input.to_uppercase() == "ALL" || repl.valid_device(s.input)
                })
            },
        ),
    )
    .parse(span)
    .map_err(|err| {
        let cli_err = match err {
            nom::Err::Error(e) => match e.kind {
                CLIErrorKind::Nom(ErrorKind::Verify) => {
                    CLIError::invalid_device(span, "RESET arguments", span.input)
                }
                _ => e,
            },
            nom::Err::Failure(e) => e,
            nom::Err::Incomplete(_) => CLIError::incomplete_input(span, "reset_arg"),
        };
        nom::Err::Error(cli_err)
    })
    .map(|(remainder, opt_span)| {
        let cmd = opt_span.as_ref().map_or(ResetCommand::All, |s| {
            if s.input.to_uppercase() == "ALL" {
                ResetCommand::All
            } else {
                // The unwrap here is safe because verification has already confirmed that the device is valid.
                ResetCommand::Specific(s.input.to_string())
            }
        });
        (remainder, cmd)
    })
}
