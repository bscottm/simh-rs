// SPDX-License-Identifier: MIT

//! Publicly facing SIMH simulator core functionality.

#![allow(rustdoc::private_intra_doc_links)]

pub mod cli;
pub mod env;
pub mod logging;
pub mod timers;

pub use resource_macro::SimResources;
