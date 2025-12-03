// SPDX-License-Identifier: MIT

//! Command line Read-Eval-Print Loop (REPL) state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::BuildHasherDefault;
use std::io::Write as IOWrite;
use std::rc::Rc;
use std::sync::{
    mpsc::{self, Receiver, RecvError, SendError, Sender},
    Arc,
};

use rustc_hash::FxHasher;

use crate::{
    cli::{
        cli_error::CLIError, parsers::InputRadix, set_cmd::SetCommandTable, show_cmd::ShowCommandTable,
        span::Span,
    },
    env::{
        new_console, CPUTraits, DeviceTraits, ResourceCLIMetadata, ResourceMetadata, SimConsole,
        SimEnvironment, SimRequest, SimResponse,
    },
    logging::{LogDestination, SharedDebugState, SharedTranscriptSink},
};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

pub struct REPLState {
    /// Device name → resource metadata lookup table (top-level entries only; units are nested
    /// inside `ResourceCLIMetadata::units`).
    pub manifest: ManifestMapping,
    /// Message connection to the simulator.
    pub sim_connection: Option<CLISimConnection>,
    /// Interpreter status.
    pub cli_status: InterpState,
    /// Default input radix when a number has no explicit base prefix.
    pub input_radix: InputRadix,
    /// Address formatting function
    pub address_format: fn(usize) -> String,
    /// Output sink for commands (stdout normally; redirectable for tests).
    pub output_sink: Rc<RefCell<dyn IOWrite>>,
    /// SET sub-command dispatch table.
    pub set_table: SetCommandTable,
    /// SHOW sub-command dispatch table.
    pub show_table: ShowCommandTable,
    // Debugging and console transcript logging:
    pub debug_log: Option<LogDestination>,
    pub debug_state: Option<SharedDebugState>,
    pub transcript_log: Option<LogDestination>,
    pub transcript_sink: Option<SharedTranscriptSink>,
}

/// Device name → `ResourceCLIMetadata` map (FxHashMap for speed).
pub type ManifestMapping = HashMap<String, ResourceCLIMetadata, BuildHasherDefault<FxHasher>>;

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// Result of a successful [`REPLState::resolve_resource`] call.
///
/// Contains everything needed to build an [`crate::env::ExamineRequest`] or `Deposit` message.
/// Numeric resource IDs are intentionally absent — the simulator resolves them from the
/// `(device_name, resource_name)` pair via its own `resource_index`.
pub struct ResolvedResource {
    /// Device or unit metadata snapshot.
    pub device: ResourceCLIMetadata,
    /// Resource metadata snapshot.
    pub resource: ResourceMetadata,
    /// First element index (0 for scalars; set by slice notation).
    pub start_offset: usize,
    /// Number of elements to examine.
    pub count: usize,
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// CLI operating state.
#[derive(Debug, PartialEq)]
pub enum InterpState {
    /// Reading and executing commands normally.
    Operating,
    /// QUIT received — drain the input stack and set `Completed`.
    Quitting,
    /// Input stack exhausted.
    Completed,
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

impl REPLState {
    pub fn new(manifest: ManifestMapping, output_sink: Rc<RefCell<dyn IOWrite>>) -> Self {
        Self {
            manifest,
            sim_connection: None,
            cli_status: InterpState::Operating,
            input_radix: InputRadix::Dec,
            address_format: default_address_formatter,
            output_sink,
            set_table: SetCommandTable::new(),
            show_table: ShowCommandTable::new(),
            debug_log: None,
            debug_state: None,
            transcript_log: None,
            transcript_sink: None,
        }
    }

    // ── connection ─────────────────────────────────────────────────────────────

    /// Wire up to the simulator's message endpoints and console queues.
    pub fn sim_connect<CPU>(&mut self, env: &mut SimEnvironment<CPU>)
    where
        CPU: CPUTraits + DeviceTraits<CPU> + Send + 'static,
    {
        let (cli_tx, sim_rx) = mpsc::channel();
        let (sim_tx, cli_rx) = mpsc::channel();
        let console = new_console();
        env.cli_connect(sim_tx, sim_rx, &console);
        self.sim_connection = Some(CLISimConnection::new(cli_tx, cli_rx, Arc::clone(&console)));
    }

    /// Create mock endpoints for tests that drive the CLI without a real simulator.
    pub fn mock_connect(&mut self) -> (Sender<SimResponse>, Receiver<SimRequest>) {
        let (cli_tx, sim_rx) = mpsc::channel();
        let (sim_tx, cli_rx) = mpsc::channel();
        let console = new_console();
        self.sim_connection = Some(CLISimConnection::new(cli_tx, cli_rx, Arc::clone(&console)));
        (sim_tx, sim_rx)
    }

    // ── Internal state setters ─────────────────────────────────────────────────

    pub fn input_radix(&self) -> InputRadix {
        self.input_radix
    }

    pub fn set_input_radix(&mut self, radix: InputRadix) {
        self.input_radix = radix
    }

    pub fn set_address_format(&mut self, address_format: fn(usize) -> String) {
        self.address_format = address_format
    }

    // ── device / resource lookup ───────────────────────────────────────────────

    /// `true` if `dev_name` names a top-level device, controller, or nested unit.
    pub fn valid_device(&self, dev_name: &str) -> bool {
        self.get_device_meta(dev_name).is_some()
    }

    /// Find device metadata by name, searching top-level entries and nested units.
    pub fn get_device_meta(&self, name: &str) -> Option<&ResourceCLIMetadata> {
        let upper = name.to_uppercase();

        if let Some(meta) = self.manifest.get(&upper) {
            return Some(meta);
        }

        for meta in self.manifest.values() {
            if let Some(unit) = meta.get_unit(&upper) {
                return Some(unit);
            }
        }
        None
    }

