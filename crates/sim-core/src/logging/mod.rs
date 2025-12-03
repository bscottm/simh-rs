// SPDX-License-Identifier: MIT

#![allow(rustdoc::private_intra_doc_links)]

mod debug;
mod logging;

// Debug registry — used everywhere categories are checked or registered
pub use debug::{debug_registry, DebugCategory, DebugRegistry};

// Expose the core debugging capabilites
pub use debug::core;

// Sink construction — used by set_cmd.rs
pub use logging::{open_sink, stderr_sink, stdout_sink};

// Core types — used by REPLState, SimEnvironment, set_cmd.rs
pub use logging::{
    new_debug_snapshot, DebugFormatFlags, DebugSnapshot, DebugState, LogDestination, LogError, LogType,
    SharedDebugSnapshot, SharedDebugState, SharedSink, SharedTranscriptSink, TranscriptSink,
};

// Needed by sim_debug! / cli_debug! macros via $crate::logging::micros_since_startup
pub use logging::micros_since_startup;
