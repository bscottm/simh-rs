// SPDX-License-Identifier: MIT

//! Generic command trie for prefix matching and ambiguity detection
//!
//! This module provides a reusable trie data structure for command dispatch
//! with support for:
//! - Prefix matching (e.g., "EX" matches "EXAMINE")
//! - Exact-only aliases (e.g., "E" must match exactly, not "EXAMINE")
//! - Ambiguity detection (e.g., "S" matches both "STEP" and "SET")
//! - Efficient lookup using FxHasher

use rustc_hash::FxHasher;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

/// A node in the command trie
#[derive(Debug)]
struct TrieNode<A> {
    /// Child nodes indexed by character
    children: HashMap<char, TrieNode<A>, BuildHasherDefault<FxHasher>>,

    /// Action to execute if this is a valid command endpoint
    action: Option<A>,

    /// If true, this command only matches exact input (no prefix matching)
    exact_only: bool,

    /// The full command name (always present for endpoints)
    command_name: &'static str,
}

impl<A> TrieNode<A> {
    fn new_intermediate() -> Self {
        TrieNode {
            children: HashMap::default(),
            action: None,
            exact_only: false,
            command_name: "",
        }
    }
}

/// Generic command trie for efficient prefix matching
///
/// Type parameter `A` is the action type (typically a function pointer).
///
/// # Examples
///
/// ```ignore
/// type CmdAction = fn(&mut Context, Args) -> Result<(), Error>;
///
/// let mut trie = CommandTrie::<CmdAction>::new();
/// trie.insert("EXAMINE", examine_command, false);
/// trie.insert("E", examine_command, true);  // Exact alias
///
/// match trie.find("EX") {
///     TrieMatch::Exact(action) => action(ctx, args),
///     TrieMatch::Ambiguous(names) => eprintln!("Ambiguous: {}", names.join(", ")),
///     TrieMatch::NotFound => eprintln!("Unknown command"),
/// }
/// ```
#[derive(Debug)]
pub struct CommandTrie<A> {
    root: TrieNode<A>,
}

impl<A: Copy> CommandTrie<A> {
    /// Create a new empty command trie
    pub fn new() -> Self {
        CommandTrie {
            root: TrieNode::new_intermediate(),
        }
    }

    /// Insert a command into the trie
    ///
    /// # Arguments
    /// * `command` - The command string (e.g., "EXAMINE", "SET", "E")
    /// * `action` - The action to execute for this command
    /// * `exact_only` - If true, only exact matches are allowed (no prefix matching)
    ///
    /// # Example
    /// ```ignore
    /// trie.insert("EXAMINE", examine_fn, false);  // Prefix matching allowed
    /// trie.insert("E", examine_fn, true);          // Must match exactly
    /// ```
    pub fn insert(&mut self, command: &'static str, action: A, exact_only: bool) {
        let mut node = &mut self.root;

        for ch in command.chars() {
            node = node
                .children
                .entry(ch)
                .or_insert_with(|| TrieNode::new_intermediate());
        }

        node.action = Some(action);
        node.exact_only = exact_only;
        node.command_name = command;
    }

    /// Find a command in the trie by input string
    ///
    /// Returns:
    /// - `TrieMatch::Exact(action)` if exactly one command matches
    /// - `TrieMatch::Ambiguous(names)` if multiple commands match
    /// - `TrieMatch::NotFound` if no commands match
    ///
    /// # Matching Rules
    /// 1. Traverse the trie following the input characters
    /// 2. If the input ends at a node with `exact_only` set, only match if lengths are equal
    /// 3. If the input ends at a node with an action, that's a candidate match
    /// 4. Otherwise, collect all commands that start with this prefix
    /// 5. If multiple matches exist, return ambiguous
    pub fn find(&self, input: &str) -> TrieMatch<A> {
        let mut node = &self.root;

        // Traverse the trie following the input
        for ch in input.chars() {
            match node.children.get(&ch) {
                Some(next_node) => node = next_node,
                None => return TrieMatch::NotFound,
            }
        }

        // Check for exact match at this node
        if let Some(action) = node.action {
            if node.exact_only {
                // Exact-only: must match full command name
                if input.len() == node.command_name.len() {
                    return TrieMatch::Exact(action);
                }
                // Fall through to check for other matches
            } else {
                // Non-exact: input can be prefix or full match
                return TrieMatch::Exact(action);
            }
        }

        // Look for commands that start with this prefix
        let mut matches = Vec::new();
        self.collect_matches(node, &mut matches);

        match matches.len() {
            0 => TrieMatch::NotFound,
            1 => TrieMatch::Exact(matches[0].0),
            _ => {
                let names: Vec<&'static str> = matches.iter().map(|(_, name)| *name).collect();
                TrieMatch::Ambiguous(names)
            }
        }
    }

