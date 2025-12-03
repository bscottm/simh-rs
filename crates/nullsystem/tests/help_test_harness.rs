// tests/help_coverage.rs - Test harness for help system coverage

use std::collections::HashSet;

use sim_core::cli::CommandTable;

use sim_help::{
    find_command_help, find_node_by_path, COMMAND_MAP, COMMAND_SUBSECTIONS, HELP_NODES, HELP_SYSTEM,
    SEE_ALSO, TOPICS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_commands_have_help() {
        let mut missing_commands = Vec::new();

        for (cmd_name, _exact_only) in CommandTable::new().all_commands() {
            // Check if command has help mapping
            if find_command_help(cmd_name, None).is_none() {
                missing_commands.push(cmd_name);
            }
        }

        if !missing_commands.is_empty() {
            panic!(
                "The following commands from CommandTable are missing help documentation:\n  {}",
                missing_commands.join("\n  ")
            );
        }
    }

    #[test]
    fn test_no_orphaned_help_entries() {
        let mut command_names = HashSet::new();

        // Collect all command names
        for (cmd_name, _) in CommandTable::new().all_commands() {
            command_names.insert(cmd_name.to_uppercase());
        }

        let mut orphaned = Vec::new();

        // Check if all help mappings correspond to actual commands
        for mapping in COMMAND_MAP {
            if !command_names.contains(mapping.command) {
                orphaned.push(mapping.command);
            }
        }

        if !orphaned.is_empty() {
            panic!(
                "The following help entries don't correspond to any command in CommandTable:\n  {}",
                orphaned.join("\n  ")
            );
        }
    }

    #[test]
    fn test_all_help_paths_are_valid() {
        let mut invalid_paths = Vec::new();

        for mapping in COMMAND_MAP {
            if find_node_by_path(mapping.help_path).is_none() {
                invalid_paths.push(format!(
                    "{} -> {} (path not found)",
                    mapping.command, mapping.help_path
                ));
            }
        }

        if !invalid_paths.is_empty() {
            panic!(
                "The following help paths are invalid:\n  {}",
                invalid_paths.join("\n  ")
            );
        }
    }

    #[test]
    fn test_all_subsection_paths_are_valid() {
        let mut invalid_paths = Vec::new();

        for subsection in COMMAND_SUBSECTIONS {
            if find_node_by_path(subsection.help_path).is_none() {
                invalid_paths.push(format!(
                    "{} {} -> {} (path not found)",
                    subsection.command, subsection.subsection, subsection.help_path
                ));
            }
        }

        if !invalid_paths.is_empty() {
            panic!(
                "The following subsection paths are invalid:\n  {}",
                invalid_paths.join("\n  ")
            );
        }
    }

    #[test]
    fn test_all_nodes_have_valid_parents() {
        let mut invalid_parents = Vec::new();
        let mut node_ids = HashSet::new();

        // Collect all node IDs
        node_ids.insert(HELP_SYSTEM.root_id);
        for node in HELP_NODES {
            node_ids.insert(node.id);
        }

        // Check that all parent references are valid
        for node in HELP_NODES {
            if let Some(parent) = node.parent {
                if !node_ids.contains(parent) {
                    invalid_parents.push(format!("Node '{}' has invalid parent '{}'", node.id, parent));
                }
            }
        }

        if !invalid_parents.is_empty() {
            panic!(
                "The following nodes have invalid parent references:\n  {}",
                invalid_parents.join("\n  ")
            );
        }
    }

    #[test]
    fn test_all_children_references_are_valid() {
        let mut invalid_children = Vec::new();
        let mut node_ids = HashSet::new();

        // Collect all node IDs
        for node in HELP_NODES {
            node_ids.insert(node.id);
        }

        // Check that all children references are valid
        for node in HELP_NODES {
            for child_id in node.children {
                if !node_ids.contains(child_id) {
                    invalid_children.push(format!(
                        "Node '{}' references invalid child '{}'",
                        node.id, child_id
                    ));
                }
            }
        }

        if !invalid_children.is_empty() {
            panic!(
                "The following nodes have invalid children references:\n  {}",
                invalid_children.join("\n  ")
            );
        }
    }

    #[test]
    fn test_all_topic_nodes_are_valid() {
        let mut invalid_topic_nodes = Vec::new();

        for topic in TOPICS {
            for node_path in topic.nodes {
                if find_node_by_path(node_path).is_none() {
                    invalid_topic_nodes.push(format!(
                        "Topic '{}' references invalid node path '{}'",
                        topic.name, node_path
                    ));
                }
            }
        }

        if !invalid_topic_nodes.is_empty() {
            panic!(
                "The following topic node references are invalid:\n  {}",
                invalid_topic_nodes.join("\n  ")
            );
        }
    }

    #[test]
    fn test_all_see_also_references_are_valid() {
        let mut invalid_see_also = Vec::new();

        for (node_path, related_paths) in SEE_ALSO {
            // Check that the source node exists
            if find_node_by_path(node_path).is_none() {
                invalid_see_also.push(format!("See also source path '{}' not found", node_path));
            }

            // Check that all related nodes exist
            for related_path in *related_paths {
                if find_node_by_path(related_path).is_none() {
                    invalid_see_also.push(format!(
                        "See also: '{}' references invalid path '{}'",
                        node_path, related_path
                    ));
                }
            }
        }

        if !invalid_see_also.is_empty() {
            panic!(
                "The following see_also references are invalid:\n  {}",
                invalid_see_also.join("\n  ")
            );
        }
    }

    #[test]
    fn test_no_duplicate_node_ids() {
        let mut seen = HashSet::new();
        let mut duplicates = Vec::new();

        for node in HELP_NODES {
            if !seen.insert(node.id) {
                duplicates.push(node.id);
            }
        }

        if !duplicates.is_empty() {
            panic!(
                "The following node IDs are duplicated:\n  {}",
                duplicates.join("\n  ")
            );
        }
    }

    #[test]
    fn test_no_duplicate_commands_in_table() {
        let mut seen = HashSet::new();
        let mut duplicates = Vec::new();

        for (cmd_name, _) in CommandTable::new().all_commands() {
            if !seen.insert(cmd_name) {
                duplicates.push(cmd_name);
            }
        }

        if !duplicates.is_empty() {
            panic!(
                "The following commands are duplicated in COMMAND_TABLE:\n  {}",
                duplicates.join("\n  ")
            );
        }
    }

    #[test]
    fn test_exact_match_commands_documented() {
        // Commands with exact_only=true need special attention
        let exact_match_commands: Vec<&str> = CommandTable::new()
            .all_commands()
            .iter()
            .filter(|(_, exact_only)| *exact_only)
            .map(|(name, _)| *name)
            .collect();

        println!("\n=== Exact Match Commands ===");
        println!("The following commands require exact matching:");
        for cmd in &exact_match_commands {
            println!("  {}", cmd);
        }

        // Verify they all have help
        let mut missing = Vec::new();
        for cmd in &exact_match_commands {
            if find_command_help(cmd, None).is_none() {
                missing.push(*cmd);
            }
        }

        if !missing.is_empty() {
            panic!(
                "The following exact-match commands lack help:\n  {}",
                missing.join("\n  ")
            );
        }
    }

    #[test]
    fn test_command_coverage_report() {
        println!("\n=== Help System Coverage Report ===\n");

        let cmd_table = CommandTable::new().all_commands();
        let total_commands = cmd_table.len();
        let mut documented_commands = 0;
        let mut exact_only_count = 0;
        let mut missing = Vec::new();

        for (cmd_name, exact_only) in cmd_table {
            if exact_only {
                exact_only_count += 1;
            }

            if find_command_help(cmd_name, None).is_some() {
                documented_commands += 1;
            } else {
                missing.push(cmd_name);
            }
        }

        println!("Total commands in CommandTable: {}", total_commands);
        println!(
            "Commands documented: {}/{} ({:.1}%)",
            documented_commands,
            total_commands,
            (documented_commands as f64 / total_commands as f64) * 100.0
        );
        println!("Commands with exact_only flag: {}", exact_only_count);

        if !missing.is_empty() {
            println!("\nUndocumented commands:");
            for cmd in missing {
                println!("  {}", cmd);
            }
        }

        println!("\nHelp system statistics:");
        println!("  Total help nodes: {}", HELP_NODES.len());
        println!("  Total topics: {}", TOPICS.len());
        println!("  Total command mappings: {}", COMMAND_MAP.len());
        println!("  Total subsections: {}", COMMAND_SUBSECTIONS.len());
        println!("\n");
    }

    #[test]
    fn test_placeholder_commands_flagged() {
        // These commands still use cmd_placeholder and should be noted
        let placeholder_commands = [
            "IEXAMINE",
            "DEPOSIT",
            "IDEPOSIT",
            "EVALUATE",
            "RUN",
            "GO",
            "STEP",
            "NEXT",
            "N",
            "CONTINUE",
            "BOOT",
            "BREAK",
            "NOBREAK",
            "DEBUG",
            "NODEBUG",
            "ATTACH",
            "DETACH",
            "ASSIGN",
            "DEASSIGN",
            "SAVE",
            "RESTORE",
            "GET",
            "LOAD",
            "DUMP",
            "TYPE",
            "CAT",
            "DELETE",
            "RENAME",
            "MOVE",
            "MV",
            "MKDIR",
            "RMDIR",
            "SET",
            "SHOW",
            "DO",
            "GOTO",
            "RETURN",
            "SHIFT",
            "CALL",
            "ON",
            "IF",
            "ELSE",
            "PROCEED",
            "IGNORE",
            "ECHO",
            "ECHOF",
            "ASSERT",
            "SEND",
            "NOSEND",
            "EXPECT",
            "NOEXPECT",
            "SLEEP",
            "SCREENSHOT",
            "TAR",
            "CURL",
            "RUNLIMIT",
            "NORUNLIMIT",
            "TESTLIB",
            "DISKINFO",
        ];

        let mut documented_placeholders = 0;
        let mut undocumented_placeholders = Vec::new();

        for cmd in &placeholder_commands {
            if find_command_help(cmd, None).is_some() {
                documented_placeholders += 1;
            } else {
                undocumented_placeholders.push(*cmd);
            }
        }

        println!("\n=== Placeholder Command Status ===");
        println!(
            "Placeholder commands with help: {}/{}",
            documented_placeholders,
            placeholder_commands.len()
        );

        if !undocumented_placeholders.is_empty() {
            println!("\nPlaceholder commands without help:");
            for cmd in undocumented_placeholders {
                println!("  {}", cmd);
            }
        }
        println!("\n");
    }
}
