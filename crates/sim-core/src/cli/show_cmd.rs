// SPDX-License-Identifier: MIT

use nom::{bytes::complete::take_while1, character::complete::space0, sequence::preceded, Parser};

use crate::{
    cli::{
        cli_error::CLIError,
        cmd_repl::CmdContext,
        cmd_trie::{CommandTrie, TrieMatch},
        legacy::LEGACY_DEVICES,
        span::Span,
    },
    logging::LogType,
    sim_println,
};

/// SHOW sub-command action type
type ShowAction = fn(&mut CmdContext, Span) -> Result<(), CLIError>;

/// Show command sub-commands table
#[derive(Debug)]
pub struct ShowCommandTable {
    trie: CommandTrie<ShowAction>,
}

impl ShowCommandTable {
    /// Build the SHOW sub-command trie
    pub fn new() -> Self {
        let mut trie = CommandTrie::<ShowAction>::new();

        trie.insert("DEBUG", show_debug_command, false);
        trie.insert("DEVICES", show_devices_command, false);

        Self { trie }
    }

    /// Look up a SHOW sub-command action by keyword.
    ///
    /// Returns a `fn` pointer (`Copy`), releasing the borrow on `self` immediately.  This is required to
    /// satisfy the borrow checker in [`show_command`]: the trie lookup borrows `context.state.set_table`
    /// immutably; returning a `Copy` value drops that borrow before `action(context, remainder)` takes `&mut
    /// context`.
    fn find_trie_action<A>(&self, keyword: &str, span: Span, trie: &CommandTrie<A>) -> Result<A, CLIError>
    where
        A: Copy,
    {
        let keyword_upper = keyword.to_uppercase();
        match trie.find(&keyword_upper) {
            TrieMatch::Exact(action) => Ok(action),
            TrieMatch::Ambiguous(matches) => Err(CLIError::ambiguous_command(
                span,
                "SET keyword",
                matches.join(", "),
            )),
            TrieMatch::NotFound => Err(CLIError::unknown_command(
                span,
                "SET keyword lookup",
                keyword_upper,
            )),
        }
    }

    /// Look up a "SET" sub-command's action by its keyword
    pub fn find_action(&self, keyword: &str, span: Span) -> Result<ShowAction, CLIError> {
        self.find_trie_action(keyword, span, &self.trie)
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Main SHOW dispatcher
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

pub fn show_command(context: &mut CmdContext, args: Span<'_>) -> Result<(), CLIError> {
    // Next token is either a subcommand or a device/unit identifier (hence the expanded condition for
    // take_while1, as opposed to purely alphabetic):
    let (remainder, keyword_span) = preceded(
        space0::<Span<'_>, CLIError>,
        take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    )
    .parse(args)
    .map_err(|_| CLIError::expected_token(args, "SHOW command", args.input.to_string()))?;

    // "SHOW" or "SHOW <dev>"?
    let keyword = keyword_span.input;
    if context.state.get_device_meta(keyword).is_none() {
        if !LEGACY_DEVICES.contains(&keyword.to_uppercase().as_str()) {
            let action = context.state.show_table.find_action(keyword, keyword_span)?;
            return action(context, remainder);
        }

        // If it's a legacy device name, fall through to the device-specific path.
        // The device name is ignored anyway.
    }

    /*
      // Device-specific path... next token:
      let (remainder, sub_span) = preceded(
          space0::<Span<'_>, CLIError>,
          take_while1(|c: char| c.is_alphabetic()),
      )
      .parse(remainder)
      .map_err(|_| CLIError::expected_token(remainder, "SHOW <dev>", remainder.input.to_string()))?;

      let dev_name = keyword.to_uppercase();
      let action = context
          .state
          .set_table
          .find_device_action(sub_span.input, sub_span)?;
      action(dev_name.as_str(), context, remainder)
    */

    // Temporarily allow "SHOW <dev>" to succeed. Silently.
    Ok(())
}

fn show_debug_command(context: &mut CmdContext, _args: Span<'_>) -> Result<(), CLIError> {
    let repl_state = &context.state;
    if let Some(debug_sink) = repl_state.debug_log.as_ref() {
        let dest = match debug_sink.log_type.clone() {
            LogType::Stdout => "stdout".to_string(),
            LogType::Stderr => "stderr".to_string(),
            LogType::File(fname) => fname.clone(),
        };
        sim_println!(repl_state, "Debug output directed to {}", dest);
    } else {
        sim_println!(repl_state, "Debugging output is not directed anywhere.");
    }

    Ok(())
}

fn show_devices_command(context: &mut CmdContext, _args: Span<'_>) -> Result<(), CLIError> {
    let _repl_state = &context.state;

    Ok(())
}
