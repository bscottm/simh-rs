// SPDX-License-Identifier: MIT

//! Protocol messages between the simulator and the CLI.
//!
//! # Resource addressing
//!
//! Resources are addressed by `(device_name, resource_name)` string pairs.  The
//! simulator resolves them to `(local_id, unit_index)` via its `resource_index` map.

use crate::env::simerror::SimError;
use crate::env::AttachmentResource;
use crate::logging::{SharedDebugSnapshot, SharedDebugState};

/// A single examine request: read `count` elements starting at `start` from the named resource
/// on the named device or unit.
#[derive(Debug, Clone)]
pub struct ExamineRequest {
    /// Device or unit name (case-insensitive).
    pub device_name: String,
    /// Resource name within that device (case-insensitive).
    pub resource_name: String,
    /// Index of the first element to read (0 for scalars).
    pub start: usize,
    /// Number of elements to read (1 for scalars).
    pub count: usize,
    /// If `true` and the CPU implements [`crate::env::CPUTraits::disassemble`], the
    /// simulator returns [`ExamineResult::Mnemonics`] instead of [`ExamineResult::Values`].
    /// Only meaningful when the resource is `MEM`; the simulator falls back to values for
    /// all other resources regardless of this flag.
    pub mnemonic: bool,
}

/// The result of a single [`ExamineRequest`].
#[derive(Debug, Clone)]
pub enum ExamineResult {
    /// Raw element values — the normal case.
    Values(Vec<u64>),
    /// Disassembled instruction text, one entry per instruction decoded.
    ///
    /// Each entry is `(address, text)` where `address` is the word address of the
    /// first word consumed by that instruction.  Multi-word instructions advance the
    /// address by more than one between successive entries.
    Mnemonics(Vec<(usize, String)>),
}

/// Commands sent from CLI → Simulator.
#[derive(Debug, Clone)]
pub enum SimRequest {
    /// Batched read: read one or more resources in a single round-trip.
    Examine(Vec<ExamineRequest>),

    /// Write values into a resource.
    Deposit {
        /// Device or unit name.
        device_name: String,
        /// Resource name.
        resource_name: String,
        /// Index of the first element to write.
        start: usize,
        /// Values to write (one per element).
        values: Vec<u64>,
    },

    /// Attach a file / network resource to a device or unit.
    Attach {
        device_name: String,
        resource: AttachmentResource,
    },

    /// Detach whatever is currently attached from a device or unit.
    Detach { device_name: String },

    /// Load a file into memory
    LoadFile { flags: Vec<char>, path: String },

    /// Transition to 'running' and execute N instructions (0 = unlimited).
    Step(usize),

    /// Emergency stop: transition from 'running' to 'paused'.
    Stop,

    /// Reset a specific device by name, or the whole system if `None`.
    Reset(Option<String>),

    /// Schedule a device service call after `delay` instructions.
    ScheduleDevice { name: String, delay: i64 },

    /// Enable debug logging.
    SetDebugState(SharedDebugState, Option<SharedDebugSnapshot>),

    /// Disable debug logging.
    DebugDisable,

    /// Shut down the simulator thread.
    Quit,
}

/// Responses sent from Simulator → CLI.
#[derive(Debug, Clone)]
pub enum SimResponse {
    /// Successful bulk read: one `Result<ExamineResult, SimError>` per `ExamineRequest`.
    ExamineData(Vec<Result<ExamineResult, SimError>>),
    /// Generic success acknowledgement.
    Ok,
    /// Error returned from the simulator.
    Error(SimError),
}