    /// Find metadata for a specific resource on a specific device or unit.
    ///
    /// Useful when re-formatting output after an examine response — the caller already
    /// knows both names.
    pub fn get_resource_meta(&self, device_name: &str, resource_name: &str) -> Option<&ResourceMetadata> {
        let res_upper = resource_name.to_uppercase();
        let dev = self.get_device_meta(device_name)?;
        dev.resources.iter().find(|r| r.name == res_upper)
    }

    /// Resolve a `(resource_name, optional_device)` pair into a [`ResolvedResource`].
    ///
    /// # Errors
    /// - `InvalidDevice` — device specified but not found.
    /// - `UnknownResource` — resource not found.
    /// - `AmbiguousResource` — resource name appears in more than one device.
    pub fn resolve_resource(
        &self,
        span: Span<'_>,
        ctx: &'static str,
        res_name: &str,
        opt_device: Option<&str>,
    ) -> Result<ResolvedResource, CLIError> {
        let res_upper = res_name.to_uppercase();

        // ── targeted search ───────────────────────────────────────────────────
        if let Some(dev_name) = opt_device {
            let dev = self
                .get_device_meta(dev_name)
                .ok_or_else(|| CLIError::invalid_device(span, ctx, dev_name))?;

            let res = dev
                .resources
                .iter()
                .find(|r| r.name == res_upper)
                .ok_or_else(|| CLIError::unknown_resource(span, ctx, &res_upper, Some(dev_name)))?;

            return Ok(ResolvedResource {
                device: dev.clone(),
                resource: res.clone(),
                start_offset: 0,
                count: res.length,
            });
        }

        // ── global search ─────────────────────────────────────────────────────
        let all_entries = self.manifest.values().flat_map(|meta| {
            let top = meta.resources.iter().map(move |r| (meta, r));
            let units = meta
                .units
                .iter()
                .flat_map(move |unit| unit.resources.iter().map(move |r| (unit, r)));
            top.chain(units)
        });

        let mut matches = all_entries.filter(|(_, r)| r.name == res_upper);

        match (matches.next(), matches.next()) {
            (Some((dev, res)), None) => Ok(ResolvedResource {
                device: dev.clone(),
                resource: res.clone(),
                start_offset: 0,
                count: res.length,
            }),
            (None, _) => Err(CLIError::unknown_resource(span, ctx, &res_upper, None)),
            (Some(first), Some(second)) => {
                let mut names = vec![first.0.name.to_uppercase(), second.0.name.to_uppercase()];
                names.extend(matches.map(|(d, _)| d.name.to_uppercase()));
                Err(CLIError::ambiguous_resource(
                    span,
                    ctx,
                    &res_upper,
                    names.join(", "),
                ))
            }
        }
    }

    // ── simulator channel ──────────────────────────────────────────────────────

    /// Send a request and apply `f` to map the expected success response to `T`.
    pub fn transact_apply<F, T>(
        &self,
        span: Span<'_>,
        ctx: &'static str,
        req: SimRequest,
        f: F,
    ) -> Result<T, CLIError>
    where
        F: FnOnce(SimResponse) -> Option<T>,
    {
        let connection = self
            .sim_connection
            .as_ref()
            .ok_or_else(|| CLIError::simulator_disconnected(span, ctx))?;

        connection
            .send(req)
            .map_err(|_| CLIError::simulator_disconnected(span, ctx))?;

        match connection.recv() {
            Ok(SimResponse::Error(sim_err)) => Err(sim_err.into()),
            Ok(resp) => f(resp.clone())
                .ok_or_else(|| CLIError::message(span, ctx, format!("Unexpected response: {:#?}", resp))),
            Err(_) => Err(CLIError::simulator_disconnected(span, ctx)),
        }
    }

    /// Send a request and ignore the response beyond checking for errors.
    pub fn transact(&self, span: Span<'_>, ctx: &'static str, req: SimRequest) -> Result<(), CLIError> {
        self.transact_apply(span, ctx, req, |_| Some(()))
    }

    /// Send a message without waiting for a response.
    pub fn send(&mut self, msg: SimRequest) -> Result<(), CLIError> {
        let connection = self
            .sim_connection
            .as_ref()
            .ok_or_else(|| CLIError::simulator_disconnected(Default::default(), "REPLState::send()"))?;

        connection
            .sim_tx
            .send(msg)
            .map_err(|_| CLIError::simulator_disconnected(Default::default(), "REPLState::send()"))
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~

/// CLI side of the CLI ↔ simulator message channel.
pub struct CLISimConnection {
    pub sim_tx: Sender<SimRequest>,
    pub sim_rx: Receiver<SimResponse>,
    pub console: SimConsole,
}

impl CLISimConnection {
    pub fn new(sim_tx: Sender<SimRequest>, sim_rx: Receiver<SimResponse>, console: SimConsole) -> Self {
        Self {
            sim_tx,
            sim_rx,
            console,
        }
    }

    pub fn send(&self, msg: SimRequest) -> Result<(), SendError<SimRequest>> {
        self.sim_tx.send(msg)
    }

    pub fn recv(&self) -> Result<SimResponse, RecvError> {
        self.sim_rx.recv()
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Default address formatter
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// The default address formatter
///
/// Outputs addresses as 8-digit hex addresses.
fn default_address_formatter(addr: usize) -> String {
    format!("{:#08x}", addr)
}
