#[derive(Debug, Clone)]
pub struct HelpNode {
    pub id: &'static str,
    pub parent: Option<&'static str>,
    pub title: &'static str,
    pub content: &'static str,
    pub children: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct Topic {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub nodes: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct CommandMapping {
    pub command: &'static str,
    pub help_path: &'static str,
}

#[derive(Debug, Clone)]
pub struct CommandSubsection {
    pub command: &'static str,
    pub subsection: &'static str,
    pub help_path: &'static str,
}

pub struct HelpSystem {
    pub root_id: &'static str,
    pub root_title: &'static str,
    pub root_content: &'static str,
}

// Include the generated help data
include!(concat!(env!("OUT_DIR"), "/help_data.rs"));

pub fn find_node_by_id(id: &str) -> Option<&'static HelpNode> {
    HELP_NODES.iter().find(|n| n.id == id)
}

pub fn find_node_by_path(path: &str) -> Option<&'static HelpNode> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let start_idx = if parts[0] == HELP_SYSTEM.root_id { 1 } else { 0 };
    if start_idx >= parts.len() {
        return None;
    }

    let mut current_parent = HELP_SYSTEM.root_id;
    for &part in &parts[start_idx..] {
        match HELP_NODES
            .iter()
            .find(|n| n.id == part && n.parent == Some(current_parent))
        {
            Some(node) => current_parent = node.id,
            None => return None,
        }
    }

    find_node_by_id(current_parent)
}

pub fn find_command_help(command: &str, subsection: Option<&str>) -> Option<&'static str> {
    let cmd_upper = command.to_uppercase();

    if let Some(sub) = subsection {
        if let Some(mapping) = COMMAND_SUBSECTIONS
            .iter()
            .find(|m| m.command == cmd_upper && m.subsection.eq_ignore_ascii_case(sub))
        {
            return Some(mapping.help_path);
        }
    }

    COMMAND_MAP
        .iter()
        .find(|m| m.command == cmd_upper)
        .map(|m| m.help_path)
}

pub fn get_children(node_id: &str) -> Vec<&'static HelpNode> {
    HELP_NODES.iter().filter(|n| n.parent == Some(node_id)).collect()
}

pub fn get_see_also(node_path: &str) -> Option<&'static [&'static str]> {
    SEE_ALSO
        .iter()
        .find(|(path, _)| *path == node_path)
        .map(|(_, related)| *related)
}

pub fn find_topic(name: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

pub fn get_node_path(node: &HelpNode) -> String {
    let mut parts = vec![node.id];
    let mut current = node;

    while let Some(parent_id) = current.parent {
        if parent_id == HELP_SYSTEM.root_id {
            break;
        }
        if let Some(parent) = find_node_by_id(parent_id) {
            parts.insert(0, parent.id);
            current = parent;
        } else {
            break;
        }
    }

    parts.insert(0, HELP_SYSTEM.root_id);
    parts.join(".")
}
