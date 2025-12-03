// SPDX-License-Identifier: MIT

use crate::cli::{
    cli_error::CLIError,
    cmd_repl::CmdContext,
    cmd_trie::{CommandTrie, TrieMatch},
    dir_cmds::{cd_command, dir_command, file_copy_command, pwd_command},
    examine_cmd::examine_command,
    help_cmd::help_command,
    load_cmd::load_command,
    quit_cmd::quit_command,
    reset_cmd::reset_command,
    set_cmd::set_command,
    show_cmd::show_command,
    span::Span,
};
use nom::IResult;

/// Command action type alias
pub type CmdAction = fn(&mut CmdContext, Span) -> Result<(), CLIError>;

/// CLI's parsing result type
pub type CLIResult<'a, O> = IResult<Span<'a>, O, CLIError>;

/// Command table using generic trie
#[derive(Debug)]
pub struct CommandTable {
    trie: CommandTrie<CmdAction>,
}

impl CommandTable {
    /// Build the command table with all available commands
    pub fn new() -> Self {
        let mut table = CommandTrie::<CmdAction>::new();

        // Insert all commands
        table.insert("RESET", reset_command, false);
        table.insert("RE", reset_command, true); // Exact alias for RESET
        table.insert("EXAMINE", examine_command, false);
        table.insert("E", examine_command, true); // Exact alias for EXAMINE
        table.insert("IEXAMINE", cmd_placeholder, false);
        table.insert("IE", cmd_placeholder, true); // Exact alias for IEXAMINE
        table.insert("DEPOSIT", cmd_placeholder, false);
        table.insert("D", cmd_placeholder, true); // Exact alias for DEPOSIT
        table.insert("IDEPOSIT", cmd_placeholder, false);
        table.insert("ID", cmd_placeholder, true); // Exact alias for IDEPOSIT
        table.insert("EVALUATE", cmd_placeholder, false);
        table.insert("EVAL", cmd_placeholder, true); // Exact alias for EVALUATE
        table.insert("RUN", cmd_placeholder, false);
        table.insert("RU", cmd_placeholder, true); // Exact alias for RUN
        table.insert("GO", cmd_placeholder, false);
        table.insert("STEP", cmd_placeholder, false);
        table.insert("S", cmd_placeholder, true); // Exact alias for STEP
        table.insert("NEXT", cmd_placeholder, false);
        table.insert("N", cmd_placeholder, true); // Exact alias for NEXT
        table.insert("CONTINUE", cmd_placeholder, false);
        table.insert("CONT", cmd_placeholder, true); // Exact alias for CONTINUE
        table.insert("CO", cmd_placeholder, true); // Exact alias for CONTINUE
        table.insert("BOOT", cmd_placeholder, false);
        table.insert("BO", cmd_placeholder, true); // Exact alias for BOOT
        table.insert("BREAK", cmd_placeholder, false);
        table.insert("NOBREAK", cmd_placeholder, false);
        table.insert("DEBUG", cmd_placeholder, false);
        table.insert("NODEBUG", cmd_placeholder, false);
        table.insert("ATTACH", cmd_placeholder, false);
        table.insert("AT", cmd_placeholder, true); // Exact alias for ATTACH
        table.insert("DETACH", cmd_placeholder, false);
        table.insert("DET", cmd_placeholder, true); // Exact alias for DETACH
        table.insert("ASSIGN", cmd_placeholder, false);
        table.insert("DEASSIGN", cmd_placeholder, false);
        table.insert("SAVE", cmd_placeholder, false);
        table.insert("SA", cmd_placeholder, true); // Exact alias for SAVE
        table.insert("RESTORE", cmd_placeholder, false);
        table.insert("REST", cmd_placeholder, true); // Exact alias for RESTORE
        table.insert("GET", cmd_placeholder, false);
        table.insert("LOAD", load_command, false);
        table.insert("LO", load_command, true); // Exact alias for LOAD
        table.insert("DUMP", cmd_placeholder, false);
        table.insert("DU", cmd_placeholder, true); // Exact alias for DUMP
        table.insert("EXIT", quit_command, false);
        table.insert("QUIT", quit_command, false);
        table.insert("BYE", quit_command, false);
        table.insert("CD", cd_command, false);
        table.insert("PWD", pwd_command, false);
        table.insert("DIR", dir_command, false);
        table.insert("LS", dir_command, false);
        table.insert("TYPE", cmd_placeholder, false);
        table.insert("CAT", cmd_placeholder, false);
        table.insert("DELETE", cmd_placeholder, false);
        table.insert("DEL", cmd_placeholder, true); // Exact alias for DELETE
        table.insert("RM", cmd_placeholder, true); // Exact match to avoid RMDIR
        table.insert("COPY", file_copy_command, false);
        table.insert("CP", file_copy_command, false);
        table.insert("RENAME", cmd_placeholder, false);
        table.insert("MOVE", cmd_placeholder, false);
        table.insert("MV", cmd_placeholder, false);
        table.insert("MKDIR", cmd_placeholder, false);
        table.insert("RMDIR", cmd_placeholder, false);
        table.insert("SET", set_command, false);
        table.insert("SHOW", show_command, false);
        table.insert("DO", cmd_placeholder, false);
        table.insert("GOTO", cmd_placeholder, false);
        table.insert("RETURN", cmd_placeholder, false);
        table.insert("SHIFT", cmd_placeholder, false);
        table.insert("CALL", cmd_placeholder, false);
        table.insert("ON", cmd_placeholder, false);
        table.insert("IF", cmd_placeholder, false);
        table.insert("ELSE", cmd_placeholder, false);
        table.insert("PROCEED", cmd_placeholder, false);
        table.insert("IGNORE", cmd_placeholder, false);
        table.insert("ECHO", cmd_placeholder, false);
        table.insert("ECHOF", cmd_placeholder, false);
        table.insert("ASSERT", cmd_placeholder, false);
        table.insert("SEND", cmd_placeholder, false);
        table.insert("NOSEND", cmd_placeholder, false);
        table.insert("EXPECT", cmd_placeholder, false);
        table.insert("NOEXPECT", cmd_placeholder, false);
        table.insert("SLEEP", cmd_placeholder, false);
        table.insert("HELP", help_command, false);
        table.insert("H", help_command, true); // Exact alias for HELP
        table.insert("SCREENSHOT", cmd_placeholder, false);
        table.insert("TAR", cmd_placeholder, false);
        table.insert("CURL", cmd_placeholder, false);
        table.insert("RUNLIMIT", cmd_placeholder, false);
        table.insert("NORUNLIMIT", cmd_placeholder, false);
        table.insert("DISKINFO", cmd_placeholder, false);

        CommandTable { trie: table }
    }

    /// Execute a command by verb
    pub fn execute_command(
        &self,
        verb: &str,
        span: Span,
        remainder: Span,
        context: &mut CmdContext,
    ) -> Result<(), CLIError> {
        let verb = verb.to_uppercase();

        match self.trie.find(&verb) {
            TrieMatch::Exact(action) => action(context, remainder),
            TrieMatch::Ambiguous(matches) => {
                let names = matches.join(", ");
                Err(CLIError::ambiguous_command(span, "verb lookup", names))
            }
            TrieMatch::NotFound => Err(CLIError::unknown_command(span, "verb lookup", verb)),
        }
    }

    /// Get all command verbs registered in the table
    pub fn all_commands(&self) -> Vec<(&'static str, bool)> {
        self.trie.all_commands()
    }
}

impl Default for CommandTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Placeholder command action
fn cmd_placeholder(_context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "verb dispatch"))
}
