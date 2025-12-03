use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use reedline::{DefaultPrompt, Reedline, Signal};
use std::io::{stdout, Write};

use sim_help::{find_topic, get_children, get_node_path, HelpDriver, HelpNode, Topic};

/// A Page represents a location in the help “trie.”
#[derive(Debug, Clone)]
enum Page {
    /// “Root” page for which we show HELP_SYSTEM.root_content and list top‐level commands & topics.
    Root,
    /// A command help page.
    Node(&'static HelpNode),
    /// A topic help page.
    Topic(&'static Topic),
}

fn main() -> std::io::Result<()> {
    // Create our help browser “driver”.
    let help_driver = HelpDriver::new();

    // Our navigation stack: the first element is always the root.
    let mut stack: Vec<Page> = vec![Page::Root];
    // current_page holds the current page we are viewing.
    let mut current_page = Page::Root;

    // Set up Reedline editor for interactive prompt.
    let mut line_editor = Reedline::create();
    let prompt = DefaultPrompt::new(
        reedline::DefaultPromptSegment::Basic("help> ".to_string()),
        reedline::DefaultPromptSegment::Empty,
    );

    // Main REPL loop.
    loop {
        // Clear screen
        execute!(stdout(), Clear(ClearType::All))?;
        // Render the current help page.
        match current_page {
            Page::Root => {
                help_driver.show_root();
            }
            Page::Node(node) => {
                let path = get_node_path(node);
                help_driver.show_node(node, &path);
            }
            Page::Topic(topic) => {
                help_driver.show_topic(topic);
            }
        }
        // Print navigation help at the bottom.
        println!("\nCommands:");
        println!("  [number]     Open child help page (if available)");
        println!("  back         Go back to previous page");
        println!("  root         Jump to root page");
        println!("  search QUERY Find help page using fuzzy search (e.g., \"search set debug\")");
        println!("  topic NAME   Jump to a help topic (e.g., \"topic debugging\")");
        println!("  exit         Quit the help browser");

        // Read a command from the user.
        let sig = line_editor.read_line(&prompt);
        let line = match sig {
            Ok(Signal::Success(s)) => s,
            Ok(Signal::CtrlC) => {
                eprintln!("Input aborted by Ctrl-C.");
                continue;
            }
            Ok(Signal::CtrlD) => {
                eprintln!("REPL terminated via Ctrl-D.");
                break;
            }
            Err(err) => {
                eprintln!("Error reading line: {err}");
                continue;
            }
        };

        let line = line.trim();

        // Process the input command.
        if line.eq_ignore_ascii_case("exit") {
            break;
        } else if line.eq_ignore_ascii_case("back") {
            // Do nothing if already at root.
            if stack.len() > 1 {
                stack.pop();
                current_page = stack.last().unwrap().clone();
            }
        } else if line.eq_ignore_ascii_case("root") {
            stack.clear();
            stack.push(Page::Root);
            current_page = Page::Root;
        } else if line.to_lowercase().starts_with("search ") {
            let query = line[7..].trim();
            if let Some(node) = help_driver.find_node_fuzzy(query) {
                // Push new page and update current_page.
                current_page = Page::Node(node);
                stack.push(Page::Node(node));
            } else {
                println!("No help page found for query: \"{}\"", query);
                pause();
            }
        } else if line.to_lowercase().starts_with("topic ") {
            let topic_name = line[6..].trim();
            if let Some(topic) = find_topic(topic_name) {
                current_page = Page::Topic(topic);
                stack.push(Page::Topic(topic));
            } else {
                println!("No help topic found for: \"{}\"", topic_name);
                pause();
            }
        } else if let Ok(idx) = line.parse::<usize>() {
            // If the user typed a number, try to drill down into the corresponding child.
            match current_page {
                Page::Node(node) => {
                    // Get all children
                    let children = get_children(node.id);
                    if idx == 0 || idx > children.len() {
                        println!("Invalid index: {}. Valid range is 1..{}", idx, children.len());
                        pause();
                        continue;
                    }
                    let child = children[idx - 1];
                    current_page = Page::Node(child);
                    stack.push(Page::Node(child));
                }
                Page::Root => {
                    // At root we list top-level commands (children of HELP_SYSTEM.root_id)
                    let children = get_children("commands");
                    if idx == 0 || idx > children.len() {
                        println!("Invalid index: {}. Valid range is 1..{}", idx, children.len());
                        pause();
                        continue;
                    }
                    let child = children[idx - 1];
                    current_page = Page::Node(child);
                    stack.push(Page::Node(child));
                }
                Page::Topic(_) => {
                    println!("Number selection is not supported on topic pages.");
                    pause();
                }
            }
        } else {
            println!("Unknown command.");
            pause();
        }
    }

    Ok(())
}

/// Pause the screen waiting for the user to press Enter.
fn pause() {
    print!("\nPress ENTER to continue...");
    stdout().flush().unwrap();
    // Just block on a new line.
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).unwrap();
}
