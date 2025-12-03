// SPDX-License-Identifier: MIT

//! Command line Read-Eval-Print Loop (REPL)

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{stdout, Write as IOWrite};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};

use crate::cli::{
    cli_error::CLIError,
    cmd_reader::CmdReader,
    cmd_table::CommandTable,
    parsers::{command_line, InputRadix},
    repl_state::{InterpState, REPLState},
    span::Span,
};

use crate::env::{
    CPUTraits, DeviceTraits, ResourceCLIMetadata, SimEnvironment, SimError, SimRequest, SimResponse,
};

// Include the generated version data
include!(concat!(env!("OUT_DIR"), "/version.rs"));

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// Structs, enums:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Command interpreter container
pub struct CmdREPL {
    /// The top-level command table.
    commands: CommandTable,
    /// The associated simulator's name
    simulator_name: String,
    /// stdin and logging prompt string.
    prompt: String,
    /// Input stack
    inputs: CmdReader,
    /// REPL state shared with command action functions via [`CmdContext`]
    state: REPLState,
}

/// Token types that can be returned by [`CmdREPL::next_command`]
#[derive(Debug, PartialEq)]
pub enum CmdToken {
    /// Shell command (everything after "!")
    ShellCommand(String),
    /// Regular command verb with the rest of the line executed successfully.
    ExecutedCommand,
    /// End of input
    Eof,
}

/// Additional context passed to the command action functions
///
/// # Notes - While this struct only has a single member, keeping this as a struct reduces the amount of
/// refactoring required if more members are added.
pub struct CmdContext<'a> {
    pub state: &'a mut REPLState,
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~
// CmdREPL implementation:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

impl CmdREPL {
    /// Create a new REPL
    pub fn new(simulator_name: &str, manifest_vec: Vec<ResourceCLIMetadata>) -> Self {
        // Output defaults to stdout.
        CmdREPL::new_with_sink(simulator_name, manifest_vec, None)
    }

    /// Create a new REPL with an output sink
    ///
    /// Creates a new `CmdREPL` instance with a specific output sink if `output_sink` is `Some(...)`,
    /// defaulting to `stdout` if `output_sink` is `None`.
    pub fn new_with_sink(
        simulator_name: &str,
        manifest_vec: Vec<ResourceCLIMetadata>,
        output_sink: Option<Rc<RefCell<dyn IOWrite>>>,
    ) -> Self {
        let mut manifest = HashMap::default();

        for dev in manifest_vec {
            manifest.insert(dev.name.clone(), dev);
        }

        let output_sink: Rc<RefCell<dyn IOWrite>> =
            output_sink.unwrap_or_else(|| Rc::new(RefCell::new(stdout())));

        Self {
            commands: CommandTable::new(),
            simulator_name: simulator_name.to_string(),
            prompt: "sim> ".to_string(),
            inputs: CmdReader::new(),
            state: REPLState::new(manifest, output_sink.clone()),
        }
    }

    /// Connect the REPL state's message endpoints to the simulator
    ///
    /// This method is a passthrough to the [`REPLState`] which stores the message endpoints.
    pub fn sim_connect<CPU>(&mut self, env: &mut SimEnvironment<CPU>)
    where
        CPU: CPUTraits + DeviceTraits<CPU> + Send + 'static,
    {
        self.state.sim_connect(env)
    }

    /// Testing interface to add message endpoints only to the CLI
    ///
    /// There are a few test harnesses where the CLI interacts with a mock simulator, not a simulator's
    /// environment. This creates the message endpoints between the CLI and the mock simulator.
    ///
    /// # Returns
    /// The `(send, recv)` message endpoints to the mock simulator.
    pub fn mock_connect(&mut self) -> (Sender<SimResponse>, Receiver<SimRequest>) {
        self.state.mock_connect()
    }

    pub fn inputs(&self) -> &CmdReader {
        &self.inputs
    }

    pub fn inputs_mut(&mut self) -> &mut CmdReader {
        &mut self.inputs
    }

