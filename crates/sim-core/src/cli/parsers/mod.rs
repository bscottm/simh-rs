// SPDX-License-Identifier: MIT

//! [nom](https://crates.io/crates/nom) parser combinators for CLI parsing.
//!
//! # Roadmap
//!
//! - `cli_verb`: Parses a commannd line up to the verb, e.g. "SET", "SHOW", "EXAMINE", skipping comments and
//!    whitespace.
//! - `filename`:
//! - `mask_search_ops`: Masking and search operators for the "EXAMINE" command.
//! - `numerics`: Exports [`parse_scalar`]; parses numeric quantities for the available [`InputRadix`]-es.
//! - `types`: Types and their implementations used in this submodule.

mod cli_verb;
mod devname;
mod examine;
mod filename;
mod load;
mod mask_search_ops;
mod numerics;
mod quoted;
mod switches;
mod types;

// Re-exports (cuts down on submodule paths):
pub use cli_verb::{command_line, consume_eol};
pub use devname::{parse_device_name, parse_valid_device_name};
pub use examine::{
    parse_array_range, parse_bare_address_range, parse_examine_command, parse_outfile, parse_resource_list,
    ExamineArgs, ExamineCommand, ExaminedResource, Examinee, EXAMINE_CTX,
};
pub use filename::{filename_noglob, filename_token};
pub use load::parse_load_command;
pub use mask_search_ops::{parse_mask_op, parse_search_op, MaskOperation, SearchOperation};
pub use numerics::parse_scalar;
pub use quoted::quoted_string;
pub use switches::{parse_all_switches, parse_switches};
pub use types::InputRadix;
