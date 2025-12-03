use crossterm::style::Color;
use termimad::{mad_print_inline, MadSkin};

use crate::helpfuncs::{
    find_command_help, find_node_by_path, find_topic, get_children, get_node_path, get_see_also, HelpNode,
    Topic, HELP_NODES, HELP_SYSTEM, TOPICS,
};

pub struct HelpDriver {
    skin: MadSkin,
}

impl HelpDriver {
    pub fn new() -> Self {
        let mut skin = MadSkin::default();

        // Customize the markdown rendering
        skin.bold.set_fg(Color::Yellow);
        skin.italic.set_fg(Color::Cyan);
        skin.code_block.set_bg(Color::AnsiValue(236)); // Dark gray
        skin.inline_code.set_bg(Color::AnsiValue(236));

        Self { skin }
    }

    /// Main entry point for help command
    /// Examples:
    ///   help           -> show root help
    ///   help reset     -> show reset command help
    ///   help set debug -> show set debug help
    ///   help attach -r -> show attach switches subsection
    pub fn show_help(&self, args: &[&str]) {
        if args.is_empty() {
            self.show_root();
            return;
        }

        // Try to parse as command with optional subsection
        let (command, subsection) = if args.len() >= 2 {
            (args[0], Some(args[1]))
        } else {
            (args[0], None)
        };

        // First, try command mapping
        if let Some(path) = find_command_help(command, subsection) {
            if let Some(node) = find_node_by_path(path) {
                self.show_node(node, path);
                return;
            }
        }

        // Try multi-word path (e.g., "set debug")
        let path_str = args.join(".");
        if let Some(node) = self.find_node_fuzzy(&path_str) {
            let node_path = get_node_path(node);
            self.show_node(node, &node_path);
            return;
        }

        // Try as topic
        if let Some(topic) = find_topic(args[0]) {
            self.show_topic(topic);
            return;
        }

        // Not found
        println!("No help found for '{}'", args.join(" "));
        println!("\nTry 'help' to see all available topics and commands.");
    }

    pub fn show_root(&self) {
        // Display root content
        self.skin.print_text(&HELP_SYSTEM.root_content);

        self.skin.print_text("\n## Available Commands\n");

        // Show top-level commands
        let top_level = get_children(HELP_SYSTEM.root_id);
        for node in top_level {
            self.skin.print_text(
                format!("  **{}** - {}\n", node.title, self.extract_summary(node.content)).as_str(),
            );
        }

        // Show topics if any
        if !TOPICS.is_empty() {
            self.skin.print_text("\n## Topics\n");
            for topic in TOPICS.iter() {
                self.skin
                    .print_text(format!("  **{}** - {}\n", topic.title, topic.description).as_str());
            }
        }
    }

    pub fn show_node(&self, node: &HelpNode, node_path: &str) {
        // Display title
        self.skin.print_text(format!("# {}\n", node.title).as_str());

        // Display content using termimad
        self.skin.print_text(node.content);

        // Show children if any
        let children = get_children(node.id);
        if !children.is_empty() {
            self.skin.print_inline("\n## Subtopics\n");
            for child in children {
                let child_summary = self.extract_summary(child.content);
                self.skin
                    .print_inline(format!("  * **{}** - {}\n", child.title, child_summary).as_str());
            }
        }

        // Show related topics (see also)
        if let Some(related) = get_see_also(node_path) {
            if !related.is_empty() {
                self.skin.print_inline("\n## See Also\n");
                for related_path in related {
                    if let Some(related_node) = find_node_by_path(related_path) {
                        mad_print_inline!(&self.skin, "  * {}", related_node.title);
                    }
                }
            }
        }
    }

    pub fn show_topic(&self, topic: &Topic) {
        self.skin
            .print_text(format!("# Topic: {}\n", topic.title).as_str());
        self.skin
            .print_inline(format!("{}\n", topic.description).as_str());

        mad_print_inline!(&self.skin, "## Related Commands\n");
        for node_path in topic.nodes {
            if let Some(node) = find_node_by_path(node_path) {
                self.skin.print_text(
                    format!("  * **{}** - {}", node.title, self.extract_summary(node.content)).as_str(),
                );
            }
        }
    }

    /// Extract the first sentence or line as a summary
    fn extract_summary(&self, content: &str) -> String {
        // Remove markdown formatting for summary
        let stripped = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter(|line| !line.starts_with("```"))
            .filter(|line| !line.starts_with('#'))
            .next()
            .unwrap_or("")
            .trim();

        // Remove markdown bold/italic markers
        let summary = stripped.replace("**", "").replace("*", "").replace("`", "");

        // Truncate if too long
        if summary.len() > 80 {
            format!("{}...", &summary[..77])
        } else {
            summary
        }
    }

    /// Fuzzy find a node by approximate path
    /// Handles cases like "set debug" -> "commands.set.set_debug"
    pub fn find_node_fuzzy(&self, query: &str) -> Option<&'static HelpNode> {
        let query_parts: Vec<&str> = query.split(|c: char| c == '.' || c.is_whitespace()).collect();

        // Try exact path first
        if let Some(node) = find_node_by_path(query) {
            return Some(node);
        }

        // Try with "commands" prefix
        let with_prefix = format!("commands.{}", query);
        if let Some(node) = find_node_by_path(&with_prefix) {
            return Some(node);
        }

        // Try matching node IDs that contain the query parts
        for node in HELP_NODES.iter() {
            let node_path = get_node_path(node);
            let node_parts: Vec<&str> = node_path.split('.').collect();

            // Check if all query parts are in the node path (in order)
            let mut query_idx = 0;
            for part in &node_parts {
                if query_idx < query_parts.len() && part.eq_ignore_ascii_case(query_parts[query_idx]) {
                    query_idx += 1;
                }
            }

            if query_idx == query_parts.len() {
                return Some(node);
            }
        }

        None
    }
}

impl Default for HelpDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_command_help() {
        assert!(find_command_help("RESET", None).is_some());
        assert!(find_command_help("reset", None).is_some());
        assert!(find_command_help("RE", None).is_some());
    }

    #[test]
    fn test_find_subsection() {
        assert!(find_command_help("ATTACH", Some("switches")).is_some());
        assert!(find_command_help("attach", Some("-r")).is_some());
    }

    #[test]
    fn test_find_topic() {
        assert!(find_topic("debugging").is_some());
        assert!(find_topic("DEBUGGING").is_some());
    }
}