    /// Read and parse the next command token
    pub fn next_command(&mut self) -> Result<CmdToken, CLIError> {
        loop {
            // Read a line
            // let current_source = self.inputs.last_mut();
            let line = match self
                .inputs
                .read_logical_line(&self.prompt, self.state.transcript_sink.as_ref())
            {
                // Empty input stack.
                Ok(None) => return Ok(CmdToken::Eof),
                Ok(Some(line)) => line,
                Err(err) => return Err(err.into()),
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with('!') {
                let shell_cmd = trimmed[1..].trim_start().to_string();
                return Ok(CmdToken::ShellCommand(shell_cmd));
            }

            let span = Span::new_with_line(line.as_str(), self.inputs.get_lineno());

            match command_line(span) {
                Ok((_, Some((cmd, rest)))) => {
                    let mut context = CmdContext {
                        state: &mut self.state,
                    };

                    match self
                        .commands
                        .execute_command(cmd.as_str(), span, rest, &mut context)
                    {
                        Ok(()) => return Ok(CmdToken::ExecutedCommand),
                        Err(err) => return Err(err),
                    }
                }
                Ok((_, None)) => {
                    // Comment-only line, continue loop to read next
                    continue;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// The default entry point for the CLI REPL
    ///
    /// Sets the CLI status to [`InterpState::Operating`], pushes a [`std::io::stdin`] reader onto
    /// the CLI reader stack and enters the REPL loop.
    pub fn run(&mut self) -> Result<(), SimError> {
        println!("");
        println!("{}", self.sim_banner());
        println!("");

        self.state.cli_status = InterpState::Operating;
        self.stdin_reader()?;
        self.repl_loop()
    }

    /// The actual REPL loop
    ///
    /// Reads commands from the [`CmdReader`] input stack until the input stack is empty.
    /// The input stack itself may be emptied if the quit command sets the CLI status to
    /// [`InterpState::Quitting`] state, which empties the input stack and sets the CLI
    /// status to [`InterpState::Completed`].
    fn repl_loop(&mut self) -> Result<(), SimError> {
        while self.state.cli_status != InterpState::Completed {
            // Get the current input source (peek, don't pop yet)
            match self.next_command() {
                Ok(token) => {
                    match token {
                        CmdToken::Eof => {
                            self.inputs.pop();
                            if self.inputs.is_empty() {
                                self.state.cli_status = InterpState::Completed
                            }
                        }
                        CmdToken::ShellCommand(_cmd) => {
                            // Execute shell command
                            // self.execute_shell_command(&cmd)?;
                        }
                        CmdToken::ExecutedCommand => {
                            // Command executed in self.next_command().
                        }
                    }
                }
                Err(err) => {
                    eprintln!("%SIM-ERROR: {}", err);
                    continue;
                }
            }

            if self.state.cli_status == InterpState::Quitting {
                // QUIT command: pop everything
                self.inputs.clear();
                self.state.cli_status = InterpState::Completed;
                // Send the quit message, but don't bother to stick around for a result.
                self.state.send(SimRequest::Quit).ok();
            }
        }

        Ok(())
    }

    /// Push a standard input command reader onto the input stack.
    ///
    /// N.B.: There is always a standard input command reader pushed onto the bottom of the
    /// [`CmdREPL::inputs`] stack in [`CmdREPL::run`].
    pub fn stdin_reader(&mut self) -> std::io::Result<()> {
        self.inputs.from_stdin()?;
        Ok(())
    }

    /// Push a new string vector command reader onto the input stack.
    pub fn stringvec_reader(&mut self, input: Vec<String>) -> std::io::Result<()> {
        self.inputs.from_string_vec(input)?;
        Ok(())
    }

    /// TODO: File reader.

    pub fn pop_input(&mut self) -> () {
        self.inputs.pop();
    }

    /// Get the simulation environment's default numeric input radix.
    pub fn input_radix(&self) -> InputRadix {
        self.state.input_radix()
    }

    /// Set the simulation environment's default numeric input radix.
    pub fn set_input_radix(&mut self, radix: InputRadix) -> &mut Self {
        self.state.set_input_radix(radix);
        self
    }

    pub fn set_address_format(&mut self, address_format: fn(usize) -> String) -> &mut Self {
        self.state.set_address_format(address_format);
        self
    }

    pub fn sim_banner(&self) -> String {
        format!(
            "{} simulator SIMH-RS V{}.{}-{} {}        git commid id: {}",
            self.simulator_name,
            version::SIMH_VERSION_MAJOR,
            version::SIMH_VERSION_MINOR,
            version::SIMH_VERSION_PATCH,
            version::SIMH_VERSION_MODE,
            version::SIMH_GIT_HASH,
        )
        .to_string()
    }
}

impl Debug for CmdREPL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceResourceAccessor")
            .field("commands", &self.commands)
            .field("prompt", &self.prompt)
            .field("inputs", &self.inputs)
            .field("cli_status", &self.state.cli_status)
            .field("input_radix", &self.state.input_radix)
            .finish()
    }
}
