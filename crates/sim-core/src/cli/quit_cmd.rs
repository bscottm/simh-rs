// SPDX-License-Identifier: MIT

use crate::cli::{cli_error::CLIError, cmd_repl::CmdContext, repl_state::InterpState, span::Span};

/// "BYE", "EXIT" and "QUIT" command action function.
///
/// Sets the [`crate::env::SimEnvironment`]'s status to [`InterpState::Quitting`], which will cause
/// the simulation environment pop everything off the [`crate::cli::cmd_repl::CmdREPL::inputs`] stack
/// and exit.
pub fn quit_command(context: &mut CmdContext, _args: Span<'_>) -> Result<(), CLIError> {
    context.state.cli_status = InterpState::Quitting;
    Ok(())
}
