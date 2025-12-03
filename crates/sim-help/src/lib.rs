// SPDX-License-Identifier: MIT

mod helpdriver;
mod helpfuncs;

// Re-exports to avoid long module (sim_help::simhelp) paths.
pub use crate::{
    helpdriver::HelpDriver,
    helpfuncs::{
        find_command_help,
        find_node_by_path,
        find_topic,
        get_children,
        get_node_path,
        HelpNode,
        Topic,
        COMMAND_MAP,
        COMMAND_SUBSECTIONS,
        // nullsystem integration tests need these exports (sigh!)
        HELP_NODES,
        HELP_SYSTEM,
        SEE_ALSO,
        TOPICS,
    },
};
