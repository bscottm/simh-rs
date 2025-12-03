use crate::cli::{cli_error::CLIError, cmd_repl::CmdContext, span::Span};
use sim_help::HelpDriver;

/// "HELP"
pub fn help_command(_context: &mut CmdContext, _args: Span) -> Result<(), CLIError> {
    let help = HelpDriver::new();

    help.show_help(&[]);
    println!("-----------------");
    help.show_help(&["examine"]);
    Ok(())
}