    /// Recursively collect all valid commands under a node
    fn collect_matches(&self, node: &TrieNode<A>, matches: &mut Vec<(A, &'static str)>) {
        if let Some(action) = node.action {
            if !node.command_name.is_empty() {
                matches.push((action, node.command_name));
            }
        }

        for child in node.children.values() {
            self.collect_matches(child, matches);
        }
    }

    /// Get all commands registered in the trie
    ///
    /// Returns a vector of (command_name, exact_only) tuples, sorted by name.
    pub fn all_commands(&self) -> Vec<(&'static str, bool)> {
        let mut commands = Vec::new();
        self.collect_all_commands(&self.root, &mut commands);
        commands.sort_by(|a, b| a.0.cmp(b.0));
        commands
    }

    fn collect_all_commands(&self, node: &TrieNode<A>, commands: &mut Vec<(&'static str, bool)>) {
        if node.action.is_some() && !node.command_name.is_empty() {
            commands.push((node.command_name, node.exact_only));
        }

        for child in node.children.values() {
            self.collect_all_commands(child, commands);
        }
    }

    /// Get the number of commands in the trie
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.all_commands().len()
    }

    /// Check if the trie is empty
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<A: Copy> Default for CommandTrie<A> {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a trie lookup
#[derive(Debug, PartialEq)]
pub enum TrieMatch<A> {
    /// Found exactly one matching command
    Exact(A),

    /// Found multiple matching commands (ambiguous input)
    Ambiguous(Vec<&'static str>),

    /// No matching command found
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestAction = fn() -> i32;

    fn action1() -> i32 {
        1
    }
    fn action2() -> i32 {
        2
    }

    #[test]
    fn test_exact_match() {
        let mut trie = CommandTrie::new();
        trie.insert("EXAMINE", action1, false);

        match trie.find("EXAMINE") {
            TrieMatch::Exact(action) => assert_eq!(action(), 1),
            _ => panic!("Should match exactly"),
        }
    }

    #[test]
    fn test_prefix_match() {
        let mut trie = CommandTrie::new();
        trie.insert("EXAMINE", action1, false);

        match trie.find("EX") {
            TrieMatch::Exact(action) => assert_eq!(action(), 1),
            _ => panic!("Should prefix match"),
        }
    }

    #[test]
    fn test_exact_only_alias() {
        let mut trie = CommandTrie::<TestAction>::new();
        trie.insert("EXAMINE", action1, false);
        trie.insert("E", action1, true);

        // "E" should match exactly
        match trie.find("E") {
            TrieMatch::Exact(action) => assert_eq!(action(), 1),
            _ => panic!("E should match exactly"),
        }

        // "EX" should still match EXAMINE
        match trie.find("EX") {
            TrieMatch::Exact(action) => assert_eq!(action(), 1),
            _ => panic!("EX should match EXAMINE"),
        }
    }

    #[test]
    fn test_ambiguous_match() {
        let mut trie = CommandTrie::<TestAction>::new();
        trie.insert("STEP", action1, false);
        trie.insert("SET", action2, false);

        match trie.find("S") {
            TrieMatch::Ambiguous(names) => {
                assert_eq!(names.len(), 2);
                assert!(names.contains(&"STEP"));
                assert!(names.contains(&"SET"));
            }
            _ => panic!("S should be ambiguous"),
        }
    }

    #[test]
    fn test_not_found() {
        let mut trie = CommandTrie::<TestAction>::new();
        trie.insert("EXAMINE", action1, false);

        match trie.find("INVALID") {
            TrieMatch::NotFound => {}
            _ => panic!("Should not be found"),
        }
    }

    #[test]
    fn test_all_commands() {
        let mut trie = CommandTrie::<TestAction>::new();
        trie.insert("EXAMINE", action1, false);
        trie.insert("E", action1, true);
        trie.insert("RESET", action2, false);

        let commands = trie.all_commands();
        assert_eq!(commands.len(), 3);

        // Should be sorted
        assert_eq!(commands[0].0, "E");
        assert_eq!(commands[1].0, "EXAMINE");
        assert_eq!(commands[2].0, "RESET");

        // Check exact_only flags
        assert_eq!(commands[0].1, true); // E is exact-only
        assert_eq!(commands[1].1, false); // EXAMINE is not
    }

    #[test]
    fn test_empty_trie() {
        let trie: CommandTrie<TestAction> = CommandTrie::<TestAction>::new();
        assert!(trie.is_empty());
        assert_eq!(trie.len(), 0);
    }

    #[test]
    fn test_case_sensitivity() {
        let mut trie = CommandTrie::<TestAction>::new();
        trie.insert("EXAMINE", action1, false);

        // Trie is case-sensitive (caller should uppercase)
        match trie.find("examine") {
            TrieMatch::NotFound => {}
            _ => panic!("Should not match lowercase"),
        }

        match trie.find("EXAMINE") {
            TrieMatch::Exact(_) => {}
            _ => panic!("Should match uppercase"),
        }
    }
}
