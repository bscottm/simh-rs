// SPDX-License-Identifier: MIT

//! SET command implementation with sub-command trie
//!
//! The SET command has its own trie of keywords (DEBUG, THROTTLE, etc.)
//! that can be prefix-matched just like top-level commands.
//!
//! # Logging command design
//!
//! `SET DEBUG` and `SET LOG` are symmetric but have different concerns:
//!
//! - `SET DEBUG [-P] [-I] [-N] <stdout|stderr|LOG|file>` — opens a debug session.
//!   `-P` includes the CPU program counter; `-I` uses instruction count as the
//!   timestamp prefix instead of wall-clock microseconds; `-N` truncates an existing
//!   file (default: append). `LOG` shares the active transcript sink.
//!
//! - `SET LOG [-A] [-N] <stdout|stderr|DEBUG|file>` — opens a transcript session.
//!   `-A` appends (default: truncate). `DEBUG` shares the active debug sink.
//!
//! In both shared-sink cases the `Arc` is cloned — no new file is opened. The
//! `RwLock`/`Mutex` inside [`crate::logging::SharedSink`] serialises writes from both sessions.
//!
//! When `SET DEBUG LOG` is active, [`crate::logging::DebugState`] holds a reference to the
//! [`crate::logging::SharedTranscriptSink`] and calls
//! [`crate::logging::TranscriptSink::write_debug_with_flush`] before each debug record, flushing any partial
//! transcript line first.

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while1},
    character::complete::space0,
    combinator::{cut, map, opt},
    multi::separated_list1,
    sequence::{preceded, terminated},
    Parser,
};

use crate::{
    cli::{
        cli_error::CLIError,
        cmd_repl::CmdContext,
        cmd_trie::{CommandTrie, TrieMatch},
        legacy::LEGACY_DEVICES,
        parsers::{consume_eol, filename_noglob, parse_switches},
        span::Span,
    },
    env::SimRequest,
    logging::{
        debug_registry, new_debug_snapshot, open_sink, stderr_sink, stdout_sink, DebugFormatFlags,
        LogDestination, LogError, LogType,
    },
};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// SetCommandTable
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// SET sub-command action type
type SetAction = fn(&mut CmdContext, Span) -> Result<(), CLIError>;
/// SET `<device>` sub-command action type
type DeviceAction = fn(&str, &mut CmdContext, Span) -> Result<(), CLIError>;

/// SET command sub-commands table
#[derive(Debug)]
pub struct SetCommandTable {
    trie: CommandTrie<SetAction>,
    device_trie: CommandTrie<DeviceAction>,
}

impl SetCommandTable {
    /// Build the SET sub-command trie
    pub fn new() -> Self {
        let mut trie = CommandTrie::<SetAction>::new();

        trie.insert("DEBUG", set_debug_command, false);
        trie.insert("NODEBUG", set_nodebug_command, false);
        trie.insert("THROTTLE", set_throttle_placeholder, false);
        trie.insert("NOTHROTTLE", set_nothrottle_placeholder, false);
        trie.insert("CONSOLE", set_console_placeholder, false);
        trie.insert("REMOTE", set_remote_placeholder, false);
        trie.insert("LOG", set_log_command, false);
        trie.insert("NOLOG", set_nolog_command, false);
        trie.insert("TELNET", set_telnet_placeholder, false);
        trie.insert("NOTELNET", set_notelnet_placeholder, false);
        trie.insert("SERIAL", set_serial_placeholder, false);
        trie.insert("NOSERIAL", set_noserial_placeholder, false);

        let mut device_trie = CommandTrie::<DeviceAction>::new();

        device_trie.insert("DEBUG", set_device_debug_command, false);
        device_trie.insert("NODEBUG", set_device_nodebug_command, false);
        device_trie.insert("OCT", set_device_octal_command, false);
        device_trie.insert("DEC", set_device_decimal_command, false);
        device_trie.insert("HEX", set_device_hex_command, false);
        device_trie.insert("BIN", set_device_binary_command, false);
        device_trie.insert("ENABLED", set_device_enabled_command, false);
        device_trie.insert("DISABLED", set_device_disabled_command, false);

        SetCommandTable { trie, device_trie }
    }

