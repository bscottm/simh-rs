// SPDX-License-Identifier: MIT

//! Module exports for integration testing -- main.rs is assumed to be a standalone entity, but lib.rs exports
//! symbols to the `nullsystem` crate for the integration tests.

pub mod cpu;
pub mod devices;
