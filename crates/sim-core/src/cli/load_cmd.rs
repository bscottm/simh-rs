// SPDX-License-Identifier: MIT

use crate::{
    cli::{cli_error::CLIError, cmd_repl::CmdContext, parsers::parse_load_command, Span},
    env::SimRequest,
};

pub fn load_command<'a>(context: &mut CmdContext, args: Span<'a>) -> Result<(), CLIError> {
    let (_, cmd) = parse_load_command(context, args)?;

    context.state.transact(
        args,
        "load",
        SimRequest::LoadFile {
            flags: cmd.switches,
            path: cmd.path,
        },
    )?;
    Ok(())
}