    /// Look up a SET sub-command action by keyword.
    ///
    /// Returns a `fn` pointer (`Copy`), releasing the borrow on `self` immediately.
    /// This is required to satisfy the borrow checker in [`set_command`]: the trie
    /// lookup borrows `context.state.set_table` immutably; returning a `Copy` value
    /// drops that borrow before `action(context, remainder)` takes `&mut context`.
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
    pub fn find_action(&self, keyword: &str, span: Span) -> Result<SetAction, CLIError> {
        self.find_trie_action(keyword, span, &self.trie)
    }

    /// Look up a "SET `<device>`" sub-command's action by its keyword
    pub fn find_device_action(&self, keyword: &str, span: Span) -> Result<DeviceAction, CLIError> {
        self.find_trie_action(keyword, span, &self.device_trie)
    }

    /// Enumerate all commands (for testing)
    pub fn all_keywords(&self) -> Vec<(&'static str, bool)> {
        self.trie.all_commands()
    }
}

impl Default for SetCommandTable {
    fn default() -> Self {
        Self::new()
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Main SET dispatcher
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Main SET command handler — parses keyword and dispatches via trie.
pub fn set_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    // Next token is either a subcommand or a device/unit identifier (hence the expanded condition for
    // take_while1, as opposed to purely alphabetic):
    let (remainder, keyword_span) = preceded(
        space0::<Span<'_>, CLIError>,
        take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
    )
    .parse(args)
    .map_err(|_| CLIError::expected_token(args, "SET command", args.input.to_string()))?;

    // "SET" or "SET <dev>"?
    let keyword = keyword_span.input;
    if context.state.get_device_meta(keyword).is_none() {
        if !LEGACY_DEVICES.contains(&keyword.to_uppercase().as_str()) {
            let action = context.state.set_table.find_action(keyword, keyword_span)?;
            return action(context, remainder);
        }

        // If it's a legacy device name, fall through to the device-specific path.
        // The device name is ignored anyway.
    }

    // Device-specific path... next token:
    let (remainder, sub_span) = preceded(
        space0::<Span<'_>, CLIError>,
        take_while1(|c: char| c.is_alphabetic()),
    )
    .parse(remainder)
    .map_err(|_| CLIError::expected_token(remainder, "SET <dev>", remainder.input.to_string()))?;

    let dev_name = keyword.to_uppercase();
    let action = context
        .state
        .set_table
        .find_device_action(sub_span.input, sub_span)?;
    action(dev_name.as_str(), context, remainder)
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// SET DEBUG
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// `SET DEBUG` parsing.
///
/// Parses two distinct "SET DEBUG" syntaxes:
///
/// - Set debug output: `SET DEBUG [-P] [-I] [-N] <stdout | stderr | LOG | filename>`
///
///   Opens a debug log session and sends [`SimRequest::SetDebugState`] to the simulator
///   so that `sim_debug!` calls in device code use the same session.
///
/// - Set debug flag/capability: `SET DEBUG=<capability{,capability}*>`
///
///   Turns on (enables) a debugging capability.
///
/// # Flags
/// | Flag | Effect                                                               |
/// |------|----------------------------------------------------------------------|
/// | `-P` | Include CPU program counter (`PC=017652`) in each record             |
/// | `-I` | Use instruction count `#N` as prefix instead of wall-clock timestamp |
/// | `-N` | Truncate an existing file (default: append)                          |
///
/// # Destinations
/// | Value    | Behaviour                                                                 |
/// |----------|---------------------------------------------------------------------------|
/// | `stdout` | Write to standard output                                                  |
/// | `stderr` | Write to standard error                                                   |
/// | `LOG`    | Share the active transcript sink (`Arc::clone`); requires `SET LOG` first |
/// | `<file>` | Open the named file                                                       |
fn set_debug_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    #[derive(Debug)]
    enum DebugDest {
        Stdout,
        Stderr,
        ShareLog,
        Filename(String),
    }

    // Does this look like the start of a capability list?
    let (args, _) = space0.parse(args)?;
    if let Ok((remainder, _)) = preceded(space0, tag::<&str, Span<'_>, CLIError>("=")).parse(args) {
        return set_debug_categories(context, remainder);
    }

    // Nope. It's setting the output sink.
    let (_, (switches, dest)) = (
        opt(parse_switches("pin")),
        preceded(
            space0,
            terminated(
                alt((
                    map(tag_no_case("stdout"), |_| DebugDest::Stdout),
                    map(tag_no_case("stderr"), |_| DebugDest::Stderr),
                    map(tag_no_case("log"), |_| DebugDest::ShareLog),
                    map(filename_noglob, DebugDest::Filename),
                )),
                cut(consume_eol()),
            ),
        ),
    )
        .parse(args)?;

    let flags = DebugFormatFlags {
        show_pc: switches.as_ref().is_some_and(|f| f.contains(&'p')),
        show_instruction_count: switches.as_ref().is_some_and(|f| f.contains(&'i')),
    };
    let truncate = switches.as_ref().is_some_and(|f| f.contains(&'n'));

    // Build sink — or clone the transcript sink for shared-file mode.
    let (sink, log_type, transcript_ref) = match dest {
        DebugDest::Stdout => (stdout_sink(), LogType::Stdout, None),
        DebugDest::Stderr => (stderr_sink(), LogType::Stderr, None),

        DebugDest::ShareLog => {
            // SET DEBUG LOG — reuse the transcript's sink and hold a reference to
            // the transcript so write_sim can call write_debug_with_flush.
            let ts = context.state.transcript_sink.clone().ok_or_else(|| {
                CLIError::generic_message(
                    "SET DEBUG LOG requires an active transcript log (use SET LOG <file> first)".into(),
                )
            })?;
            // Clone the transcript's underlying sink so both sessions share one file.
            let sink = {
                let inner = ts.inner.lock().unwrap();
                std::sync::Arc::clone(&inner.writer)
            };
            (sink, LogType::File("<shared with LOG>".to_string()), Some(ts))
        }

        DebugDest::Filename(ref fname) => {
            let sink = open_sink(fname, !truncate)?;
            (sink, LogType::File(fname.clone()), None)
        }
    };

    // Create a CPU snapshot only when it will actually be used.
    let snapshot = if flags.show_pc || flags.show_instruction_count {
        Some(new_debug_snapshot())
    } else {
        None
    };

    let (log_dest, debug_state) =
        LogDestination::new_debug(sink, transcript_ref, flags, snapshot.clone(), log_type)?;

    context.state.debug_log = Some(log_dest);
    context.state.debug_state = Some(std::sync::Arc::clone(&debug_state));

    // Send both the state and the snapshot to the simulator.
    // The execution loop writes to the snapshot each cycle; sim_debug! reads it.
    context.state.transact(
        args,
        "SET DEBUG",
        SimRequest::SetDebugState(debug_state, snapshot),
    )
}

/// Parse "SET DEBUG=capability{,capability}*", enabling debug categories
fn set_debug_categories(_context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    let (args, _) = space0.parse(args)?;
    let (_, categories) = separated_list1(
        (space0::<Span<'_>, CLIError>, tag(","), space0),
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
    )
    .parse(args)
    .map_err(|_| CLIError::expected_token(args, "SET DEBUG=", "category name".to_string()))?;

    let mut unknown = Vec::new();
    for category_span in categories {
        let name = category_span.input;
        if let Err(_) = debug_registry().enable_by_name(name) {
            unknown.push(name.to_string());
        }
    }

    if !unknown.is_empty() {
        return Err(CLIError::generic_message(format!(
            "Unknown debug categories: {}",
            unknown.join(", ")
        )));
    }

    Ok(())
}
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// SET NODEBUG
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// `SET NODEBUG`
///
/// Closes the debug log. Dropping [`LogDestination`] flushes and shuts down the
/// flexi_logger session. Clears `debug_state` on both the CLI and simulator sides.
fn set_nodebug_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    consume_eol().parse(args)?;

    // Drop LogDestination first — flushes flexi_logger before clearing the state Arc.
    context.state.debug_log = None;
    context.state.debug_state = None;

    context
        .state
        .transact(args, "SET NODEBUG", SimRequest::DebugDisable)
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// SET LOG
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// `SET LOG [-A] [-N] <stdout | stderr | DEBUG | filename>`
///
/// Opens a transcript log session. Console output (character-at-a-time from the
/// simulator) is accumulated in a line buffer and flushed atomically on newline.
///
/// # Flags
/// | Flag | Effect |
/// |------|--------|
/// | `-A` | Append to an existing file (default: truncate) |
/// | `-N` | Truncate explicitly (overrides `-A` if both given) |
///
/// # Destinations
/// | Value | Behaviour |
/// |-------|-----------|
/// | `stdout` | Write to standard output |
/// | `stderr` | Write to standard error |
/// | `DEBUG` | Share the active debug sink (`Arc::clone`); requires `SET DEBUG` first |
/// | `<file>` | Open the named file |
fn set_log_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    #[derive(Debug)]
    enum LogDest {
        Stdout,
        Stderr,
        ShareDebug,
        Filename(String),
    }

    let (_, (switches, dest)) = (
        opt(parse_switches("an")),
        preceded(
            space0,
            terminated(
                alt((
                    map(tag_no_case("stdout"), |_| LogDest::Stdout),
                    map(tag_no_case("stderr"), |_| LogDest::Stderr),
                    map(tag_no_case("debug"), |_| LogDest::ShareDebug),
                    map(filename_noglob, LogDest::Filename),
                )),
                cut(consume_eol()),
            ),
        ),
    )
        .parse(args)?;

    let append = switches.as_ref().is_some_and(|f| f.contains(&'a'));
    let truncate = switches.as_ref().is_some_and(|f| f.contains(&'n'));
    // -N overrides -A
    let append = append && !truncate;

    let (sink, log_type) = match dest {
        LogDest::Stdout => (stdout_sink(), LogType::Stdout),
        LogDest::Stderr => (stderr_sink(), LogType::Stderr),

        LogDest::ShareDebug => {
            // SET LOG DEBUG — clone the debug session's sink directly.
            // No partial-line flush needed here: debug records are always line-complete,
            // so the transcript line buffer just writes completed lines to the shared file.
            let debug_sink = context
                .state
                .debug_state
                .as_ref()
                .ok_or_else(|| {
                    CLIError::generic_message(
                        "SET LOG DEBUG requires an active debug log (use SET DEBUG <file> first)".into(),
                    )
                })
                .map(|s| std::sync::Arc::clone(&s.lock().unwrap().sink))?;

            (debug_sink, LogType::File("<shared with DEBUG>".to_string()))
        }

        LogDest::Filename(ref fname) => (open_sink(fname, append)?, LogType::File(fname.clone())),
    };

    let (log_dest, transcript_sink) = LogDestination::new_transcript(sink, log_type)?;

    context.state.transcript_log = Some(log_dest);
    context.state.transcript_sink = Some(transcript_sink);

    Ok(())
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// SET NOLOG
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// `SET NOLOG` — close the transcript log.
fn set_nolog_command(context: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    consume_eol().parse(args)?;
    context.state.transcript_log = None;
    context.state.transcript_sink = None;
    Ok(())
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Placeholder sub-commands
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

fn set_throttle_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET THROTTLE"))
}
fn set_nothrottle_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET NOTHROTTLE"))
}
fn set_console_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET CONSOLE"))
}
fn set_remote_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET REMOTE"))
}
fn set_telnet_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET TELNET"))
}
fn set_notelnet_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET NOTELNET"))
}
fn set_serial_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET SERIAL"))
}
fn set_noserial_placeholder(_: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET NOSERIAL"))
}
fn set_device_debug_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> DEBUG"))
}
fn set_device_nodebug_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> NODEBUG"))
}
fn set_device_octal_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> OCT"))
}
fn set_device_decimal_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> DEC"))
}
fn set_device_hex_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> HEX"))
}
fn set_device_binary_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> BIN"))
}
fn set_device_enabled_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> ENABLED"))
}
fn set_device_disabled_command(_: &str, _: &mut CmdContext, args: Span) -> Result<(), CLIError> {
    Err(CLIError::unimplemented_command(args, "SET <device> DISABLED"))
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Error type conversions
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

impl From<LogError> for CLIError {
    fn from(e: LogError) -> Self {
        CLIError::generic_message(e.to_string())
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Tests
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_table_creation() {
        let table = SetCommandTable::new();
        let keywords = table.all_keywords();
        assert!(keywords.iter().any(|(name, _)| *name == "DEBUG"));
        assert!(keywords.iter().any(|(name, _)| *name == "NODEBUG"));
        assert!(keywords.iter().any(|(name, _)| *name == "LOG"));
        assert!(keywords.iter().any(|(name, _)| *name == "NOLOG"));
    }

    #[test]
    fn test_set_keyword_matching() {
        let table = SetCommandTable::new();
        match table.trie.find("DEBUG") {
            TrieMatch::Exact(_) => {}
            _ => panic!("DEBUG exact"),
        }
        match table.trie.find("DEB") {
            TrieMatch::Exact(_) => {}
            _ => panic!("DEB prefix"),
        }
        match table.trie.find("INVALID") {
            TrieMatch::NotFound => {}
            _ => panic!("INVALID"),
        }
    }
}
