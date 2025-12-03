// SPDX-License-Identifier: MIT

//! Publicly facing SIMH simulator command driver functionality.
//!
//! This is the command line interface (CLI) for SIMH-RS. It is based on the original SIMH
//! CLI.

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Component modules and exports:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
pub mod cli_error;
pub mod cmd_repl;
pub mod formats;
pub mod parsers;
pub mod repl_state;

mod cmd_reader;
mod cmd_table;
mod cmd_trie;
mod dir_cmds;
mod examine_cmd;
mod help_cmd;
mod legacy;
mod load_cmd;
mod quit_cmd;
mod reset_cmd;
mod set_cmd;
mod show_cmd;
mod span;

// InputRadix and Span are re-exported for nullsystem tests. InputRadix is also used by the simulators to set
// their default radix.
//
// CommandTable: Also exported for nullsystem tests.
pub use crate::cli::{cmd_repl::CmdREPL, cmd_table::CommandTable, parsers::InputRadix, span::Span};
